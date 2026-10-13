//! Bounded state for one leaf/subject/operand comparison. Scope, rule and source
//! epoch binding belong to the owning evaluation job, not to an individual page.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Evidence, Truth, compare};
use crate::RepositoryError;
use crate::event_rule_sources::SourceState;
use crate::event_rules::Operator;
use crate::identity_decisions::IdentityResolution;
use crate::state_properties::{Cardinality, PropertySchema, ValueType, value_valid};

pub(super) struct ComparisonContext<'a> {
    pub time: Option<i64>,
    pub schema: &'a PropertySchema,
    pub operand: &'a Value,
    pub operator: &'a Operator,
    pub identity: &'a IdentityResolution,
    pub kinds: &'a HashMap<String, String>,
}

/// Persist only independent witnesses and unresolved checks, never an aggregate
/// `unknown` for an unfinished page. Each direction retains at most one source.
/// This internal checkpoint is not an authorization token or a completion proof.
#[derive(Default, Debug, PartialEq, Eq, Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ComparisonAccumulator {
    support: Option<Evidence>,
    refute: Option<Evidence>,
    unresolved: usize,
}

impl ComparisonAccumulator {
    pub fn observe(
        &mut self,
        state: &SourceState,
        context: &ComparisonContext<'_>,
    ) -> Result<(), RepositoryError> {
        let assertion = &state.assertion;
        let Some((time, start)) = context.time.zip(assertion["validFrom"].as_i64()) else {
            return self.unresolved();
        };
        if start > time
            || assertion["validUntilExclusive"]
                .as_i64()
                .is_some_and(|end| end < time)
        {
            return Ok(());
        }
        if start == time || !value_valid(context.schema, &assertion["value"]) {
            return self.unresolved();
        }
        let mut value = assertion["value"].clone();
        if let Some(id) = value["entityId"].as_str() {
            value = json!({"entityId":context.identity.canonical_id(id)});
        }
        if let ValueType::Entity { kind } = &context.schema.value_type
            && value["entityId"]
                .as_str()
                .is_none_or(|id| context.kinds.get(id) != Some(kind))
        {
            return self.unresolved();
        }
        if matches!(context.schema.cardinality, Cardinality::Multiple)
            && matches!(context.operator, Operator::Eq | Operator::Ne)
            && value != *context.operand
            && match (
                value["entityId"].as_str(),
                context.operand["entityId"].as_str(),
            ) {
                (Some(left), Some(right)) => context.identity.explicitly_distinct(left, right),
                _ => true,
            }
        {
            return Ok(());
        }
        let comparison = compare(
            &value,
            context.operand,
            context.operator,
            &context.schema.cardinality,
            context.identity,
        );
        if comparison == Truth::default() {
            return self.unresolved();
        }
        for (matched, witness) in [
            (comparison.support, &mut self.support),
            (comparison.refute, &mut self.refute),
        ] {
            if matched && witness.is_none() {
                *witness = Some(Evidence {
                    source_batch_id: state.batch_id.clone(),
                    assertion_index: state.assertion_index,
                });
            }
        }
        Ok(())
    }

    pub fn truth(&self) -> Truth {
        Truth {
            support: self.support.is_some(),
            refute: self.refute.is_some(),
        }
    }

    /// Final report for a completed or abandoned scan, not a page checkpoint.
    /// Missing witnesses on an intermediate page are not persisted as a gap.
    pub fn final_unresolved_checks(&self) -> Result<usize, RepositoryError> {
        self.unresolved
            .checked_add(usize::from(self.truth() == Truth::default()))
            .ok_or(RepositoryError::CorruptState)
    }

    pub fn witnesses(&self) -> impl Iterator<Item = &Evidence> {
        self.support.iter().chain(self.refute.iter())
    }

    pub fn explanation(&self) -> Result<Value, RepositoryError> {
        Ok(
            json!({"result":self.truth().label(),"support":self.support,"refute":self.refute,"unresolvedChecks":self.final_unresolved_checks()?}),
        )
    }

    fn unresolved(&mut self) -> Result<(), RepositoryError> {
        self.unresolved = self
            .unresolved
            .checked_add(1)
            .ok_or(RepositoryError::CorruptState)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "event_condition_accumulator_tests.rs"]
mod tests;
