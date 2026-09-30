use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use super::{
    ChapterBinding, assert_revision, cancel_active_tasks_for_chapter_structure, replay,
    storage_error, validate_plan_entry,
};
use crate::{ProjectId, RepositoryError, SqliteStoryRepository};

pub struct RebindChapterPlanRequest<'a> {
    pub project_id: &'a ProjectId,
    pub chapter_id: &'a str,
    pub previous_plan_revision_id: &'a str,
    pub previous_chapter_plan_id: &'a str,
    pub source_revision_id: &'a str,
    pub plan_revision_id: &'a str,
    pub chapter_plan_id: &'a str,
    pub expected_revision: i64,
    pub idempotency_key: &'a str,
    pub request_digest: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReboundChapterPlan {
    pub previous_plan_revision_id: String,
    pub previous_chapter_plan_id: String,
    pub source_revision_id: String,
    pub binding: ChapterBinding,
}

impl SqliteStoryRepository {
    pub fn rebind_chapter_plan(
        &mut self,
        request: RebindChapterPlanRequest<'_>,
    ) -> Result<ReboundChapterPlan, RepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let project_id = request.project_id;
        if let Some(value) = replay(
            &transaction,
            project_id,
            "rebind-plan",
            request.idempotency_key,
            request.request_digest,
        )? {
            return serde_json::from_value(value).map_err(|_| RepositoryError::CorruptState);
        }
        assert_revision(&transaction, project_id, request.expected_revision)?;
        let current: Option<(i64, String, String, String, bool)> = transaction.query_row(
            "SELECT c.number,b.plan_revision_id,b.chapter_plan_id,COALESCE(d.candidate_revision_id,d.accepted_revision_id),d.locked
             FROM story_chapters_v2 c
             JOIN story_chapter_plan_bindings_v2 b ON b.project_id=c.project_id AND b.chapter_id=c.chapter_id
             JOIN story_object_heads_v2 d ON d.project_id=c.project_id AND d.object_id='draft-' || c.chapter_id AND d.object_kind='draft'
             WHERE c.project_id=?1 AND c.chapter_id=?2 AND COALESCE(d.candidate_revision_id,d.accepted_revision_id) IS NOT NULL",
            params![project_id.as_str(),request.chapter_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
        ).optional().map_err(storage_error)?;
        let (number, previous_plan, previous_entry, source, locked) =
            current.ok_or(RepositoryError::CandidateNotFound)?;
        if locked
            || previous_plan != request.previous_plan_revision_id
            || previous_entry != request.previous_chapter_plan_id
            || source != request.source_revision_id
        {
            return Err(RepositoryError::InvalidContent(
                "chapter binding or editable draft changed; read a new confirmation".into(),
            ));
        }
        let selected_plan: Option<String> = transaction.query_row(
            "SELECT COALESCE(candidate_revision_id,accepted_revision_id) FROM story_object_heads_v2 WHERE project_id=?1 AND object_id='artifact-plan' AND object_kind='plan'",
            [project_id.as_str()], |row| row.get(0),
        ).optional().map_err(storage_error)?.flatten();
        if selected_plan.as_deref() != Some(request.plan_revision_id) {
            return Err(RepositoryError::InvalidContent(
                "selected plan changed; read a new confirmation".into(),
            ));
        }
        validate_plan_entry(
            &transaction,
            project_id,
            request.plan_revision_id,
            request.chapter_plan_id,
            number,
        )?;
        if previous_plan == request.plan_revision_id && previous_entry == request.chapter_plan_id {
            return Err(RepositoryError::InvalidContent(
                "chapter already uses this plan entry".into(),
            ));
        }
        let occupied: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM story_chapter_plan_bindings_v2 WHERE project_id=?1 AND chapter_plan_id=?2 AND chapter_id<>?3)",
            params![project_id.as_str(),request.chapter_plan_id,request.chapter_id], |row| row.get(0),
        ).map_err(storage_error)?;
        if occupied {
            return Err(RepositoryError::InvalidContent(
                "plan entry is associated with another chapter".into(),
            ));
        }
        let result = ReboundChapterPlan {
            previous_plan_revision_id: previous_plan,
            previous_chapter_plan_id: previous_entry,
            source_revision_id: source,
            binding: ChapterBinding {
                plan_revision_id: request.plan_revision_id.into(),
                chapter_plan_id: request.chapter_plan_id.into(),
                chapter_id: request.chapter_id.into(),
                chapter_number: number,
                result_revision: request.expected_revision + 1,
            },
        };
        transaction.execute(
            "UPDATE story_chapter_plan_bindings_v2 SET plan_revision_id=?1,chapter_plan_id=?2,created_at=?3 WHERE project_id=?4 AND chapter_id=?5",
            params![request.plan_revision_id,request.chapter_plan_id,request.now,project_id.as_str(),request.chapter_id],
        ).map_err(storage_error)?;
        transaction
            .execute(
                "UPDATE story_workspaces_v2 SET revision=?1 WHERE project_id=?2",
                params![result.binding.result_revision, project_id.as_str()],
            )
            .map_err(storage_error)?;
        cancel_active_tasks_for_chapter_structure(&transaction, project_id, request.now)?;
        let open_runs = {
            let mut statement = transaction.prepare(
                "SELECT run_id,version,state_json FROM story_generation_runs_v1 WHERE project_id=?1 AND json_extract(state_json,'$.status') IN ('running','paused','blocked') LIMIT 2",
            ).map_err(storage_error)?;
            statement
                .query_map([project_id.as_str()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(storage_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage_error)?
        };
        if open_runs.len() > 1 {
            return Err(RepositoryError::CorruptState);
        }
        for (run_id, version, content) in open_runs {
            let mut state: serde_json::Value =
                serde_json::from_str(&content).map_err(|_| RepositoryError::CorruptState)?;
            if state["id"] != run_id
                || state["projectId"] != project_id.as_str()
                || state["version"].as_i64() != Some(version)
            {
                return Err(RepositoryError::CorruptState);
            }
            let next_version = version
                .checked_add(1)
                .ok_or(RepositoryError::CorruptState)?;
            let status = match state["status"].as_str() {
                Some("running" | "blocked") => "blocked",
                Some("paused") => "paused",
                Some(_) | None => return Err(RepositoryError::CorruptState),
            };
            state["status"] = serde_json::json!(status);
            state["blockedReason"] = serde_json::json!("chapter_plan_rebound");
            state["version"] = serde_json::json!(next_version);
            if transaction.execute(
                "UPDATE story_generation_runs_v1 SET version=?1,state_json=?2 WHERE project_id=?3 AND run_id=?4 AND version=?5",
                params![next_version,state.to_string(),project_id.as_str(),run_id,version],
            ).map_err(storage_error)? != 1 || transaction.execute(
                "UPDATE story_work_tasks_v1 SET status=?1,version=version+1 WHERE project_id=?2 AND id=?3 AND kind='generation'",
                params![status,project_id.as_str(),format!("work-{run_id}")],
            ).map_err(storage_error)? != 1 {
                return Err(RepositoryError::CorruptState);
            }
        }
        let receipt = serde_json::to_string(&result).map_err(|_| RepositoryError::CorruptState)?;
        transaction.execute(
            "INSERT INTO story_domain_receipts_v2(project_id,operation,idempotency_key,request_digest,result_json) VALUES (?1,'rebind-plan',?2,?3,?4)",
            params![project_id.as_str(),request.idempotency_key,request.request_digest,receipt],
        ).map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        Ok(result)
    }
}
