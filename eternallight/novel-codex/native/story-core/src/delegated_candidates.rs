//! Immutable, task-local candidate branches. They are not manuscript heads,
//! task outputs or proof of delivery; the root must use the normal draft write.
use crate::{RepositoryError, SqliteStoryRepository, TaskRecord};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_delegated_drafts_v1 (
 candidate_id TEXT PRIMARY KEY, task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id),
 caller_thread_id TEXT NOT NULL, parent_thread_id TEXT NOT NULL,
 ordinal INTEGER NOT NULL, previous_candidate_id TEXT, payload_json TEXT NOT NULL,
 content_hash TEXT NOT NULL, request_key TEXT NOT NULL, request_hash TEXT NOT NULL,
 turn_id TEXT NOT NULL, call_id TEXT NOT NULL,
 UNIQUE(task_id,caller_thread_id,request_key), UNIQUE(task_id,caller_thread_id,ordinal)
);
CREATE INDEX IF NOT EXISTS story_delegated_drafts_task_v1 ON story_delegated_drafts_v1(task_id,caller_thread_id,ordinal);
CREATE TABLE IF NOT EXISTS story_delegated_writer_grants_v1 (
 grant_id TEXT PRIMARY KEY, task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id),
 root_thread_id TEXT NOT NULL, child_thread_id TEXT NOT NULL,
 input_revision INTEGER NOT NULL, target_hash TEXT NOT NULL,
 UNIQUE(task_id, child_thread_id)
);
"#;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DelegatedDraftWrite {
    pub payload: Value,
    pub expected_delegated_candidate_id: Option<String>,
    pub idempotency_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CreateTaskRequest, TaskUpdate};

    const CHILD: &str = "11111111-1111-4111-8111-111111111111";
    const OTHER: &str = "22222222-2222-4222-8222-222222222222";

    fn fixture(scope: Value, prior: Option<Value>) -> (SqliteStoryRepository, TaskRecord) {
        let (mut repo, project) = crate::work_modeling_recovery_tests::source_repository();
        if let Some(prior) = prior {
            repo.connection
                .execute(
                    "UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id='source'",
                    [prior.to_string()],
                )
                .unwrap();
        }
        let mut target = json!({"chapterId":"chapter-1","chapterNumber":1,"scope":scope});
        if target["scope"]["type"] == "throughScene" {
            target["sceneOrder"] = target["scope"]["endSceneOrder"].clone();
        }
        let task = repo
            .create_task(CreateTaskRequest {
                task_id: "delegated-parent".into(),
                project_id: project,
                thread_id: "root-thread".into(),
                operation: "generateChapter".into(),
                input_revision: 0,
                model: None,
                target_json: target.to_string(),
                input_manifest_json: json!({"authorIntent":"按当前范围修改正文"}).to_string(),
                capability_hash: "a".repeat(64),
                capability_expires_at: i64::MAX,
            })
            .unwrap();
        (repo, task)
    }
    fn caller(id: &str) -> DelegatedDraftCaller {
        DelegatedDraftCaller {
            caller_thread_id: id.into(),
            parent_thread_id: "root-thread".into(),
            turn_id: "child-turn".into(),
            call_id: "child-call".into(),
        }
    }
    fn payload(body: &str) -> Value {
        json!({"chapterId":"chapter-1","chapterNumber":1,"title":"正文","body":body,"completeness":"full"})
    }
    fn write(payload: Value, key: &str, expected: Option<&str>) -> DelegatedDraftWrite {
        DelegatedDraftWrite {
            payload,
            idempotency_key: key.into(),
            expected_delegated_candidate_id: expected.map(str::to_owned),
        }
    }
    fn grouped(first_locked: bool) -> Value {
        json!({"chapterId":"chapter-1","chapterNumber":1,"title":"正文","completeness":"full","sourcePlanRefs":{"localOnly":true},"body":"可见原文\n\n---\n\n隐藏原文","sceneGroups":[{"id":"one","body":"可见原文","locked":first_locked},{"id":"two","body":"隐藏原文","locked":true}]})
    }

    #[test]
    fn explicit_grant_is_bound_to_actual_child_target_and_direct_lineage() {
        let (mut repo, task) = fixture(json!({"type":"wholeChapter"}), None);
        assert!(matches!(
            repo.save_delegated_draft(
                &task,
                &caller(CHILD),
                write(payload("隔离正文"), "first", None)
            ),
            Err(RepositoryError::TaskOutputForbidden)
        ));
        for invalid in ["root-thread", "invented-role", ""] {
            assert!(repo.authorize_delegated_writer(&task, invalid).is_err());
        }
        let grant = repo.authorize_delegated_writer(&task, CHILD).unwrap();
        assert_eq!(
            repo.authorize_delegated_writer(&task, CHILD).unwrap()["grantId"],
            grant["grantId"]
        );
        let identity = repo.read_delegated_draft(&task, CHILD, None, None).unwrap();
        assert_eq!(identity["callerThreadId"], CHILD);
        assert_eq!(identity["rootThreadId"], "root-thread");
        assert!(
            repo.save_delegated_draft(&task, &caller(OTHER), write(payload("越界"), "other", None))
                .is_err()
        );
        let mut indirect = caller(CHILD);
        indirect.parent_thread_id = OTHER.into();
        assert!(
            repo.save_delegated_draft(&task, &indirect, write(payload("越界"), "indirect", None))
                .is_err()
        );
        let mut changed = payload("越界");
        changed["chapterId"] = json!("chapter-2");
        assert!(
            repo.save_delegated_draft(
                &task,
                &caller(CHILD),
                write(changed, "different-target", None)
            )
            .is_err()
        );
        repo.connection.execute("UPDATE story_tasks_v2 SET target_json=json_set(target_json,'$.povCharacterId','new-pov') WHERE task_id=?1",[&task.task_id]).unwrap();
        assert!(
            repo.save_delegated_draft(
                &task,
                &caller(CHILD),
                write(payload("变化后正文"), "new-target", None)
            )
            .is_err()
        );
    }

    #[test]
    fn immutable_candidate_cas_and_replay_never_change_manuscript_or_task_outputs() {
        let (mut repo, task) = fixture(json!({"type":"wholeChapter"}), None);
        let revision = repo.project_revision(&task.project_id).unwrap();
        repo.authorize_delegated_writer(&task, CHILD).unwrap();
        let first = repo
            .save_delegated_draft(
                &task,
                &caller(CHILD),
                write(payload(&"中文正文".repeat(600)), "first", None),
            )
            .unwrap();
        let id = first["candidateId"].as_str().unwrap();
        assert_eq!(
            repo.save_delegated_draft(
                &task,
                &caller(CHILD),
                write(payload(&"中文正文".repeat(600)), "first", None)
            )
            .unwrap()["candidateId"],
            id
        );
        assert!(matches!(
            repo.save_delegated_draft(
                &task,
                &caller(CHILD),
                write(payload("换正文"), "first", None)
            ),
            Err(RepositoryError::IdempotencyConflict)
        ));
        assert!(
            repo.save_delegated_draft(
                &task,
                &caller(CHILD),
                write(payload("换正文"), "second", None)
            )
            .is_err()
        );
        let next = repo
            .save_delegated_draft(
                &task,
                &caller(CHILD),
                write(payload("修订隔离正文"), "second", Some(id)),
            )
            .unwrap();
        assert_ne!(next["candidateId"], id);
        assert!(
            repo.read_delegated_draft(&task, OTHER, Some(id), None)
                .is_err()
        );
        assert_eq!(
            repo.read_delegated_draft(&task, "root-thread", None, None)
                .unwrap()["candidates"][0]["candidateId"],
            next["candidateId"]
        );
        let mut cursor = None;
        let mut raw = String::new();
        loop {
            let page = repo
                .read_delegated_draft(&task, "root-thread", Some(id), cursor)
                .unwrap();
            raw.push_str(page["payloadPart"].as_str().unwrap());
            cursor = page["nextCursor"].as_u64().map(|v| v as usize);
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(
            serde_json::from_str::<Value>(&raw).unwrap()["body"],
            "中文正文".repeat(600)
        );
        assert_eq!(repo.project_revision(&task.project_id).unwrap(), revision);
        let outputs: i64 = repo
            .connection
            .query_row(
                "SELECT COUNT(*) FROM story_task_outputs_v2 WHERE task_id=?1",
                [&task.task_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(outputs, 0);
        let head:String=repo.connection.query_row("SELECT candidate_revision_id FROM story_object_heads_v2 WHERE object_id='draft-chapter-1'",[],|r|r.get(0)).unwrap();
        assert_eq!(head, "source");
    }

    #[test]
    fn pause_and_cancel_fence_late_writes_and_grants() {
        for state in ["paused", "blocked", "cancelled", "completed"] {
            let (mut repo, task) = fixture(json!({"type":"wholeChapter"}), None);
            repo.authorize_delegated_writer(&task, CHILD).unwrap();
            repo.connection.execute("UPDATE story_work_tasks_v1 SET status=?1 WHERE id=(SELECT s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE a.attempt_id=?2)",params![state,task.task_id]).unwrap();
            assert!(
                repo.save_delegated_draft(
                    &task,
                    &caller(CHILD),
                    write(payload("迟到正文"), "late", None)
                )
                .is_err(),
                "{state}"
            );
            assert!(
                repo.authorize_delegated_writer(&task, OTHER).is_err(),
                "{state}"
            );
        }
        let (mut repo, task) = fixture(json!({"type":"wholeChapter"}), None);
        repo.authorize_delegated_writer(&task, CHILD).unwrap();
        let request = write(payload("已保存隔离正文"), "first", None);
        repo.save_delegated_draft(&task, &caller(CHILD), request.clone())
            .unwrap();
        repo.update_task(
            &task.project_id,
            &task.task_id,
            &task.thread_id,
            TaskUpdate {
                status: "cancelled".into(),
                error: None,
                result_summary: None,
                now: 3,
            },
        )
        .unwrap();
        assert!(
            repo.save_delegated_draft(&task, &caller(CHILD), request)
                .is_err()
        );
        assert!(repo.authorize_delegated_writer(&task, OTHER).is_err());
    }

    #[test]
    fn whole_chapter_candidates_preserve_locked_groups() {
        let prior = grouped(true);
        let (mut repo, task) = fixture(json!({"type":"wholeChapter"}), Some(prior.clone()));
        repo.authorize_delegated_writer(&task, CHILD).unwrap();
        assert!(
            repo.save_delegated_draft(
                &task,
                &caller(CHILD),
                write(payload("删掉受保护正文"), "delete", None)
            )
            .is_err()
        );
        let mut changed = prior.clone();
        changed["sceneGroups"][0]["body"] = json!("修改锁定段");
        changed["body"] = json!("修改锁定段\n\n---\n\n隐藏原文");
        assert!(
            repo.save_delegated_draft(&task, &caller(CHILD), write(changed, "change", None))
                .is_err()
        );
        assert!(
            repo.save_delegated_draft(&task, &caller(CHILD), write(prior, "preserve", None))
                .is_ok()
        );
    }

    #[test]
    fn prefix_candidates_cannot_write_or_return_hidden_suffix() {
        let prior = grouped(false);
        let (mut repo, task) = fixture(
            json!({"type":"throughScene","endSceneOrder":1}),
            Some(prior.clone()),
        );
        repo.authorize_delegated_writer(&task, CHILD).unwrap();
        assert!(
            repo.save_delegated_draft(&task, &caller(CHILD), write(prior.clone(), "full", None))
                .is_err()
        );
        let mut visible = prior;
        visible["sceneGroups"].as_array_mut().unwrap().truncate(1);
        visible["sceneGroups"][0]["body"] = json!("改写可见段");
        visible["body"] = json!("改写可见段");
        let saved = repo
            .save_delegated_draft(
                &task,
                &caller(CHILD),
                write(visible.clone(), "visible", None),
            )
            .unwrap();
        let raw = repo
            .delegated_draft_payload(&task, saved["candidateId"].as_str().unwrap())
            .unwrap();
        assert_eq!(raw, visible);
        assert!(!raw.to_string().contains("隐藏原文"));
    }

    #[test]
    fn reopening_current_storage_preserves_grant_candidate_and_cas() {
        let (mut repo, task) = fixture(json!({"type":"wholeChapter"}), None);
        repo.authorize_delegated_writer(&task, CHILD).unwrap();
        let saved = repo
            .save_delegated_draft(
                &task,
                &caller(CHILD),
                write(payload("持久隔离正文"), "save", None),
            )
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("current.sqlite");
        repo.connection
            .execute("VACUUM INTO ?1", [path.to_str().unwrap()])
            .unwrap();
        drop(repo);
        let mut reopened = SqliteStoryRepository::open(&path).unwrap();
        let fresh = reopened
            .task(&task.project_id, &task.task_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            reopened
                .read_delegated_draft(&fresh, CHILD, None, None)
                .unwrap()["candidates"][0]["candidateId"],
            saved["candidateId"]
        );
        assert_eq!(
            reopened
                .save_delegated_draft(
                    &fresh,
                    &caller(CHILD),
                    write(payload("持久隔离正文"), "save", None)
                )
                .unwrap()["candidateId"],
            saved["candidateId"]
        );
        assert!(
            reopened
                .save_delegated_draft(
                    &fresh,
                    &caller(CHILD),
                    write(payload("未读当前头"), "stale", None)
                )
                .is_err()
        );
    }

    #[test]
    fn branch_directory_pages_all_children_and_marks_snapshot_truncation() {
        let (mut repo, task) = fixture(json!({"type":"wholeChapter"}), None);
        let mut last = String::new();
        for index in 0..9 {
            last = format!("33333333-3333-4333-8333-{index:012x}");
            repo.authorize_delegated_writer(&task, &last).unwrap();
            repo.save_delegated_draft(
                &task,
                &caller(&last),
                write(payload("分支候选"), "save", None),
            )
            .unwrap();
        }
        let first = repo
            .read_delegated_draft(&task, &task.thread_id, None, None)
            .unwrap();
        assert_eq!(first["candidates"].as_array().unwrap().len(), 8);
        assert_eq!(first["total"], 9);
        assert_eq!(first["nextCursor"], 8);
        let next = repo
            .read_delegated_draft(&task, &task.thread_id, None, Some(8))
            .unwrap();
        assert_eq!(next["candidates"].as_array().unwrap().len(), 1);
        assert!(next["nextCursor"].is_null());
        let child = repo.read_delegated_draft(&task, &last, None, None).unwrap();
        assert_eq!(child["total"], 1);
        let snapshot = summaries(&repo.connection, &task.task_id, None).unwrap();
        assert_eq!(snapshot["total"], 9);
        assert_eq!(snapshot["truncated"], true);
        assert!(
            repo.read_delegated_draft(&task, &task.thread_id, None, Some(100))
                .is_err()
        );
    }
}
#[derive(Clone, Debug)]
pub struct DelegatedDraftCaller {
    pub caller_thread_id: String,
    pub parent_thread_id: String,
    pub turn_id: String,
    pub call_id: String,
}
fn invalid(message: &str) -> RepositoryError {
    RepositoryError::InvalidContent(message.into())
}
fn storage(e: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(e.to_string())
}
fn hash(v: &str) -> String {
    format!("{:x}", Sha256::digest(v.as_bytes()))
}
fn identity(v: &str) -> bool {
    !v.is_empty() && v.len() <= 160 && !v.chars().any(char::is_control)
}
fn authorize(db: &rusqlite::Connection, task: &TaskRecord) -> Result<TaskRecord, RepositoryError> {
    let fresh = crate::tasks::load_task(db, &task.task_id)?;
    if fresh.project_id != task.project_id
        || fresh.thread_id != task.thread_id
        || fresh.operation != "generateChapter"
        || !matches!(fresh.status.as_str(), "queued" | "running")
        || fresh.capability_revoked_at.is_some()
    {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    crate::task_input::assert_task_inputs_current(db, &fresh)?;
    crate::author_message::validate_execution(db, &fresh)?;
    let fenced:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1 AND w.status IN ('paused','blocked','cancelled','completed'))",[&fresh.task_id],|r|r.get(0)).map_err(storage)?;
    if fenced {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    Ok(fresh)
}

fn writer_grant(
    db: &rusqlite::Connection,
    task: &TaskRecord,
    caller: &str,
) -> Result<String, RepositoryError> {
    let grant:Option<(String,String,i64,String)>=db.query_row("SELECT grant_id,root_thread_id,input_revision,target_hash FROM story_delegated_writer_grants_v1 WHERE task_id=?1 AND child_thread_id=?2",params![task.task_id,caller],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(storage)?;
    match grant {
        Some((id, root, revision, target))
            if root == task.thread_id
                && revision == task.input_revision
                && target == hash(&task.target_json) =>
        {
            Ok(id)
        }
        _ => Err(RepositoryError::TaskOutputForbidden),
    }
}

fn validate_payload(
    db: &rusqlite::Connection,
    task: &TaskRecord,
    payload: &Value,
) -> Result<(), RepositoryError> {
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    if payload.to_string().len() > 600_000
        || payload["chapterId"] != target["chapterId"]
        || payload["chapterNumber"] != target["chapterNumber"]
        || payload["title"]
            .as_str()
            .is_none_or(|v| v.chars().count() > 160)
        || payload["body"]
            .as_str()
            .is_none_or(|v| v.chars().count() > 120_000)
        || payload["completeness"]
            .as_str()
            .is_none_or(|v| !identity(v))
    {
        return Err(invalid(
            "Isolated draft must match the admitted chapter and the bounded DraftPayload contract",
        ));
    }
    if let Some(groups) = payload["sceneGroups"].as_array() {
        let mut ids = std::collections::BTreeSet::new();
        if groups.len() > 100
            || groups.iter().any(|g| {
                g["id"]
                    .as_str()
                    .is_none_or(|v| !identity(v) || !ids.insert(v))
                    || g["body"].as_str().is_none()
                    || g["locked"].as_bool().is_none()
            })
        {
            return Err(invalid(
                "Isolated scene groups need unique IDs, body and locked metadata",
            ));
        }
        let body = groups
            .iter()
            .map(|g| g["body"].as_str().unwrap().trim())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");
        if payload["body"] != body {
            return Err(invalid(
                "Isolated draft body must equal canonical scene group text",
            ));
        }
    }
    // Validate with the same Native scope/locked-scene authority as a formal
    // draft. A prefix writer submits only visible groups; merge the hidden
    // suffix privately for validation, never into the returned candidate.
    let mut scoped = payload.clone();
    if let Some(end) = crate::task_scope::writer_scene_limit(&target)? {
        let groups = scoped["sceneGroups"]
            .as_array_mut()
            .ok_or(RepositoryError::TaskOutputForbidden)?;
        if groups.len() > end {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        if let Some(prior) = crate::protected_scenes::draft_base(db, task)? {
            if payload["title"] != prior["title"] {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            let before = prior["sceneGroups"]
                .as_array()
                .ok_or(RepositoryError::CorruptState)?;
            if before.len() > end {
                if groups.len() != end {
                    return Err(RepositoryError::TaskOutputForbidden);
                }
                groups.extend(before.iter().skip(end).cloned());
            }
            scoped["sourcePlanRefs"] = prior["sourcePlanRefs"].clone();
        }
        scoped["body"] = json!(
            scoped["sceneGroups"]
                .as_array()
                .unwrap()
                .iter()
                .map(|g| g["body"].as_str().unwrap().trim())
                .collect::<Vec<_>>()
                .join("\n\n---\n\n")
        );
    }
    crate::task_scope::validate_scoped_write(db, task, &scoped.to_string())?;
    Ok(())
}

pub(crate) fn summaries(
    db: &rusqlite::Connection,
    task_id: &str,
    caller: Option<&str>,
) -> Result<Value, RepositoryError> {
    summary_page(db, task_id, caller, 0)
}
fn summary_page(
    db: &rusqlite::Connection,
    task_id: &str,
    caller: Option<&str>,
    offset: usize,
) -> Result<Value, RepositoryError> {
    let total:usize=db.query_row("SELECT COUNT(DISTINCT caller_thread_id) FROM story_delegated_drafts_v1 WHERE task_id=?1 AND (?2 IS NULL OR caller_thread_id=?2)",params![task_id,caller],|r|r.get(0)).map_err(storage)?;
    if offset > total || (offset % 8 != 0 && offset != total) {
        return Err(invalid("Use a returned branch directory nextCursor"));
    }
    let mut q=db.prepare("SELECT candidate_id,caller_thread_id,ordinal,content_hash,payload_json FROM story_delegated_drafts_v1 d WHERE task_id=?1 AND (?2 IS NULL OR caller_thread_id=?2) AND NOT EXISTS(SELECT 1 FROM story_delegated_drafts_v1 n WHERE n.task_id=d.task_id AND n.caller_thread_id=d.caller_thread_id AND n.ordinal>d.ordinal) ORDER BY caller_thread_id LIMIT 8 OFFSET ?3").map_err(storage)?;
    let items = q
        .query_map(params![task_id, caller, offset], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(storage)?;
    let mut result = Vec::new();
    for row in items {
        let (id, caller, ordinal, digest, payload) = row.map_err(storage)?;
        let p: Value = serde_json::from_str(&payload).map_err(|_| RepositoryError::CorruptState)?;
        result.push(json!({"candidateId":id,"callerThreadId":caller,"version":ordinal,"contentHash":digest,
            "chapterId":p["chapterId"],"title":p["title"],"state":"isolated","adopted":false,
            "characterCount":p["body"].as_str().unwrap_or_default().chars().filter(|c|!c.is_whitespace()).count()}));
    }
    let end = offset + result.len();
    Ok(
        json!({"items":result,"total":total,"truncated":end<total,"nextCursor":(end<total).then_some(end)}),
    )
}

impl SqliteStoryRepository {
    /// Runtime authenticates the root caller before invoking this authority.
    /// The child must later present trusted direct-child lineage and this exact
    /// caller identity. A role label, message or inherited tool list is no grant.
    pub fn authorize_delegated_writer(
        &mut self,
        task: &TaskRecord,
        child_thread_id: &str,
    ) -> Result<Value, RepositoryError> {
        let result=self.authorize_delegated_writer_inner(task,child_thread_id);
        crate::recovery_operations::settle_prewrite_rejection(self,task,"authorize_delegated_writer",&result)?;
        result
    }
    fn authorize_delegated_writer_inner(&mut self,task:&TaskRecord,child_thread_id:&str) -> Result<Value,RepositoryError> {
        if !identity(child_thread_id)
            || child_thread_id == task.thread_id
            || uuid::Uuid::parse_str(child_thread_id).is_err()
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let fresh = authorize(&tx, task)?;
        let existing:Option<String>=tx.query_row("SELECT grant_id FROM story_delegated_writer_grants_v1 WHERE task_id=?1 AND child_thread_id=?2",params![fresh.task_id,child_thread_id],|r|r.get(0)).optional().map_err(storage)?;
        if let Some(id) = existing {
            writer_grant(&tx, &fresh, child_thread_id)?;
            crate::recovery_operations::commit_operation(&tx,self.recovery_operation.as_ref(),&fresh.project_id,&fresh.task_id,
                &["authorize_delegated_writer"],&format!("runtime:delegated-writer-grant:{id}"))?;
            tx.commit().map_err(storage)?;
            return Ok(
                json!({"grantId":id,"childThreadId":child_thread_id,"state":"authorized","replayed":true,"manuscriptChanged":false}),
            );
        }
        let id = format!("delegate-grant-{}", uuid::Uuid::new_v4().simple());
        tx.execute(
            "INSERT INTO story_delegated_writer_grants_v1 VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                id,
                fresh.task_id,
                fresh.thread_id,
                child_thread_id,
                fresh.input_revision,
                hash(&fresh.target_json)
            ],
        )
        .map_err(storage)?;
        crate::recovery_operations::commit_operation(&tx,self.recovery_operation.as_ref(),&fresh.project_id,&fresh.task_id,
            &["authorize_delegated_writer"],&format!("runtime:delegated-writer-grant:{id}"))?;
        tx.commit().map_err(storage)?;
        Ok(
            json!({"grantId":id,"childThreadId":child_thread_id,"state":"authorized","manuscriptChanged":false,"guidance":"Only this actual child may save isolated candidates for this task. Source scope and cumulative budget remain the parent's. No formal save, completion, adoption or publication authority is granted."}),
        )
    }

    pub fn save_delegated_draft(
        &mut self,
        task: &TaskRecord,
        caller: &DelegatedDraftCaller,
        request: DelegatedDraftWrite,
    ) -> Result<Value, RepositoryError> {
        let result=self.save_delegated_draft_inner(task,caller,request);
        crate::recovery_operations::settle_prewrite_rejection(self,task,"save_delegated_draft",&result)?;
        result
    }
    fn save_delegated_draft_inner(&mut self,task:&TaskRecord,caller:&DelegatedDraftCaller,request:DelegatedDraftWrite) -> Result<Value,RepositoryError> {
        for v in [
            &caller.caller_thread_id,
            &caller.parent_thread_id,
            &caller.turn_id,
            &caller.call_id,
            &request.idempotency_key,
        ] {
            if !identity(v) {
                return Err(RepositoryError::TaskBindingInvalid);
            }
        }
        // Restrict candidate writers to direct children of the admitted root.
        if caller.caller_thread_id == task.thread_id || caller.parent_thread_id != task.thread_id {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let fresh = authorize(&tx, task)?;
        writer_grant(&tx, &fresh, &caller.caller_thread_id)?;
        validate_payload(&tx, &fresh, &request.payload)?;
        let digest =
            hash(&serde_json::to_string(&request).map_err(|_| RepositoryError::CorruptState)?);
        let replay:Option<(String,String)>=tx.query_row("SELECT candidate_id,request_hash FROM story_delegated_drafts_v1 WHERE task_id=?1 AND caller_thread_id=?2 AND request_key=?3",params![task.task_id,caller.caller_thread_id,request.idempotency_key],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage)?;
        if let Some((id, prior)) = replay {
            if prior != digest {
                return Err(RepositoryError::IdempotencyConflict);
            }
            tx.commit().map_err(storage)?;
            return Ok(
                json!({"candidateId":id,"writeState":"committed","state":"isolated","replayed":true,"manuscriptChanged":false}),
            );
        }
        let head:Option<(String,i64)>=tx.query_row("SELECT candidate_id,ordinal FROM story_delegated_drafts_v1 WHERE task_id=?1 AND caller_thread_id=?2 ORDER BY ordinal DESC LIMIT 1",params![task.task_id,caller.caller_thread_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(storage)?;
        if head.as_ref().map(|(id, _)| id) != request.expected_delegated_candidate_id.as_ref() {
            return Err(invalid(
                "Isolated candidate changed; read the current branch and retry with its exact expectedDelegatedCandidateId",
            ));
        }
        let count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM story_delegated_drafts_v1 WHERE task_id=?1",
                [&task.task_id],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if count >= 128 {
            return Err(invalid("Task-local candidate branch budget exhausted"));
        }
        let id = format!("delegated-{}", uuid::Uuid::new_v4().simple());
        let content = request.payload.to_string();
        let content_hash = hash(&content);
        let ordinal = head.as_ref().map_or(1, |(_, n)| n + 1);
        tx.execute(
            "INSERT INTO story_delegated_drafts_v1 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                id,
                task.task_id,
                caller.caller_thread_id,
                caller.parent_thread_id,
                ordinal,
                request.expected_delegated_candidate_id,
                content,
                content_hash,
                request.idempotency_key,
                digest,
                caller.turn_id,
                caller.call_id
            ],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(
            json!({"candidateId":id,"writeState":"committed","state":"isolated","version":ordinal,"contentHash":content_hash,"manuscriptChanged":false,
            "guidance":"This is an immutable task-local candidate, not a saved manuscript. Return its candidateId to the parent; only the root can integrate it with save_draft_checkpoint under the original scope and CAS."}),
        )
    }

    pub fn read_delegated_draft(
        &self,
        task: &TaskRecord,
        caller: &str,
        candidate: Option<&str>,
        cursor: Option<usize>,
    ) -> Result<Value, RepositoryError> {
        authorize(&self.connection, task)?;
        if !identity(caller) {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let Some(id) = candidate else {
            let page = summary_page(
                &self.connection,
                &task.task_id,
                (caller != task.thread_id).then_some(caller),
                cursor.unwrap_or(0),
            )?;
            return Ok(
                json!({"callerThreadId":caller,"rootThreadId":task.thread_id,"candidates":page["items"],"total":page["total"],"nextCursor":page["nextCursor"],"truncated":page["truncated"],"state":"isolated","manuscriptChanged":false}),
            );
        };
        let (owner,raw,digest):(String,String,String)=self.connection.query_row("SELECT caller_thread_id,payload_json,content_hash FROM story_delegated_drafts_v1 WHERE task_id=?1 AND candidate_id=?2",params![task.task_id,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage)?.ok_or_else(||invalid("Candidate is not owned by this task"))?;
        if caller != task.thread_id && caller != owner {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        let start = cursor.unwrap_or(0);
        if start > raw.len() || !raw.is_char_boundary(start) {
            return Err(invalid("Use a returned candidate nextCursor"));
        }
        let mut end = (start + 1800).min(raw.len());
        while !raw.is_char_boundary(end) {
            end -= 1;
        }
        Ok(
            json!({"candidateId":id,"callerThreadId":owner,"contentHash":digest,"byteRange":{"start":start,"end":end,"total":raw.len()},
            "payloadPart":&raw[start..end],"nextCursor":(end<raw.len()).then_some(end),"state":"isolated","manuscriptChanged":false}),
        )
    }

    pub fn delegated_draft_payload(
        &self,
        task: &TaskRecord,
        candidate: &str,
    ) -> Result<Value, RepositoryError> {
        authorize(&self.connection, task)?;
        let raw:String=self.connection.query_row("SELECT payload_json FROM story_delegated_drafts_v1 WHERE task_id=?1 AND candidate_id=?2",params![task.task_id,candidate],|r|r.get(0)).optional().map_err(storage)?.ok_or_else(||invalid("Candidate is not owned by this task"))?;
        let value = serde_json::from_str(&raw).map_err(|_| RepositoryError::CorruptState)?;
        validate_payload(&self.connection, task, &value)?;
        Ok(value)
    }
}
