use rusqlite::{TransactionBehavior, params};
use serde_json::json;

use super::{ConditionEvaluationJob, ConditionEvaluationStatus, load, storage};
use crate::event_rule_evaluation::assess_with_progress;
use crate::event_rule_sources::{SourceWindow, collect_page};
use crate::model_sources::{ModelSourceScope, validated_source};
use crate::{ProjectId, RepositoryError, SnapshotId, SqliteStoryRepository};
use sha2::{Digest, Sha256};

impl SqliteStoryRepository {
    /// Execute one source batch and commit its cursor, checkpoint and report
    /// together. Expected-version fencing prevents duplicate page consumption.
    pub fn advance_condition_evaluation(
        &mut self,
        project: &ProjectId,
        id: &str,
        expected_version: i64,
    ) -> Result<ConditionEvaluationJob, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let job = load(&tx, project, id)?;
        if job.version != expected_version {
            return Err(RepositoryError::InvalidContent(
                "condition evaluation version changed".into(),
            ));
        }
        let version = job
            .version
            .checked_add(1)
            .ok_or(RepositoryError::CorruptState)?;
        if job.status == ConditionEvaluationStatus::Stale {
            tx.execute("UPDATE story_condition_evaluations_v1 SET status='stale',version=?1,result_json=NULL WHERE id=?2 AND project_id=?3",params![version,id,project.as_str()]).map_err(storage)?;
            let job = load(&tx, project, id)?;
            tx.commit().map_err(storage)?;
            return Ok(job);
        }
        if !matches!(
            job.status,
            ConditionEvaluationStatus::Queued | ConditionEvaluationStatus::Running
        ) {
            return Err(RepositoryError::InvalidContent(
                "condition evaluation is not runnable".into(),
            ));
        }
        let last:Option<String>=tx.query_row("SELECT last_batch_id FROM story_condition_evaluations_v1 WHERE project_id=?1 AND id=?2",params![project.as_str(),id],|row|row.get(0)).map_err(storage)?;
        let mut progress = super::explanations::checkpoint(&tx, project, id)?;
        let snapshot = job
            .target
            .snapshot_id
            .as_ref()
            .map(|id| SnapshotId::parse(id.clone()))
            .transpose()
            .map_err(|_| RepositoryError::CorruptState)?;
        let scope = snapshot
            .as_ref()
            .map_or(ModelSourceScope::Current, ModelSourceScope::Snapshot);
        let identity = match &snapshot {
            Some(snapshot) => {
                crate::identity_decisions::identity_resolution_at_snapshot(&tx, project, snapshot)?
            }
            None => crate::identity_decisions::identity_resolution(&tx, project)?,
        };
        let rules = crate::property_rules::load(&tx, project, snapshot.as_ref())?;
        let source = validated_source(&tx, project, scope, &job.target.source_batch_id)?
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let event = source
            .assertions
            .get(job.target.assertion_index)
            .ok_or(RepositoryError::CorruptState)?;
        let mut page = collect_page(
            &tx,
            project,
            scope,
            &identity,
            &rules,
            SourceWindow::ContinueAfter(last.as_deref()),
        )?;
        if page.prior_sources != job.scanned_sources {
            return Err(RepositoryError::CorruptState);
        }
        let scanned = page.sources.coverage["scannedSources"]
            .as_i64()
            .ok_or(RepositoryError::CorruptState)?;
        let relevant = page.sources.coverage["relevantSources"]
            .as_i64()
            .ok_or(RepositoryError::CorruptState)?;
        if (scanned == 0 && relevant > job.scanned_sources)
            || page.sources.coverage["entityKindsComplete"] != true
        {
            return Err(RepositoryError::InvalidContent(
                "condition source page exceeds its byte or type budget".into(),
            ));
        }
        let scanned = job
            .scanned_sources
            .checked_add(scanned)
            .ok_or(RepositoryError::CorruptState)?;
        if scanned > relevant {
            return Err(RepositoryError::CorruptState);
        }
        let bytes = job
            .source_bytes
            .checked_add(
                page.sources.coverage["sourceBytes"]
                    .as_i64()
                    .ok_or(RepositoryError::CorruptState)?,
            )
            .ok_or(RepositoryError::CorruptState)?;
        let finished = scanned == relevant;
        page.sources.complete = finished && page.sources.coverage["pendingSources"] == 0;
        page.sources.coverage["scannedSources"] = json!(scanned);
        page.sources.coverage["omittedSources"] = json!(relevant - scanned);
        page.sources.coverage["sourceBytes"] = json!(bytes);
        let assessment = assess_with_progress(
            event,
            &rules,
            &identity,
            Some(&page.sources),
            &mut 100_000,
            &mut progress,
        )?;
        if assessment.budget_exhausted {
            return Err(RepositoryError::InvalidContent(
                "condition page comparison budget exhausted; checkpoint was not advanced".into(),
            ));
        }
        let encoded =
            serde_json::to_string(&progress).map_err(|_| RepositoryError::CorruptState)?;
        if encoded.len() > 8 * 1024 * 1024 {
            return Err(RepositoryError::InvalidContent(
                "condition checkpoint exceeds byte budget".into(),
            ));
        }
        let result = if finished {
            Some(assessment.into_durable_report(id)?.to_string())
        } else {
            None
        };
        let digest = format!("{:x}", Sha256::digest(encoded.as_bytes()));
        let last = page.last_batch.or(last);
        let status = if finished { "completed" } else { "running" };
        let changed=tx.execute("UPDATE story_condition_evaluations_v1 SET status=?1,version=?2,progress_json=?3,last_batch_id=?4,scanned_sources=?5,source_bytes=?6,result_json=?7,progress_digest=?11 WHERE id=?8 AND project_id=?9 AND version=?10",params![status,version,encoded,last,scanned,bytes,result,id,project.as_str(),expected_version,digest]).map_err(storage)?;
        if changed != 1 {
            return Err(RepositoryError::CorruptState);
        }
        let job = load(&tx, project, id)?;
        tx.commit().map_err(storage)?;
        Ok(job)
    }
}
