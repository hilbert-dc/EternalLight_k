use serde_json::{Value, json};

use crate::{
    ImpactAnalysisQuery, ImpactAnalysisScope, RepositoryError, SqliteStoryRepository, TaskRecord,
};

impl SqliteStoryRepository {
    /// Reads explicit impact against current, captured advisory inputs in one read snapshot.
    /// The caller cannot select another snapshot, task purpose or revision pair.
    pub fn analyze_task_impact(
        &self,
        task: &TaskRecord,
        entity_id: &str,
        max_hops: usize,
        limit: usize,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        if !(1..=3).contains(&max_hops)
            || !(1..=20).contains(&limit)
            || entity_id.is_empty()
            || entity_id.len() > 160
        {
            return Err(RepositoryError::InvalidContent(
                "invalid task impact limits".into(),
            ));
        }
        self.with_read_snapshot(|| {
            let task = self.authorize_task_capability(
                &task.project_id,
                &task.task_id,
                &task.thread_id,
                task.input_revision,
                &task.capability_hash,
                now,
            )?;
            let target: Value = serde_json::from_str(&task.target_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            if !matches!(
                task.operation.as_str(),
                "advisory" | "assessChapterStructure"
            ) || target["type"] != "project"
            {
                return Err(RepositoryError::TaskOutputForbidden);
            }
            self.assert_task_inputs_current(&task)?;
            if task.operation == "assessChapterStructure" {
                crate::chapter_structure::assert_task(&self.connection, &task)?;
            }
            // Current means this SQLite read snapshot, whose object/knowledge heads
            // were just checked against the task captures. A branch snapshot can
            // predate a published knowledge batch and must not silently omit it.
            let mut report = self.analyze_explicit_impact_in_snapshot(ImpactAnalysisQuery {
                project_id: &task.project_id,
                scope: ImpactAnalysisScope::Current,
                changed_entity_id: entity_id,
                world_time: None,
                max_hops,
                limit,
                revision_pair: None,
            })?;
            report["taskId"] = json!(task.task_id);
            report["projectId"] = json!(task.project_id.as_str());
            report["inputRevision"] = json!(task.input_revision);
            Ok(report)
        })?
    }
}
