use crate::model_sources::ModelSourceScope;
use crate::proposition_sources::proposition_source_visible;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::{ObjectId, ProjectId, RepositoryError, RevisionId};

pub(crate) fn proposals_for_current_batch(
    connection: &Connection,
    project_id: &ProjectId,
    object_id: &ObjectId,
    revision_id: &RevisionId,
    batch_id: &str,
) -> Result<String, RepositoryError> {
    let (assertions_json, entities_json) = connection.query_row(
        "SELECT b.assertions_json, b.entities_json FROM story_modeling_batches_v2 b JOIN story_knowledge_heads_v2 h ON h.project_id = b.project_id AND h.batch_id = b.batch_id AND h.object_id = b.object_id WHERE b.project_id = ?1 AND b.object_id = ?2 AND b.source_revision_id = ?3 AND b.batch_id = ?4 AND b.status = 'ready'",
        params![project_id.as_str(), object_id.as_str(), revision_id.as_str(), batch_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    ).optional().map_err(storage_error)?.ok_or(RepositoryError::ReviewChecksFailed)?;
    let assertions: Vec<Value> =
        serde_json::from_str(&assertions_json).map_err(|_| RepositoryError::CorruptState)?;
    let entities: Vec<Value> =
        serde_json::from_str(&entities_json).map_err(|_| RepositoryError::CorruptState)?;
    crate::story_events::validate_batch(&assertions, &entities)
        .map_err(|_| RepositoryError::CorruptState)?;
    crate::event_sources::validate_batch(
        connection,
        project_id,
        ModelSourceScope::Current,
        batch_id,
        &assertions,
        &entities,
    )
    .map_err(|_| RepositoryError::CorruptState)?;
    let source_json = connection.query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE project_id = ?1 AND object_id = ?2 AND revision_id = ?3",
        params![project_id.as_str(), object_id.as_str(), revision_id.as_str()],
        |row| row.get::<_, String>(0),
    ).map_err(storage_error)?;
    let source: Value =
        serde_json::from_str(&source_json).map_err(|_| RepositoryError::CorruptState)?;
    let body = source["body"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?;
    let groups = source["sceneGroups"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    let mut cursor = body.chars().count() - body.trim_start().chars().count();
    let mut group_ranges = Vec::with_capacity(groups.len());
    for (index, group) in groups.iter().enumerate() {
        let text = group["body"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?
            .trim();
        let end = cursor + text.chars().count();
        let id = group["id"].as_str().ok_or(RepositoryError::CorruptState)?;
        group_ranges.push((cursor, end, id, index + 1));
        cursor = end + "\n\n---\n\n".chars().count();
    }
    let chapter_number = source["chapterNumber"]
        .as_i64()
        .ok_or(RepositoryError::CorruptState)?;
    let rules = crate::property_rules::load(connection, project_id, None)?;
    let identity = crate::identity_decisions::identity_resolution(connection, project_id)?;
    let mut condition_sources = None;
    let mut condition_budget = 100_000;
    let proposals = assertions
        .iter()
        .enumerate()
        .map(|(index, assertion)| {
            let start = assertion["evidence"]["start"]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or(RepositoryError::CorruptState)?;
            let end = assertion["evidence"]["end"]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or(RepositoryError::CorruptState)?;
            let (scene_group_id, scene_order) = group_ranges
                .iter()
                .find(|(group_start, group_end, _, _)| start >= *group_start && end <= *group_end)
                .map(|(_, _, id, order)| (*id, *order))
                .ok_or(RepositoryError::ReviewChecksFailed)?;
            let mut proposal = json!({
                "id": format!("fact-{batch_id}-{index}"),
                "sceneGroupId": scene_group_id, "sceneOrder": scene_order,
                "sceneId": groups[scene_order - 1]["sceneId"],
                "readingChapter": chapter_number,
                "factKind": if assertion["kind"] == "epistemic" { "character" } else { "world" },
                "subjectId": assertion["subjectId"], "predicate": assertion["predicate"],
                "value": assertion["value"],
                "epistemicOwnerId": assertion.get("epistemicOwnerId"),
                "epistemicState": assertion["epistemicState"].as_str().unwrap_or("unknown"),
                "storyTime": assertion.get("storyTime"),
                "evidence": assertion["evidence"]["quote"],
                "evidenceAnchor": assertion["evidence"],
                "modelingAssertion": assertion,
                "modelingBatchId": batch_id,
                "modelingAssertionIndex": index,
                "sourceRevisionId": revision_id.as_str(),
                "source": "agent", "reviewDecision": "candidate"
            });
            if assertion["kind"] == "state" {
                crate::state_properties::schema(assertion)
                    .map_err(|_| RepositoryError::CorruptState)?;
                proposal["stateProperty"] = json!({
                    "schema": assertion["stateSchema"],
                    "validFrom": assertion["validFrom"],
                    "validUntilExclusive": assertion["validUntilExclusive"],
                    "schemaAuthority": "modelDeclared"
                });
            }
            if assertion["kind"] == "event" {
                proposal["storyEvent"] = json!({
                    "evaluationTarget":{"snapshotId":null,"sourceBatchId":batch_id,"assertionIndex":index},
                    "eventId":assertion["subjectId"],"eventType":assertion["predicate"],
                    "storyTime":assertion["storyTime"],"untilExclusive":assertion["value"]["untilExclusive"],
                    "participants":assertion["value"]["participants"],
                    "preconditions":assertion["value"]["preconditions"],"effects":assertion["value"]["effects"],
                    "temporalAssessment":crate::event_conditions::assess(assertion,&assertions,&crate::event_sources::resolve(connection, project_id, ModelSourceScope::Current, batch_id, assertion, &entities)?)?,
                    "conditionAssessment":crate::condition_evaluation_jobs::resolve_assessment(connection,project_id,crate::ConditionEvaluationTarget{snapshot_id:None,source_batch_id:batch_id.into(),assertion_index:index},|| {
                        if condition_sources.is_none() && rules.event_rule_count()!=0 {
                            condition_sources=Some(crate::event_rule_sources::collect(connection,project_id,ModelSourceScope::Current,&identity,&rules)?);
                        }
                        crate::event_rule_evaluation::assess(assertion,&rules,&identity,condition_sources.as_ref(),&mut condition_budget)
                    })?
                });
            }
            Ok(proposal)
        })
        .collect::<Result<Vec<_>, RepositoryError>>()?;
    serde_json::to_string(&proposals).map_err(|_| RepositoryError::CorruptState)
}

pub(crate) fn validate_confirmed_modeling_proposals(
    connection: &Connection,
    project_id: &ProjectId,
    object_id: &ObjectId,
    revision_id: &RevisionId,
    proposals_json: &str,
    selections_json: &str,
) -> Result<(), RepositoryError> {
    let proposals: Vec<Value> =
        serde_json::from_str(proposals_json).map_err(|_| RepositoryError::CorruptState)?;
    let selections: Value = serde_json::from_str(selections_json)
        .map_err(|_| RepositoryError::InvalidContent("review selections are invalid".into()))?;
    let decisions = selections["factDecisions"].as_object();
    let selected_groups = selections["sceneGroupIds"].as_array();
    for proposal in proposals {
        let Some(batch_id) = proposal["modelingBatchId"].as_str() else {
            continue;
        };
        let fact_id = proposal["id"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        if decisions
            .and_then(|values| values.get(fact_id))
            .and_then(Value::as_str)
            != Some("confirmed")
        {
            continue;
        }
        let scene_group_id = proposal["sceneGroupId"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?;
        if !selected_groups.is_some_and(|groups| {
            groups
                .iter()
                .any(|group| group.as_str() == Some(scene_group_id))
        }) {
            return Err(RepositoryError::ReviewChecksFailed);
        }
        let (assertions_json, entities_json) = connection.query_row(
            "SELECT b.assertions_json, b.entities_json FROM story_modeling_batches_v2 b JOIN story_knowledge_heads_v2 h ON h.project_id = b.project_id AND h.batch_id = b.batch_id AND h.object_id = b.object_id WHERE b.project_id = ?1 AND b.object_id = ?2 AND b.source_revision_id = ?3 AND b.batch_id = ?4 AND b.status = 'ready'",
            params![project_id.as_str(), object_id.as_str(), revision_id.as_str(), batch_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ).optional().map_err(storage_error)?.ok_or(RepositoryError::ReviewChecksFailed)?;
        let assertions: Vec<Value> =
            serde_json::from_str(&assertions_json).map_err(|_| RepositoryError::CorruptState)?;
        let entities: Vec<Value> =
            serde_json::from_str(&entities_json).map_err(|_| RepositoryError::CorruptState)?;
        crate::story_events::validate_batch(&assertions, &entities)
            .map_err(|_| RepositoryError::CorruptState)?;
        crate::event_sources::validate_batch(
            connection,
            project_id,
            ModelSourceScope::Current,
            batch_id,
            &assertions,
            &entities,
        )
        .map_err(|_| RepositoryError::CorruptState)?;
        let index = proposal["modelingAssertionIndex"]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(RepositoryError::CorruptState)?;
        if assertions.get(index) != proposal.get("modelingAssertion") {
            return Err(RepositoryError::ReviewChecksFailed);
        }
        let assertion = &assertions[index];
        if let Some(source_batch_id) = assertion["propositionSourceBatchId"].as_str() {
            let proposition_id = assertion["propositionId"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?;
            if !proposition_source_visible(
                connection,
                project_id,
                ModelSourceScope::Current,
                source_batch_id,
                proposition_id,
            )? {
                return Err(RepositoryError::ReviewChecksFailed);
            }
        }
    }
    Ok(())
}

fn storage_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
