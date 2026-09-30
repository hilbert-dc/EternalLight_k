use std::collections::{BTreeSet, HashMap};

use rusqlite::{Connection, params};
use serde_json::{Value, json};

use crate::identity_decisions::IdentityResolution;
use crate::model_sources::{ModelSourceScope, validated_source, visibility};
use crate::property_rules::PropertyRules;
use crate::{ProjectId, RepositoryError};

pub(crate) struct SourceState {
    pub batch_id: String,
    pub assertion_index: usize,
    pub assertion: Value,
}

pub(crate) struct ConditionSources {
    pub states: HashMap<(String, String), Vec<SourceState>>,
    pub kinds: HashMap<String, String>,
    pub coverage: Value,
    pub complete: bool,
}

pub(crate) enum SourceWindow<'a> {
    Interactive,
    ContinueAfter(Option<&'a str>),
}

pub(crate) struct ConditionSourcePage {
    pub sources: ConditionSources,
    pub last_batch: Option<String>,
    pub prior_sources: i64,
}

/// Validate small scopes directly, or use a same-snapshot proof to select relevant sources.
/// Never interpret a bounded arbitrary prefix as the complete long-work scope.
pub(crate) fn collect(
    connection: &Connection,
    project: &ProjectId,
    scope: ModelSourceScope<'_>,
    identity: &IdentityResolution,
    rules: &PropertyRules,
) -> Result<ConditionSources, RepositoryError> {
    Ok(collect_page(
        connection,
        project,
        scope,
        identity,
        rules,
        SourceWindow::Interactive,
    )?
    .sources)
}

