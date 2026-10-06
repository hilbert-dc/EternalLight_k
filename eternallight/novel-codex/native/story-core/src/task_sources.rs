use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    ObjectHead, ObjectId, RepositoryError, RevisionId, SqliteStoryRepository, TaskRecord,
    WorkingBaseline,
};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_task_source_origins_v2 (
 task_id TEXT PRIMARY KEY REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
 snapshot_id TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_task_source_heads_v2 (
 task_id TEXT NOT NULL REFERENCES story_task_source_origins_v2(task_id) ON DELETE CASCADE,
 object_id TEXT NOT NULL, object_kind TEXT NOT NULL, locked INTEGER NOT NULL,
 candidate_revision_id TEXT, accepted_revision_id TEXT, working_revision_id TEXT,
 working_selection_revision INTEGER, PRIMARY KEY(task_id,object_id)
);
CREATE TABLE IF NOT EXISTS story_task_source_knowledge_v2 (
 task_id TEXT NOT NULL REFERENCES story_task_source_origins_v2(task_id) ON DELETE CASCADE,
 object_id TEXT NOT NULL, batch_id TEXT NOT NULL, PRIMARY KEY(task_id,object_id)
);
CREATE TABLE IF NOT EXISTS story_task_source_items_v2 (
 task_id TEXT NOT NULL REFERENCES story_task_source_origins_v2(task_id) ON DELETE CASCADE,
 policy_digest TEXT NOT NULL, section_path TEXT NOT NULL, source_id TEXT NOT NULL UNIQUE,
 content_hash TEXT NOT NULL, payload_json TEXT NOT NULL,
 PRIMARY KEY(task_id,policy_digest,section_path)
);
"#;

// References only: the revision and batch stores remain the content authorities.
pub(crate) fn capture(
    connection: &Connection,
    task: &str,
    project: &crate::ProjectId,
) -> Result<(), RepositoryError> {
    connection.execute("INSERT INTO story_task_source_origins_v2 SELECT ?1,head_snapshot_id FROM story_branches_v2 WHERE project_id=?2 AND branch_id='main'", params![task,project.as_str()]).map_err(storage)?;
    connection.execute("INSERT INTO story_task_source_heads_v2 SELECT i.task_id,i.object_id,h.object_kind,h.locked,i.candidate_revision_id,i.accepted_revision_id,i.working_revision_id,i.working_selection_revision FROM story_task_input_heads_v2 i JOIN story_object_heads_v2 h ON h.project_id=?2 AND h.object_id=i.object_id WHERE i.task_id=?1", params![task,project.as_str()]).map_err(storage)?;
    connection.execute("INSERT INTO story_task_source_knowledge_v2 SELECT task_id,object_id,batch_id FROM story_task_input_knowledge_v2 WHERE task_id=?1", [task]).map_err(storage)?;
    Ok(())
}

impl SqliteStoryRepository {
    /// Current-generation findings are advisory evidence, not permission to expand scope.
    /// The source cache freezes this report under the decision task's existing capability.
    pub fn task_generation_scene_feedback(
        &self,
        task: &TaskRecord,
    ) -> Result<Option<Value>, RepositoryError> {
        let Some(bound) = crate::work_decision::binding(&self.connection, task)? else {
            return Ok(None);
        };
        let target: Value =
            serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
        let Some(run) = target["runId"].as_str() else {
            return Ok(None);
        };
        self.source_task(task)?;
        let state: String = self
            .connection
            .query_row(
                "SELECT state_json FROM story_generation_runs_v1 WHERE project_id=?1 AND run_id=?2",
                params![task.project_id.as_str(), run],
                |r| r.get(0),
            )
            .map_err(storage)?;
        let state: Value =
            serde_json::from_str(&state).map_err(|_| RepositoryError::CorruptState)?;
        if state["activeTaskId"] != task.task_id {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let Some(chapter) = state["currentChapterId"].as_str() else {
            return Ok(None);
        };
        let Some(feedback) = crate::work_decision_delivery::scene_feedback(
            &self.connection,
            &task.project_id,
            bound["workTaskId"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?,
            chapter,
        )?
        else {
            return Ok(None);
        };
        Ok(Some(
            json!({"chapterId":chapter,"assessmentRevisionId":feedback.revision,
            "assessmentTaskId":feedback.task_id,"sourceCurrent":feedback.current,
            "unresolved":feedback.unresolved,"needsRecheck":feedback.needs_recheck(),
            "authority":"candidateAnalysis","report":feedback.content,
            "repairPolicy":"在当前作者授权内，可按实际问题提议仅修改选定场景或本章；锁定内容和未选中场景必须保留。",
            "recheckPolicy":"旧问题不会因改稿自动解决。若报告引用的正文或资料已变化，按实际依赖更新本章知识，再调用场景效果检查取得对应当前稿的报告。没有报告不代表检查通过；仍需作者决定的重要问题可如实交接。"}),
        ))
    }

    /// Initial source choices never advance with this task's own checkpoints.
    pub fn task_context_origins(
        &self,
        task: &TaskRecord,
    ) -> Result<(Vec<ObjectHead>, Vec<WorkingBaseline>), RepositoryError> {
        self.source_task(task)?;
        let mut statement = self.connection.prepare("SELECT object_id,object_kind,locked,candidate_revision_id,accepted_revision_id,working_revision_id,working_selection_revision FROM story_task_source_heads_v2 WHERE task_id=?1 ORDER BY object_id").map_err(storage)?;
        let rows = statement
            .query_map([&task.task_id], |row| {
                Ok((
                    row.get::<_, String>(/*idx*/ 0)?,
                    row.get::<_, String>(/*idx*/ 1)?,
                    row.get::<_, bool>(/*idx*/ 2)?,
                    row.get::<_, Option<String>>(/*idx*/ 3)?,
                    row.get::<_, Option<String>>(/*idx*/ 4)?,
                    row.get::<_, Option<String>>(/*idx*/ 5)?,
                    row.get::<_, Option<i64>>(/*idx*/ 6)?,
                ))
            })
            .map_err(storage)?;
        let mut heads = Vec::new();
        let mut baselines = Vec::new();
        for row in rows {
            let (object, kind, locked, candidate, accepted, working, selection) =
                row.map_err(storage)?;
            if let Some(selection_revision) = selection {
                baselines.push(WorkingBaseline {
                    object_kind: kind.clone(),
                    object_id: object.strip_prefix("draft-").unwrap_or(&kind).to_owned(),
                    revision_id: revision(working)?,
                    selection_revision,
                    selected_at: 0,
                });
            }
            heads.push(ObjectHead {
                project_id: task.project_id.clone(),
                object_id: ObjectId::parse(object).map_err(|_| RepositoryError::CorruptState)?,
                object_kind: kind,
                locked,
                candidate_revision_id: revision(candidate)?,
                accepted_revision_id: revision(accepted)?,
            });
        }
        Ok((heads, baselines))
    }

    /// Cache only a projection already admitted by the normal context visibility builder.
    /// Callers cannot register arbitrary paths or content through a public HTTP endpoint.
    pub fn project_task_sources(
        &self,
        task: &TaskRecord,
        context: &mut Value,
    ) -> Result<(), RepositoryError> {
        self.source_task(task)?;
        let policy = digest(
            &json!({"operation":task.operation,"target":serde_json::from_str::<Value>(&task.target_json).map_err(|_|RepositoryError::CorruptState)?,"position":context["target"]["position"]}),
        );
        let (known, count): (bool,i64)=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM story_task_source_items_v2 WHERE task_id=?1 AND policy_digest=?2),COUNT(DISTINCT policy_digest) FROM story_task_source_items_v2 WHERE task_id=?1",params![task.task_id,policy],|row|Ok((row.get(/*idx*/0)?,row.get(/*idx*/1)?))).map_err(storage)?;
        if !known && count >= 32 {
            return Err(RepositoryError::InvalidContent(
                "Task source policy cache limit reached".into(),
            ));
        }
        let mut entries = Vec::new();
        // Fixed constructor-owned sections, never JSON pointers supplied by the model.
        for path in [
            "/narrativeGoals/brief",
            "/narrativeGoals/outline",
            "/plannedEffects",
            "/relevantText",
            "/relevantCommitments",
            "/candidateCommitmentProposals",
            "/sceneOutcomeRefs",
            "/operationContext/boundCommitmentInputs",
            "/operationContext/modeling",
            "/operationContext/planningReview",
            "/operationContext/modelingBatch",
            "/operationContext/entityCatalog",
            "/operationContext/workingKnowledge",
            "/operationContext/scenePlanDraftLinks",
            "/operationContext/generationSceneFeedback",
            "/operationContext/propertyRulesBase",
            "/operationContext/revisionOutcomeInput",
            "/operationContext/revisionRepairInput",
            "/operationContext/chapterStructureChange",
        ] {
            let Some(value) = context.pointer(path) else {
                continue;
            };
            if value.is_null() || value.as_array().is_some_and(Vec::is_empty) {
                continue;
            }
            let payload = value.to_string();
            if payload.len() > 64 * 1024 * 1024 {
                return Err(RepositoryError::InvalidContent(
                    "Visible task source exceeds its storage bound".into(),
                ));
            }
            let hash = digest(value);
            let id = format!(
                "source-{}",
                digest(&json!([
                    task.project_id.as_str(),
                    task.task_id,
                    policy,
                    path,
                    hash
                ]))
            );
            self.connection.execute("INSERT OR IGNORE INTO story_task_source_items_v2(task_id,policy_digest,section_path,source_id,content_hash,payload_json) VALUES(?1,?2,?3,?4,?5,?6)",params![task.task_id,policy,path,id,hash,payload]).map_err(storage)?;
            let (id, hash, size): (String,String,i64) = self.connection.query_row("SELECT source_id,content_hash,length(CAST(payload_json AS BLOB)) FROM story_task_source_items_v2 WHERE task_id=?1 AND policy_digest=?2 AND section_path=?3",params![task.task_id,policy,path],|row|Ok((row.get(/*idx*/0)?,row.get(/*idx*/1)?,row.get(/*idx*/2)?))).map_err(storage)?;
            let entry =
                json!({"sourceId":id,"sectionPath":path,"contentHash":hash,"payloadBytes":size});
            entries.push(entry.clone());
            *context
                .pointer_mut(path)
                .ok_or(RepositoryError::CorruptState)? =
                json!({"sourceId":id,"textDelivered":false});
        }
        context["taskSources"] = json!({"authority":"frozenVisibleInput","entries":entries,"textDelivered":false,"readTool":"read_task_source"});
        if !entries.is_empty() {
            context["coverage"]["complete"] = json!(false);
            let missing = context["coverage"]["missingScope"]
                .as_array_mut()
                .ok_or(RepositoryError::CorruptState)?;
            if !missing.iter().any(|entry| entry == "taskSourceContents") {
                missing.push(json!("taskSourceContents"));
            }
        }
        Ok(())
    }

    /// Read a contiguous UTF-8 page of a task-owned immutable visible projection.
    pub fn task_source_page(
        &self,
        task: &TaskRecord,
        source: &str,
        cursor: Option<&str>,
    ) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| {
            self.source_task(task)?;
            let (path,hash,payload): (String,String,String) = self.connection.query_row("SELECT section_path,content_hash,payload_json FROM story_task_source_items_v2 WHERE task_id=?1 AND source_id=?2",params![task.task_id,source],|row|Ok((row.get(/*idx*/0)?,row.get(/*idx*/1)?,row.get(/*idx*/2)?))).optional().map_err(storage)?.ok_or(RepositoryError::TaskOutputForbidden)?;
            let fence = digest(&json!([task.project_id.as_str(),task.task_id,task.thread_id,source,hash]));
            let start = match cursor {
                None => 0,
                Some(cursor) => {
                    let (prefix,offset) = cursor.split_once(':').ok_or_else(invalid_cursor)?;
                    let offset:usize = offset.parse().map_err(|_|invalid_cursor())?;
                    if prefix!=fence || cursor!=format!("{fence}:{offset}") { return Err(invalid_cursor()); }
                    offset
                }
            };
            if start>=payload.len() || !payload.is_char_boundary(start) { return Err(invalid_cursor()); }
            let mut end = (start+2_000).min(payload.len());
            while !payload.is_char_boundary(end) {end-=1;}
            let value = json!({"sourceId":source,"sectionPath":path,"contentHash":hash,"payloadEncoding":"jsonUtf8","byteRange":{"start":start,"end":end,"total":payload.len()},"payloadPart":payload[start..end],"nextCursor":(end<payload.len()).then(||format!("{fence}:{end}")),"endOfInputFromCursor":end==payload.len(),"byteLimit":6500});
            if value.to_string().len()>6_500 { return Err(RepositoryError::InvalidContent("Task source page exceeds its byte budget".into())); }
            Ok(value)
        })?
    }

    fn source_task(&self, task: &TaskRecord) -> Result<(), RepositoryError> {
        if crate::tasks::load_task(&self.connection, &task.task_id)? != *task {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::tasks::validate_active_task(
            &self.connection,
            task,
            &task.project_id,
            &task.thread_id,
            task.input_revision,
        )?;
        if task.capability_revoked_at.is_some() {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        self.assert_task_inputs_current(task)?;
        let exists = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM story_task_source_origins_v2 WHERE task_id=?1)",
                [&task.task_id],
                |row| row.get::<_, bool>(/*idx*/ 0),
            )
            .map_err(storage)?;
        if !exists {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        Ok(())
    }
}

fn digest(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}
fn revision(value: Option<String>) -> Result<Option<RevisionId>, RepositoryError> {
    value
        .map(|value| RevisionId::parse(value).map_err(|_| RepositoryError::CorruptState))
        .transpose()
}
fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
fn invalid_cursor() -> RepositoryError {
    RepositoryError::InvalidContent(
        "Task source cursor does not match the frozen task and UTF-8 range".into(),
    )
}
