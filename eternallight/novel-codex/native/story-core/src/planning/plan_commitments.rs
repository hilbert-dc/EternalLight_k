use rusqlite::params;
use serde::Deserialize;
use serde_json::Value;

use crate::{RepositoryError, TaskRecord};

#[derive(Deserialize, PartialEq, Eq, Hash, ts_rs::TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum CommitmentRef {
    Commitment {
        commitment_id: String,
        revision_id: String,
    },
    Proposal {
        report_revision_id: String,
        proposal_id: String,
        source_revision_id: String,
    },
}

impl CommitmentRef {
    pub(crate) fn parse(value: &Value) -> Result<Self, RepositoryError> {
        let reference: Self = serde_json::from_value(value.clone())
            .map_err(|_| RepositoryError::TaskOutputForbidden)?;
        let ids = match &reference {
            Self::Commitment {
                commitment_id,
                revision_id,
            } => vec![commitment_id, revision_id],
            Self::Proposal {
                report_revision_id,
                proposal_id,
                source_revision_id,
            } => vec![report_revision_id, proposal_id, source_revision_id],
        };
        if ids.iter().any(|id| id.trim().is_empty() || id.len() > 160) {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        Ok(reference)
    }
}

pub(crate) fn validate_links(
    connection: &rusqlite::Connection,
    task: &TaskRecord,
    content: &str,
) -> Result<(), RepositoryError> {
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    let through = match target.get("planFromChapter") {
        Some(from) => {
            from.as_i64()
                .filter(|from| *from > 0)
                .ok_or(RepositoryError::TaskBindingInvalid)?
                - 1
        }
        None => i64::MAX,
    };
    let plan: Value =
        serde_json::from_str(content).map_err(|_| RepositoryError::TaskOutputForbidden)?;
    for chapter in plan["payload"]["chapters"]
        .as_array()
        .ok_or(RepositoryError::TaskOutputForbidden)?
    {
        let through = through.min(
            chapter["chapterNumber"]
                .as_i64()
                .filter(|number| *number > 0)
                .ok_or(RepositoryError::TaskOutputForbidden)?
                - 1,
        );
        for scene in chapter["scenes"]
            .as_array()
            .ok_or(RepositoryError::TaskOutputForbidden)?
        {
            for effect in scene["expectedEffects"]
                .as_array()
                .ok_or(RepositoryError::TaskOutputForbidden)?
            {
                for value in effect["commitmentRefs"]
                    .as_array()
                    .ok_or(RepositoryError::TaskOutputForbidden)?
                {
                    let valid: bool = match CommitmentRef::parse(value)? {
                        CommitmentRef::Commitment {commitment_id,revision_id} => connection.query_row(
                            "SELECT EXISTS(SELECT 1 FROM story_branches_v2 b
                             JOIN story_snapshot_members_v2 m ON m.snapshot_id=b.head_snapshot_id
                             JOIN story_revisions_v2 r ON r.project_id=b.project_id AND r.revision_id=m.revision_id
                             WHERE b.project_id=?1 AND b.branch_id='main' AND r.object_kind='commitment'
                               AND m.object_id='commitment-' || ?2 AND r.revision_id=?3
                               AND NOT EXISTS (
                                 SELECT 1 FROM (
                                   SELECT value FROM json_each(r.content_json,'$.payload.setupEvidenceRefs')
                                   UNION ALL SELECT value FROM json_each(r.content_json,'$.payload.fulfillmentEvidenceRefs')
                                 ) a LEFT JOIN story_snapshot_members_v2 sm ON sm.snapshot_id=b.head_snapshot_id
                                   AND sm.object_id=json_extract(a.value,'$.objectId')
                                 LEFT JOIN story_revisions_v2 s ON s.project_id=b.project_id AND s.revision_id=sm.revision_id AND s.object_kind='draft'
                                 WHERE s.revision_id IS NULL OR s.revision_id!=json_extract(a.value,'$.revisionId')
                                   OR COALESCE(json_extract(s.content_json,'$.chapterNumber'),0) NOT BETWEEN 1 AND ?4
                               ))",
                            params![task.project_id.as_str(),commitment_id,revision_id,through],|row|row.get(0),
                        ),
                        CommitmentRef::Proposal {report_revision_id,proposal_id,source_revision_id} => connection.query_row(
                            "SELECT EXISTS(SELECT 1 FROM story_branches_v2 b
                             JOIN story_snapshot_members_v2 m ON m.snapshot_id=b.head_snapshot_id
                             JOIN story_revisions_v2 r ON r.project_id=b.project_id AND r.revision_id=m.revision_id
                             JOIN story_snapshot_members_v2 sm ON sm.snapshot_id=b.head_snapshot_id
                               AND sm.object_id='draft-' || json_extract(r.content_json,'$.payload.chapterId')
                             JOIN story_revisions_v2 s ON s.project_id=b.project_id AND s.revision_id=sm.revision_id AND s.object_kind='draft'
                             JOIN json_each(r.content_json,'$.payload.candidates') p
                             WHERE b.project_id=?1 AND b.branch_id='main' AND r.object_kind='commitmentExtraction'
                               AND r.revision_id=?2 AND json_extract(p.value,'$.id')=?3 AND s.revision_id=?4
                               AND json_extract(r.content_json,'$.payload.sourceRevisionId')=s.revision_id
                               AND json_extract(s.content_json,'$.chapterNumber') BETWEEN 1 AND ?5
                               AND NOT EXISTS (
                                 SELECT 1 FROM story_snapshot_members_v2 cm
                                 JOIN story_revisions_v2 c ON c.project_id=b.project_id AND c.revision_id=cm.revision_id
                                 WHERE cm.snapshot_id=b.head_snapshot_id AND c.object_kind='commitment'
                                   AND json_extract(c.content_json,'$.extractionOrigin.reportRevisionId')=r.revision_id
                                   AND json_extract(c.content_json,'$.extractionOrigin.proposalId')=json_extract(p.value,'$.id')
                               ))",
                            params![task.project_id.as_str(),report_revision_id,proposal_id,source_revision_id,through],|row|row.get(0),
                        ),
                    }.map_err(|error|RepositoryError::Storage(error.to_string()))?;
                    if !valid {
                        return Err(RepositoryError::TaskOutputForbidden);
                    }
                }
            }
        }
    }
    Ok(())
}
