use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::RepositoryError;
use crate::state_properties::{PropertySchema, ValueType};
use crate::story_events::role_kinds;

/// An author-adoptable declaration, not a certificate of event truth or execution.
#[derive(Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventRule {
    pub(crate) event_type: String,
    description: String,
    pub(crate) exceptions: Vec<String>,
    pub(crate) condition: Condition,
}

#[derive(Deserialize, Serialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Condition {
    All {
        conditions: Vec<Condition>,
    },
    Any {
        conditions: Vec<Condition>,
    },
    Not {
        condition: Box<Condition>,
    },
    State {
        role: String,
        #[serde(rename = "subjectKind")]
        subject_kind: String,
        predicate: String,
        quantifier: Quantifier,
        operator: Operator,
        operand: Operand,
    },
}

#[derive(Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Quantifier {
    All,
    Any,
}

#[derive(Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Operator {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Deserialize, Serialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Operand {
    Boolean {
        value: bool,
    },
    Integer {
        value: i64,
    },
    Text {
        value: String,
    },
    Participant {
        role: String,
        quantifier: Quantifier,
    },
}

pub(crate) fn validate(
    rules: &[EventRule],
    schemas: &HashMap<(&str, &str), PropertySchema>,
) -> Result<(), RepositoryError> {
    if rules.len() > 16 {
        return Err(invalid());
    }
    let mut keys = HashSet::new();
    for rule in rules {
        if !bounded_text(&rule.event_type, /*limit*/ 80)
            || !bounded_text(&rule.description, /*limit*/ 512)
            || rule.exceptions.len() > 8
            || rule
                .exceptions
                .iter()
                .any(|text| !bounded_text(text, /*limit*/ 256))
            || !keys.insert(&rule.event_type)
            || serde_json::to_vec(rule).map_err(|_| invalid())?.len() > 4096
        {
            return Err(invalid());
        }
        let mut nodes = 0;
        rule.condition.validate(schemas, /*depth*/ 0, &mut nodes)?;
    }
    Ok(())
}

impl Condition {
    fn validate(
        &self,
        schemas: &HashMap<(&str, &str), PropertySchema>,
        depth: usize,
        nodes: &mut usize,
    ) -> Result<(), RepositoryError> {
        *nodes += 1;
        if depth > 4 || *nodes > 31 {
            return Err(invalid());
        }
        match self {
            Self::All { conditions } | Self::Any { conditions } => {
                if conditions.is_empty() || conditions.len() > 8 {
                    return Err(invalid());
                }
                for condition in conditions {
                    condition.validate(schemas, depth + 1, nodes)?;
                }
            }
            Self::Not { condition } => condition.validate(schemas, depth + 1, nodes)?,
            Self::State {
                role,
                subject_kind,
                predicate,
                operator,
                operand,
                ..
            } => {
                let schema = schemas
                    .get(&(subject_kind.as_str(), predicate.as_str()))
                    .ok_or_else(invalid)?;
                if !role_kinds(role).contains(&subject_kind.as_str())
                    || !operand.matches(&schema.value_type)
                    || (!matches!(operator, Operator::Eq | Operator::Ne)
                        && !matches!(schema.value_type, ValueType::Integer))
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

impl Operand {
    fn matches(&self, value_type: &ValueType) -> bool {
        match (self, value_type) {
            (Self::Boolean { .. }, ValueType::Boolean) => true,
            (Self::Integer { value }, ValueType::Integer) => {
                (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(value)
            }
            (Self::Text { value }, ValueType::Text) => bounded_text(value, /*limit*/ 512),
            (Self::Participant { role, .. }, ValueType::Entity { kind }) => {
                role_kinds(role).contains(&kind.as_str())
            }
            _ => false,
        }
    }
}

fn bounded_text(text: &str, limit: usize) -> bool {
    !text.trim().is_empty() && text.len() <= limit
}

fn invalid() -> RepositoryError {
    RepositoryError::InvalidContent("event rules require bounded typed conditions, declared properties and compatible participant roles".into())
}

#[cfg(test)]
#[path = "event_rules_tests.rs"]
mod tests;
