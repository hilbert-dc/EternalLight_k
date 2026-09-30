use rusqlite::types::Value as SqlValue;
use serde_json::Value;

use crate::relation_types::RELATION_TYPES;
use crate::{KnowledgePerspective, KnowledgeStateQuery};

pub(crate) struct StateSelector<'a> {
    pub query: &'a KnowledgeStateQuery<'a>,
    pub entity_ids: Vec<String>,
    pub owner_ids: Vec<String>,
    pub visible_through_chapter: Option<i64>,
}

impl StateSelector<'_> {
    pub fn matches_selectors(&self, assertion: &Value) -> bool {
        self.query
            .predicate
            .is_none_or(|predicate| assertion["predicate"] == predicate)
            && (self.entity_ids.is_empty()
                || ["subjectId", "targetId", "epistemicOwnerId"]
                    .iter()
                    .any(|key| {
                        assertion[key]
                            .as_str()
                            .is_some_and(|id| self.entity_ids.iter().any(|entity| entity == id))
                    })
                || crate::state_properties::target_entity(assertion)
                    .is_some_and(|id| self.entity_ids.iter().any(|entity| entity == id))
                || crate::story_events::participant_ids(assertion)
                    .any(|id| self.entity_ids.iter().any(|entity| entity == id)))
    }

    pub fn sql(&self, args: &mut Vec<SqlValue>) -> String {
        let mut filters = Vec::new();
        if let Some(predicate) = self.query.predicate {
            filters.push("json_extract(assertion.value, '$.predicate') = ?".to_owned());
            args.push(SqlValue::Text(predicate.to_owned()));
        }
        if !self.entity_ids.is_empty() {
            let placeholders = vec!["?"; self.entity_ids.len()].join(",");
            filters.push(format!("(json_extract(assertion.value, '$.subjectId') IN ({placeholders}) OR json_extract(assertion.value, '$.targetId') IN ({placeholders}) OR json_extract(assertion.value, '$.epistemicOwnerId') IN ({placeholders}) OR (json_extract(assertion.value, '$.kind')='state' AND json_extract(assertion.value, '$.stateSchema.valueType.type')='entity' AND json_extract(assertion.value, '$.value.entityId') IN ({placeholders})) OR (json_extract(assertion.value, '$.kind')='event' AND EXISTS(SELECT 1 FROM json_each(assertion.value, '$.value.participants') participant WHERE json_extract(participant.value,'$.entityId') IN ({placeholders}))))"));
            for _ in 0..5 {
                args.extend(self.entity_ids.iter().cloned().map(SqlValue::Text));
            }
        }
        match self.query.perspective {
            KnowledgePerspective::Character => {
                let placeholders = vec!["?"; self.owner_ids.len()].join(",");
                filters.push(format!("json_extract(assertion.value, '$.kind') = 'epistemic' AND json_extract(assertion.value, '$.epistemicOwnerId') IN ({placeholders})"));
                args.extend(self.owner_ids.iter().cloned().map(SqlValue::Text));
            }
            KnowledgePerspective::World => {
                let names = RELATION_TYPES
                    .iter()
                    .filter(|(_, definition)| matches!(definition.category, "world" | "causal"))
                    .map(|(name, _)| SqlValue::Text((*name).to_owned()))
                    .collect::<Vec<_>>();
                let placeholders = vec!["?"; names.len()].join(",");
                filters.push(format!("json_extract(assertion.value, '$.kind') != 'epistemic' AND (json_extract(assertion.value, '$.kind') != 'relation' OR json_extract(assertion.value, '$.relationType') IN ({placeholders}))"));
                args.extend(names);
            }
            KnowledgePerspective::Author | KnowledgePerspective::Reader => {}
        }
        if let Some(time) = self.query.world_time {
            filters.push("(json_extract(assertion.value, '$.kind') = 'epistemic' OR (json_extract(assertion.value, '$.kind')='state' AND (json_type(assertion.value,'$.validFrom') IS NULL OR json_type(assertion.value,'$.validFrom') NOT IN ('integer','null') OR (json_type(assertion.value,'$.validUntilExclusive') IS NOT NULL AND (json_type(assertion.value,'$.validUntilExclusive') IS NOT 'integer' OR json_type(assertion.value,'$.validFrom') IS NOT 'integer' OR json_extract(assertion.value,'$.validFrom')>=json_extract(assertion.value,'$.validUntilExclusive') OR json_type(assertion.value,'$.validUntilEvidence') IS NOT 'object')) OR (json_type(assertion.value,'$.validUntilEvidence') IS NOT NULL AND json_type(assertion.value,'$.validUntilExclusive') IS NULL))) OR ((CASE WHEN json_extract(assertion.value, '$.kind') IN ('relation','state') THEN COALESCE(json_extract(assertion.value, '$.validFrom'), json_extract(assertion.value, '$.storyTime')) ELSE json_extract(assertion.value, '$.storyTime') END IS NULL OR CASE WHEN json_extract(assertion.value, '$.kind') IN ('relation','state') THEN COALESCE(json_extract(assertion.value, '$.validFrom'), json_extract(assertion.value, '$.storyTime')) ELSE json_extract(assertion.value, '$.storyTime') END <= ?) AND (json_extract(assertion.value, '$.kind') NOT IN ('relation','state') OR json_extract(assertion.value, '$.validUntilExclusive') IS NULL OR json_extract(assertion.value, '$.validUntilExclusive') > ?)))".into());
            args.extend([SqlValue::Integer(time), SqlValue::Integer(time)]);
        }
        if let Some(time) = self.query.epistemic_time {
            // Keep malformed intervals in the candidate set so the projection's
            // authoritative validator rejects them instead of claiming no knowledge.
            filters.push("(json_extract(assertion.value, '$.kind') != 'epistemic' OR (CASE WHEN json_type(assertion.value, '$.epistemicUntilExclusive') IS NULL AND json_type(assertion.value, '$.epistemicUntilEvidence') IS NULL THEN 0 WHEN json_type(assertion.value, '$.epistemicTime') = 'integer' AND json_type(assertion.value, '$.epistemicUntilExclusive') = 'integer' AND json_type(assertion.value, '$.epistemicUntilEvidence') = 'object' AND json_extract(assertion.value, '$.epistemicTime') < json_extract(assertion.value, '$.epistemicUntilExclusive') THEN 0 ELSE 1 END = 1 OR ((json_type(assertion.value, '$.epistemicTime') IS NOT 'integer' OR json_extract(assertion.value, '$.epistemicTime') <= ?) AND (json_extract(assertion.value, '$.epistemicUntilExclusive') IS NULL OR json_extract(assertion.value, '$.epistemicUntilExclusive') > ?))))".into());
            args.extend([SqlValue::Integer(time), SqlValue::Integer(time)]);
        }
        if filters.is_empty() {
            "1 = 1".into()
        } else {
            filters.join(" AND ")
        }
    }
}
