//! Agent self-assessment bound to persisted work; never a literary certificate.
use crate::authoring_runs::storage_error;
use crate::{RepositoryError, TaskRecord};
use rusqlite::{Connection, params};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Review {
    expected_work_version: i64,
    expected_source_digest: String,
    goal_status: String,
    goal_assessment: String,
    checked_scope: String,
    unresolved: Vec<crate::work_issues::UnresolvedIssue>,
    issue_resolutions: Vec<crate::work_issues::IssueResolution>,
    author_decisions: Vec<String>,
    evidence: Vec<Evidence>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Evidence {
    task_id: String,
    revision_id: String,
}
fn invalid(detail: &str) -> RepositoryError {
    RepositoryError::InvalidContent(format!("deliveryReview: {detail}"))
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max
}
pub(crate) fn save(
    db: &Connection,
    task: &TaskRecord,
    work: &str,
    status: &Value,
    value: Option<&Value>,
) -> Result<(), RepositoryError> {
    crate::task_sources::require_author_requirements_read(db, task)?;
    let value = value.ok_or_else(|| invalid("final decision requires a delivery review"))?;
    let review: Review =
        serde_json::from_value(value.clone()).map_err(|_| invalid("invalid review fields"))?;
    if status["version"] != review.expected_work_version {
        return Err(RepositoryError::WorkTaskStale);
    }
    if status["sourceDigest"] != review.expected_source_digest {
        return Err(RepositoryError::TaskInputStale);
    }
    if !matches!(
        review.goal_status.as_str(),
        "fulfilled" | "partial" | "needsAuthorDecision"
    ) || !text(&review.goal_assessment, 1600)
        || !text(&review.checked_scope, 1600)
        || review.unresolved.len() > 16
        || review.author_decisions.len() > 16
        || review.evidence.len() > 1024
        || review.author_decisions.iter().any(|s| !text(s, 600))
    {
        return Err(invalid(
            "review exceeds bounds or has empty required content",
        ));
    }
    if (review.goal_status == "fulfilled"
        && (!review.unresolved.is_empty() || !review.author_decisions.is_empty()))
        || (review.goal_status == "partial" && review.unresolved.is_empty())
        || (review.goal_status == "needsAuthorDecision" && review.author_decisions.is_empty())
    {
        return Err(invalid(
            "goal status must match unresolved work and author decisions",
        ));
    }
    crate::model_work_plan::final_delivery(db, work, &review.goal_status)?;
    if review.goal_status == "fulfilled" {
        crate::model_work_plan::fulfilled(db, work)?;
    }
    let mut refs = BTreeSet::new();
    for reference in &review.evidence {
        if !text(&reference.task_id, 160)
            || !text(&reference.revision_id, 160)
            || !refs.insert((&reference.task_id, &reference.revision_id))
        {
            return Err(invalid("invalid or duplicate evidence"));
        }
        let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id JOIN story_task_outputs_v2 o ON o.task_id=t.task_id WHERE s.work_task_id=?1 AND t.project_id=?2 AND t.task_id=?3 AND t.status='completed' AND o.revision_id=?4)",params![work,task.project_id.as_str(),reference.task_id,reference.revision_id],|r|r.get(0)).map_err(storage_error)?;
        if !valid {
            return Err(invalid("evidence is not a completed output of this Work"));
        }
    }
    // Cover every currently selected revision saved by this Work, not only
    // the last page of receipts; obsolete drafts must not stand in for current text.
    let mut query=db.prepare("SELECT DISTINCT t.task_id,o.revision_id FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id JOIN story_task_outputs_v2 o ON o.task_id=t.task_id JOIN story_object_heads_v2 h ON h.project_id=t.project_id AND COALESCE(h.candidate_revision_id,h.accepted_revision_id)=o.revision_id WHERE s.work_task_id=?1 AND t.project_id=?2 AND t.status='completed' AND (o.output_kind!='draft' OR o.completeness='chapterComplete')").map_err(storage_error)?;
    let required = query
        .query_map(params![work, task.project_id.as_str()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(storage_error)?;
    for item in required {
        let (task_id, revision) = item.map_err(storage_error)?;
        if !refs.contains(&(&task_id, &revision)) {
            return Err(invalid(
                "review must cover all current saved revision outputs",
            ));
        }
    }
    db.execute("INSERT INTO story_work_delivery_reviews_v1(decision_task_id,work_task_id,content_json) VALUES(?1,?2,?3)",params![task.task_id,work,value.to_string()]).map_err(storage_error)?;
    crate::work_issues::record_delivery(db, task, work, &review.unresolved)?;
    crate::work_issues::record_resolutions(db, task, work, &review.issue_resolutions)?;
    if review.goal_status == "fulfilled" {
        crate::work_issues::require_resolved(db, &task.project_id, work, None)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn output(db: &Connection, id: &str, kind: &str) {
        let root: String = db
            .query_row(
                "SELECT task_id FROM story_author_messages_v1 WHERE task_id IS NOT NULL LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let work = format!("work-{root}");
        db.execute("INSERT INTO story_tasks_v2 SELECT ?1,project_id,?1,operation,task_kind,purpose,required_output_kind,target_json,input_manifest_json,input_revision,model,NULL,'completed',stage,'ready_for_review','[]',result_summary,NULL,capability_hash,capability_expires_at,NULL FROM story_tasks_v2 WHERE task_id=?2",params![id,root]).unwrap();
        let ordinal: i64 = db
            .query_row(
                "SELECT COALESCE(MAX(ordinal),0)+1 FROM story_work_steps_v1 WHERE work_task_id=?1",
                [&work],
                |r| r.get(0),
            )
            .unwrap();
        db.execute("INSERT INTO story_work_steps_v1 VALUES(?1,?2,?3,'advisory','{}',0,NULL,'completed',?1)",params![id,work,ordinal]).unwrap();
        db.execute(
            "INSERT INTO story_task_outputs_v2 VALUES(?1,0,?2,?1,NULL,?3)",
            params![id, kind, (kind == "draft").then_some("chapterComplete")],
        )
        .unwrap();
        db.execute(
            "INSERT INTO story_revisions_v2 VALUES(?1,'project',?1,?2,NULL,'digest','{}')",
            params![id, kind],
        )
        .unwrap();
        db.execute(
            "INSERT INTO story_object_heads_v2 VALUES('project',?1,?2,?1,NULL,0)",
            params![id, kind],
        )
        .unwrap();
    }
    fn setup() -> (Connection, TaskRecord, Value) {
        let mut repo = crate::SqliteStoryRepository::in_memory().unwrap();
        let project = crate::ProjectId::parse("project").unwrap();
        repo.create_project(
            crate::ProjectRecord {
                project_id: project.clone(),
                title: "Review".into(),
                logline: "".into(),
                created_at: 1,
                updated_at: 1,
            },
            crate::BranchId::parse("main").unwrap(),
            crate::SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
        let digest = repo.project_source_watermarks(&project).unwrap()["digest"]
            .as_str()
            .unwrap()
            .to_owned();
        let task = repo
            .create_budgeted_root_task(
                crate::CreateTaskRequest {
                    task_id: "decision".into(),
                    project_id: project,
                    thread_id: "thread".into(),
                    operation: "advisory".into(),
                    input_revision: 0,
                    model: None,
                    target_json: json!({"type":"project"}).to_string(),
                    input_manifest_json:
                        json!({"authorIntent":"只修正指定句子，检查真实保存内容与本次授权。"})
                            .to_string(),
                    capability_hash: "a".repeat(64),
                    capability_expires_at: 99999,
                },
                &crate::InitialWorkBudget {
                    authorization_id: "budget".into(),
                    author_message_id: "message".into(),
                    expected_base_snapshot_id: "initial".into(),
                    expected_source_digest: digest,
                    max_requests: None,
                    max_tokens: None,
                    deadline_at: None,
                },
                1,
            )
            .unwrap();
        // The standards gate uses actual Native catalogue/transport, not fabricated receipts.
        let mut context = json!({"coverage":{"complete":false,"missingScope":[]}});
        repo.project_task_sources(&task, &mut context).unwrap();
        let source = context["taskSources"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["sectionPath"] == "/authorRequirementsSource")
            .unwrap()["sourceId"]
            .as_str()
            .unwrap();
        let mut cursor = None;
        loop {
            let page = repo
                .task_source_content(&task, source, cursor.as_deref())
                .unwrap();
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        let db = repo.connection;
        output(&db, "writer", "draft");
        let review = json!({"expectedWorkVersion":2,"expectedSourceDigest":"digest","goalStatus":"fulfilled","goalAssessment":"已按作者要求修正这一句","checkedScope":"仅核对本次句子与上下文，未检查全书","unresolved":[],"issueResolutions":[],"authorDecisions":[],"evidence":[{"taskId":"writer","revisionId":"writer"}]});
        (db, task, review)
    }
    fn status() -> Value {
        json!({"version":2,"sourceDigest":"digest"})
    }
    #[test]
    fn rejects_missing_stale_forged_and_incomplete_reviews_without_writes() {
        let (db, task, review) = setup();
        assert!(
            save(
                &db,
                &task,
                &format!("work-{}", task.task_id),
                &status(),
                None
            )
            .is_err()
        );
        for (field, value) in [
            ("expectedWorkVersion", json!(1)),
            ("expectedSourceDigest", json!("old")),
            ("evidence", json!([])),
            (
                "evidence",
                json!([{"taskId":"foreign","revisionId":"revision"}]),
            ),
            ("unresolved", json!(["尚未改完"])),
        ] {
            let mut invalid = review.clone();
            invalid[field] = value;
            assert!(
                save(
                    &db,
                    &task,
                    &format!("work-{}", task.task_id),
                    &status(),
                    Some(&invalid)
                )
                .is_err(),
                "{field}"
            );
        }
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM story_work_delivery_reviews_v1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        save(
            &db,
            &task,
            &format!("work-{}", task.task_id),
            &status(),
            Some(&review),
        )
        .unwrap();
        let stored: String = db
            .query_row(
                "SELECT content_json FROM story_work_delivery_reviews_v1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(), review);
    }
    #[test]
    fn accepts_explicit_partial_outcome_and_read_only_work_without_fabricated_evidence() {
        let (db, task, mut review) = setup();
        review["goalStatus"] = json!("partial");
        review["unresolved"] = json!([{"issue":{"issueId":"fixture-unfinished-0","newIssue":{"summary":"后续人物冲突仍需另行处理"},"resolutionOf":[]},"observationId":null,"reason":"后续人物冲突仍需另行处理"}]);
        save(
            &db,
            &task,
            &format!("work-{}", task.task_id),
            &status(),
            Some(&review),
        )
        .unwrap();
        assert_eq!(
            crate::work_issues::summary(&db, &task.project_id, &format!("work-{}", task.task_id))
                .unwrap()["totalCount"],
            1
        );
        let (db, task, mut review) = setup();
        db.execute(
            "DELETE FROM story_work_steps_v1 WHERE current_attempt_id!=?1",
            [&task.task_id],
        )
        .unwrap();
        review["evidence"] = json!([]);
        review["goalStatus"] = json!("needsAuthorDecision");
        review["authorDecisions"] = json!(["作者选择保留哪个结局"]);
        save(
            &db,
            &task,
            &format!("work-{}", task.task_id),
            &status(),
            Some(&review),
        )
        .unwrap();
    }
    #[test]
    fn future_method_does_not_waive_saved_evidence_or_create_continuation_authority() {
        let (db, task, mut review) = setup();
        let mut goal = serde_json::json!({"id":"continue","title":"接续","acceptance":"登记真实原范围交接","kind":"continuation","status":"pending","dependsOn":[],"readingQuestion":"","checkScope":"原任务","approach":"前置完成后接回","assessment":"","evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":{"runId":"original-run","proposalId":null}});
        let persist = |version: i64, goal: &Value| {
            db.execute(
                "INSERT INTO story_model_work_plans_v1 VALUES(?1,?2,'writer',?3,?4)",
                params![
                    format!("work-{}",task.task_id),
                    version,
                    version.to_string(),
                    serde_json::json!({"intentMode":"executeGoals","activeGoalId":null,"goals":[goal]}).to_string()
                ],
            )
            .unwrap();
        };
        persist(1, &goal);
        review["goalStatus"] = serde_json::json!("partial");
        review["unresolved"] = serde_json::json!([{"issue":{"issueId":"fixture-unfinished-0","newIssue":{"summary":"原范围接续尚未交付"},"resolutionOf":[]},"observationId":null,"reason":"原范围接续尚未交付"}]);
        review["evidence"] = serde_json::json!([]);
        assert!(
            save(
                &db,
                &task,
                &format!("work-{}", task.task_id),
                &status(),
                Some(&review)
            )
            .is_err()
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM story_work_delivery_reviews_v1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        goal["status"] = serde_json::json!("blocked");
        goal["assessment"] = serde_json::json!("原任务存在未知写入回执，核对前不得重复生成");
        persist(2, &goal);
        review["unresolved"] = serde_json::json!([{"issue":{"issueId":"fixture-unfinished-0","newIssue":{"summary":"原任务写入结果未核对，接续被安全门禁阻止"},"resolutionOf":[]},"observationId":null,"reason":"原任务写入结果未核对，接续被安全门禁阻止"}]);
        review["evidence"] = serde_json::json!([{"taskId":"writer","revisionId":"writer"}]);
        save(
            &db,
            &task,
            &format!("work-{}", task.task_id),
            &status(),
            Some(&review),
        )
        .unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM story_generation_handoffs_v1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0,
            "a method window and partial delivery cannot grant a generation handoff"
        );
    }
    #[test]
    fn current_planning_output_requires_evidence_even_when_prose_is_covered() {
        let (db, task, mut review) = setup();
        output(&db, "planner", "brief");
        assert!(
            save(
                &db,
                &task,
                &format!("work-{}", task.task_id),
                &status(),
                Some(&review)
            )
            .is_err()
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM story_work_delivery_reviews_v1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        review["evidence"]
            .as_array_mut()
            .unwrap()
            .push(json!({"taskId":"planner","revisionId":"planner"}));
        save(
            &db,
            &task,
            &format!("work-{}", task.task_id),
            &status(),
            Some(&review),
        )
        .unwrap();
    }
    #[test]
    fn hundred_chapter_review_covers_all_current_prose_and_optional_reports() {
        let (db, task, mut review) = setup();
        db.execute(
            "DELETE FROM story_work_steps_v1 WHERE current_attempt_id!=?1",
            [&task.task_id],
        )
        .unwrap();
        db.execute(
            "DELETE FROM story_tasks_v2 WHERE task_id!=?1",
            [&task.task_id],
        )
        .unwrap();
        db.execute_batch("DELETE FROM story_task_outputs_v2; DELETE FROM story_object_heads_v2;")
            .unwrap();
        let mut evidence = Vec::new();
        for chapter in 1..=100 {
            for kind in ["draft", "commitmentExtraction", "sceneAssessment"] {
                let id = format!("{kind}-{chapter}");
                output(&db, &id, kind);
                evidence.push(json!({"taskId":id,"revisionId":id}));
            }
        }
        review["evidence"] = json!(evidence);
        let mut incomplete = review.clone();
        incomplete["evidence"].as_array_mut().unwrap().pop();
        assert!(
            save(
                &db,
                &task,
                &format!("work-{}", task.task_id),
                &status(),
                Some(&incomplete)
            )
            .is_err()
        );
        save(
            &db,
            &task,
            &format!("work-{}", task.task_id),
            &status(),
            Some(&review),
        )
        .unwrap();
        let stored: String = db
            .query_row(
                "SELECT content_json FROM story_work_delivery_reviews_v1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&stored).unwrap()["evidence"]
                .as_array()
                .unwrap()
                .len(),
            300
        );
    }
}