pub(crate) fn collect_page(
    connection: &Connection,
    project: &ProjectId,
    scope: ModelSourceScope<'_>,
    identity: &IdentityResolution,
    rules: &PropertyRules,
    window: SourceWindow<'_>,
) -> Result<ConditionSourcePage, RepositoryError> {
    let pending = crate::model_sources::pending_sources(connection, project, scope)?;
    let (visibility, scope_id) = visibility(scope);
    let mut base = format!(
        "FROM story_modeling_batches_v2 b {visibility} AND b.project_id=?1 AND ?2 IS NULL AND b.published_at IS NOT NULL"
    );
    let total: i64 = connection
        .query_row(
            &format!("SELECT COUNT(*) {base}"),
            params![project.as_str(), Option::<String>::None, scope_id],
            |row| row.get(0),
        )
        .map_err(storage)?;
    let proof = crate::knowledge_audit_jobs::current_source_proof(connection, project)?;
    if matches!(window, SourceWindow::ContinueAfter(_)) && proof.is_none() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if proof.is_none() && total > 200 {
        return Ok(ConditionSourcePage {
            sources: ConditionSources {
                states: HashMap::new(),
                kinds: HashMap::new(),
                complete: false,
                coverage: json!({"pendingSources":pending,"totalSources":total,"scannedSources":0,"omittedSources":total,"sourceBytes":0,"relevantSources":total,"sourceProofId":null,"entityKindsComplete":false}),
            },
            last_batch: None,
            prior_sources: 0,
        });
    }
    let mut kinds = HashMap::new();
    if proof.is_some() {
        // Predicates are bound parameters, never executable model-provided SQL.
        base.push_str(" AND EXISTS(SELECT 1 FROM story_assertion_claims_v1 a WHERE a.project_id=b.project_id AND a.batch_id=b.batch_id AND json_extract(a.assertion_json,'$.kind')='state' AND json_extract(a.assertion_json,'$.predicate') IN (SELECT value FROM json_each(?4)))");
    } else {
        base.push_str(" AND ?4 IS NOT NULL");
    }
    let mut predicates = BTreeSet::new();
    for rule in &rules.event_rules {
        condition_predicates(&rule.condition, &mut predicates);
    }
    let predicates =
        serde_json::to_string(&predicates).map_err(|_| RepositoryError::CorruptState)?;
    let relevant: i64 = connection
        .query_row(
            &format!("SELECT COUNT(*) {base}"),
            params![
                project.as_str(),
                Option::<String>::None,
                scope_id,
                predicates
            ],
            |row| row.get(0),
        )
        .map_err(storage)?;
    let (after, limit) = match window {
        SourceWindow::Interactive => (None, 200),
        SourceWindow::ContinueAfter(after) => (after, 1),
    };
    let prior_sources = if let Some(after) = after {
        let present: bool = connection
            .query_row(
                &format!("SELECT EXISTS(SELECT 1 {base} AND b.batch_id=?5)"),
                params![
                    project.as_str(),
                    Option::<String>::None,
                    scope_id,
                    predicates,
                    after
                ],
                |row| row.get(0),
            )
            .map_err(storage)?;
        if !present {
            return Err(RepositoryError::CorruptState);
        }
        connection
            .query_row(
                &format!("SELECT COUNT(*) {base} AND b.batch_id<=?5"),
                params![
                    project.as_str(),
                    Option::<String>::None,
                    scope_id,
                    predicates,
                    after
                ],
                |row| row.get::<_, i64>(0),
            )
            .map_err(storage)?
    } else {
        0
    };
    let mut statement = connection.prepare(&format!(
        "SELECT b.batch_id,length(CAST(b.entities_json AS BLOB))+length(CAST(b.assertions_json AS BLOB))+(SELECT length(CAST(r.content_json AS BLOB)) FROM story_revisions_v2 r WHERE r.project_id=b.project_id AND r.object_id=b.object_id AND r.revision_id=b.source_revision_id AND r.object_kind='draft') {base} AND (?5 IS NULL OR b.batch_id>?5) ORDER BY b.batch_id LIMIT ?6"
    )).map_err(storage)?;
    let rows = statement
        .query_map(
            params![
                project.as_str(),
                Option::<String>::None,
                scope_id,
                predicates,
                after,
                limit
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<usize>>(1)?)),
        )
        .map_err(storage)?;
    let mut states: HashMap<(String, String), Vec<SourceState>> = HashMap::new();
    let mut bytes = 0;
    let mut scanned = 0;
    let mut last_batch = None;
    for row in rows {
        let (batch_id, size) = row.map_err(storage)?;
        let size = size.ok_or(RepositoryError::CorruptState)?;
        if size > 8 * 1024 * 1024 - bytes {
            break;
        }
        bytes += size;
        let source = validated_source(connection, project, scope, &batch_id)?
            .ok_or(RepositoryError::CorruptState)?;
        for entity in &source.entities {
            let id =
                identity.canonical_id(entity["id"].as_str().ok_or(RepositoryError::CorruptState)?);
            let kind = entity["kind"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?
                .to_owned();
            if kinds
                .insert(id, kind.clone())
                .is_some_and(|prior| prior != kind)
            {
                return Err(RepositoryError::CorruptState);
            }
        }
        for (assertion_index, assertion) in source.assertions.into_iter().enumerate() {
            if assertion["kind"] == "state" {
                let key = (
                    identity.canonical_id(
                        assertion["subjectId"]
                            .as_str()
                            .ok_or(RepositoryError::CorruptState)?,
                    ),
                    assertion["predicate"]
                        .as_str()
                        .ok_or(RepositoryError::CorruptState)?
                        .to_owned(),
                );
                states.entry(key).or_default().push(SourceState {
                    batch_id: batch_id.clone(),
                    assertion_index,
                    assertion,
                });
            }
        }
        scanned += 1;
        last_batch = Some(batch_id);
    }
    // A condition needs the types of its state subjects and entity-valued claims,
    // not a bounded prefix of every entity in the novel. Include all identity
    // aliases so a conflicting declaration outside these source batches is seen.
    let mut needed = BTreeSet::new();
    for ((subject, _), claims) in &states {
        needed.insert(subject.clone());
        for claim in claims {
            if let Some(id) = claim.assertion["value"]["entityId"].as_str() {
                needed.insert(identity.canonical_id(id));
            }
        }
    }
    kinds.retain(|id, _| needed.contains(id));
    let mut kinds_complete = true;
    if proof.is_some() {
        let raw_ids = needed
            .iter()
            .flat_map(|id| identity.raw_ids_for_canonical(id))
            .collect::<BTreeSet<_>>();
        let raw_ids = serde_json::to_string(&raw_ids).map_err(|_| RepositoryError::CorruptState)?;
        // Apply scope and identity selection before the materialization limit.
        let mut statement = connection.prepare(&format!("SELECT DISTINCT c.entity_id,c.kind FROM story_entity_claims_v2 c JOIN story_modeling_batches_v2 b ON b.batch_id=c.batch_id AND b.project_id=c.project_id {visibility} AND b.project_id=?1 AND ?2 IS NULL AND b.published_at IS NOT NULL AND c.entity_id IN (SELECT value FROM json_each(?4)) ORDER BY c.entity_id,c.kind LIMIT 1001")).map_err(storage)?;
        let rows = statement
            .query_map(
                params![project.as_str(), Option::<String>::None, scope_id, raw_ids],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(storage)?;
        for (index, row) in rows.enumerate() {
            if index == 1000 {
                kinds_complete = false;
                break;
            }
            let (id, kind) = row.map_err(storage)?;
            if kinds
                .insert(identity.canonical_id(&id), kind.clone())
                .is_some_and(|prior| prior != kind)
            {
                return Err(RepositoryError::CorruptState);
            }
        }
    }
    Ok(ConditionSourcePage {
        sources: ConditionSources {
            states,
            kinds,
            complete: pending == 0 && scanned == relevant && kinds_complete,
            coverage: json!({"pendingSources":pending,"totalSources":total,"scannedSources":scanned,"omittedSources":relevant-scanned,"sourceBytes":bytes,"relevantSources":relevant,"sourceProofId":proof,"entityKindsComplete":kinds_complete}),
        },
        last_batch,
        prior_sources,
    })
}

fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

fn condition_predicates(
    condition: &crate::event_rules::Condition,
    predicates: &mut BTreeSet<String>,
) {
    use crate::event_rules::Condition;
    match condition {
        Condition::All { conditions } | Condition::Any { conditions } => {
            for condition in conditions {
                condition_predicates(condition, predicates);
            }
        }
        Condition::Not { condition } => condition_predicates(condition, predicates),
        Condition::State { predicate, .. } => {
            predicates.insert(predicate.clone());
        }
    }
}
