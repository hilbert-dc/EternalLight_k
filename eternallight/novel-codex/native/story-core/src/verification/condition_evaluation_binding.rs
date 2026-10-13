use rusqlite::{Connection, params};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{ConditionEvaluationTarget, storage};
use crate::model_sources::{ModelSourceScope, pending_sources, validated_source};
use crate::{ProjectId, RepositoryError, SnapshotId};

/// Capture only inside the transaction that reads or stores the request.
/// The global proof conservatively fences even historical requests on any source
/// change; it is not permission to import current sources into an old snapshot.
pub(super) fn capture(
    db: &Connection,
    project: &ProjectId,
    target: &ConditionEvaluationTarget,
) -> Result<Option<String>, RepositoryError> {
    if db.is_autocommit() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let snapshot = target
        .snapshot_id
        .as_ref()
        .map(|id| SnapshotId::parse(id.clone()))
        .transpose()
        .map_err(|_| RepositoryError::InvalidContent("invalid condition snapshot".into()))?;
    if let Some(snapshot) = &snapshot {
        let owned:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM story_snapshots_v2 WHERE project_id=?1 AND snapshot_id=?2)",params![project.as_str(),snapshot.as_str()],|row|row.get(0)).map_err(storage)?;
        if !owned {
            return Ok(None);
        }
    }
    let scope = snapshot
        .as_ref()
        .map_or(ModelSourceScope::Current, ModelSourceScope::Snapshot);
    let Some(proof) = crate::knowledge_audit_jobs::current_source_proof(db, project)? else {
        return Ok(None);
    };
    if pending_sources(db, project, scope)? != 0 {
        return Ok(None);
    }
    let Some(source) = validated_source(db, project, scope, &target.source_batch_id)? else {
        return Ok(None);
    };
    let Some(event) = source
        .assertions
        .get(target.assertion_index)
        .filter(|event| event["kind"] == "event")
    else {
        return Ok(None);
    };
    match crate::event_sources::resolve(
        db,
        project,
        scope,
        &target.source_batch_id,
        event,
        &source.entities,
    ) {
        Ok(_) => {}
        Err(RepositoryError::InvalidContent(_)) => return Ok(None),
        Err(error) => return Err(error),
    }
    let rules = crate::property_rules::load(db, project, snapshot.as_ref())?;
    if !rules
        .event_rules
        .iter()
        .any(|rule| event["predicate"].as_str() == Some(rule.event_type.as_str()))
    {
        return Ok(None);
    }
    let identity = match &snapshot {
        Some(snapshot) => {
            crate::identity_decisions::identity_resolution_at_snapshot(db, project, snapshot)?
        }
        None => crate::identity_decisions::identity_resolution(db, project)?,
    };
    let rule_revision = rules.revision_id.ok_or(RepositoryError::CorruptState)?;
    let rule_content:String=db.query_row("SELECT content_json FROM story_revisions_v2 WHERE project_id=?1 AND object_id='artifact-propertyRules' AND object_kind='propertyRules' AND revision_id=?2",params![project.as_str(),rule_revision],|row|row.get(0)).map_err(storage)?;
    let snapshot_generation:i64=db.query_row("SELECT COALESCE((SELECT generation FROM story_snapshot_knowledge_audit_state_v1 WHERE project_id=?1),0)",[project.as_str()],|row|row.get(0)).map_err(storage)?;
    let payload = json!({"algorithm":"condition-evaluation-v1","projectId":project.as_str(),"target":target,"sourceProofId":proof,"snapshotGeneration":snapshot_generation,"event":event,"ruleRevisionId":rule_revision,"ruleContent":rule_content,"identityRevisionId":identity.revision_id,"identityDecisions":identity.decisions});
    Ok(Some(format!(
        "{:x}",
        Sha256::digest(payload.to_string().as_bytes())
    )))
}
