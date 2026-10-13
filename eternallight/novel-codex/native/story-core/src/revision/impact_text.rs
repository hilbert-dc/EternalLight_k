use std::collections::HashSet;

use rusqlite::{params_from_iter, types::Value as SqlValue};
use serde_json::{Value, json};

use crate::identity_decisions::{identity_resolution, identity_resolution_at_snapshot};
use crate::{
    ImpactAnalysisScope, ProjectId, RepositoryError, SearchQuery, SearchScope,
    SqliteStoryRepository,
};

const MAX_NAMES: usize = 4;
const MAX_TEXT_RESULTS: usize = 30;

pub(crate) fn literal_impact_candidates(
    repository: &SqliteStoryRepository,
    project_id: &ProjectId,
    scope: ImpactAnalysisScope<'_>,
    canonical_id: &str,
) -> Result<Value, RepositoryError> {
    let identity = match scope {
        ImpactAnalysisScope::Current => identity_resolution(&repository.connection, project_id)?,
        ImpactAnalysisScope::Snapshot(snapshot_id) => {
            identity_resolution_at_snapshot(&repository.connection, project_id, snapshot_id)?
        }
    };
    let raw_ids = identity.raw_ids_for_canonical(canonical_id);
    let placeholders = vec!["?"; raw_ids.len()].join(", ");
    let visibility = match scope {
        ImpactAnalysisScope::Current => {
            "JOIN story_knowledge_heads_v2 h ON h.batch_id = n.batch_id AND h.project_id = n.project_id \
             JOIN story_modeling_batches_v2 b ON b.batch_id = h.batch_id AND b.project_id = h.project_id AND b.object_id = h.object_id AND b.status = 'ready' \
             JOIN story_object_heads_v2 o ON o.project_id = b.project_id AND o.object_id = b.object_id AND b.source_revision_id = COALESCE(o.candidate_revision_id, o.accepted_revision_id) \
             WHERE n.project_id = ? AND ? IS NULL"
        }
        ImpactAnalysisScope::Snapshot(_) => {
            "JOIN story_snapshot_knowledge_v2 k ON k.batch_id = n.batch_id \
             JOIN story_modeling_batches_v2 b ON b.batch_id = k.batch_id AND b.project_id = n.project_id AND b.object_id = k.object_id \
             JOIN story_snapshot_members_v2 m ON m.snapshot_id = k.snapshot_id AND m.object_id = k.object_id AND m.revision_id = b.source_revision_id \
             WHERE n.project_id = ? AND k.snapshot_id = ?"
        }
    };
    let sql = format!(
        "SELECT DISTINCT n.name FROM story_entity_names_v2 n {visibility} \
         AND n.entity_id IN ({placeholders}) ORDER BY n.name LIMIT ?"
    );
    let mut args = vec![SqlValue::Text(project_id.as_str().to_owned())];
    args.push(match scope {
        ImpactAnalysisScope::Current => SqlValue::Null,
        ImpactAnalysisScope::Snapshot(snapshot_id) => {
            SqlValue::Text(snapshot_id.as_str().to_owned())
        }
    });
    args.extend(raw_ids.into_iter().map(SqlValue::Text));
    args.push(SqlValue::Integer((MAX_NAMES + 1) as i64));
    let mut statement = repository.connection.prepare(&sql).map_err(storage_error)?;
    let names = statement
        .query_map(params_from_iter(args), |row| row.get::<_, String>(0))
        .map_err(storage_error)?
        .map(|row| row.map_err(storage_error))
        .collect::<Result<Vec<_>, _>>()?;
    let names_truncated = names.len() > MAX_NAMES;
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let mut search_complete = true;
    let mut results_truncated = false;
    let scanned_names = names.into_iter().take(MAX_NAMES).collect::<Vec<_>>();
    for name in &scanned_names {
        if name.trim().is_empty() || name.chars().count() > 240 {
            search_complete = false;
            continue;
        }
        let result = repository.search_documents(SearchQuery {
            project_id,
            snapshot_id: match scope {
                ImpactAnalysisScope::Current => None,
                ImpactAnalysisScope::Snapshot(snapshot_id) => Some(snapshot_id),
            },
            text: name,
            kind: Some("draft"),
            scope: SearchScope::All,
            limit: MAX_TEXT_RESULTS,
            cursor: None,
        })?;
        search_complete &= result["coverage"]["complete"] == true;
        for hit in result["results"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?
        {
            let key = (
                hit["sourceRevisionId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                hit["sourcePath"].as_str().unwrap_or_default().to_owned(),
                hit["evidence"]["start"].as_u64().unwrap_or_default(),
            );
            if !seen.insert(key) {
                continue;
            }
            if candidates.len() == MAX_TEXT_RESULTS {
                results_truncated = true;
                break;
            }
            candidates.push(json!({
                "classification": "literalMentionNeedsReview",
                "matchedName": name,
                "sourceObjectId": hit["id"],
                "sourceRevisionId": hit["sourceRevisionId"],
                "sourcePath": hit["sourcePath"],
                "evidence": hit["evidence"],
                "snippet": hit["snippet"]
            }));
        }
        if results_truncated {
            break;
        }
    }
    Ok(json!({
        "candidates": candidates,
        "searchedNames": scanned_names,
        "coverage": {
            "literalNameSearchComplete": !names_truncated && !results_truncated && search_complete && !scanned_names.is_empty(),
            "namesTruncated": names_truncated,
            "resultsTruncated": results_truncated,
            "indexIntegrityVerified": false
        }
    }))
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

use crate::{SearchIndexRepository};
