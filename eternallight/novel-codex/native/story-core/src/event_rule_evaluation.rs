use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::RepositoryError;
use crate::event_rule_sources::ConditionSources;
use crate::event_rules::{Condition, Operand, Operator, Quantifier};
use crate::identity_decisions::IdentityResolution;
use crate::property_rules::PropertyRules;
use crate::state_properties::{Cardinality, parse_schema};

#[path = "event_condition_accumulator.rs"]
mod accumulator;
use accumulator::{ComparisonAccumulator, ComparisonContext};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Truth {
    support: bool,
    refute: bool,
}

impl Truth {
    fn label(self) -> &'static str {
        match (self.support, self.refute) {
            (true, false) => "supported",
            (false, true) => "refuted",
            (true, true) => "both",
            (false, false) => "unknown",
        }
    }
}

fn combine(values: &[Truth], quantifier: &Quantifier) -> Truth {
    if values.is_empty() {
        return Truth::default();
    }
    match quantifier {
        Quantifier::All => Truth {
            support: values.iter().all(|v| v.support),
            refute: values.iter().any(|v| v.refute),
        },
        Quantifier::Any => Truth {
            support: values.iter().any(|v| v.support),
            refute: values.iter().all(|v| v.refute),
        },
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Evidence {
    source_batch_id: String,
    assertion_index: usize,
}

/// A bounded rule checkpoint, not a caller-supplied authorization or proof.
/// The owning job additionally binds source scope/epochs and advances its cursor
/// atomically; each page must be processed once with cumulative source coverage.
#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConditionProgress {
    input_hash: Option<String>,
    comparisons: BTreeMap<String, ComparisonAccumulator>,
    subject_kinds: BTreeMap<String, String>,
}

pub(crate) struct ConditionPageAssessment {
    pub report: Value,
    evaluation_complete: bool,
    /// Do not commit the source-page cursor when comparison work stopped early.
    pub budget_exhausted: bool,
}

impl ConditionPageAssessment {
    /// Publish a bounded summary only when its comparisons have a durable,
    /// authorized explanation endpoint owned by this evaluation job.
    pub(crate) fn into_durable_report(mut self, job_id: &str) -> Result<Value, RepositoryError> {
        if self.budget_exhausted {
            return Err(RepositoryError::CorruptState);
        }
        self.report["coverage"]["complete"] = json!(self.evaluation_complete);
        self.report["result"] = if self.evaluation_complete {
            self.report["observedResult"].clone()
        } else {
            json!("unknown")
        };
        self.report["explanationJobId"] = json!(job_id);
        self.report["sourceClaimsTruncated"] = self.report["coverage"]["evidenceTruncated"].clone();
        if self.report.to_string().len() > 6144 {
            return Err(RepositoryError::CorruptState);
        }
        Ok(self.report)
    }
}

impl ConditionProgress {
    pub(crate) fn explanations(&self) -> Result<Vec<Value>, RepositoryError> {
        if self.input_hash.is_none() || self.comparisons.len() > 8192 {
            return Err(RepositoryError::CorruptState);
        }
        self.comparisons.iter().map(|(key,comparison)| {
            let (leaf,subject,operand):(usize,String,Value)=serde_json::from_str(key).map_err(|_|RepositoryError::CorruptState)?;
            Ok(json!({"leafIndex":leaf,"subjectId":subject,"operand":operand,"assessment":comparison.explanation()?}))
        }).collect()
    }
}

struct Evaluation<'a> {
    sources: &'a ConditionSources,
    rules: &'a PropertyRules,
    identity: &'a IdentityResolution,
    event: &'a Value,
    remaining: &'a mut usize,
    evidence: Vec<Evidence>,
    seen: HashSet<Evidence>,
    gaps: usize,
    evidence_truncated: bool,
    progress: &'a mut ConditionProgress,
    leaf_index: usize,
    budget_exhausted: bool,
}

/// Interpret author-adopted rules against source claims, not confirmed world truth.
/// The shared request budget bounds participant expansion and conflicting sources.
pub(crate) fn assess(
    event: &Value,
    rules: &PropertyRules,
    identity: &IdentityResolution,
    sources: Option<&ConditionSources>,
    remaining: &mut usize,
) -> Result<Value, RepositoryError> {
    let page = assess_with_progress(
        event,
        rules,
        identity,
        sources,
        remaining,
        &mut ConditionProgress::default(),
    )?;
    debug_assert!(!page.budget_exhausted || page.report["coverage"]["complete"] == false);
    Ok(page.report)
}

pub(crate) fn assess_with_progress(
    event: &Value,
    rules: &PropertyRules,
    identity: &IdentityResolution,
    sources: Option<&ConditionSources>,
    remaining: &mut usize,
    progress: &mut ConditionProgress,
) -> Result<ConditionPageAssessment, RepositoryError> {
    let Some(rule) = rules
        .event_rules
        .iter()
        .find(|rule| event["predicate"].as_str() == Some(rule.event_type.as_str()))
    else {
        if progress.input_hash.is_some() {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        return Ok(ConditionPageAssessment {
            report: json!({"basis":"authorRulesSourceClaims","status":"noApplicableRule","result":"unknown","observedResult":"unknown","ruleRevisionId":rules.revision_id,"identityRevisionId":identity.revision_id,"stateChangesExecuted":false,"worldTruthCertified":false}),
            budget_exhausted: false,
            evaluation_complete: false,
        });
    };
    let sources = sources.ok_or(RepositoryError::CorruptState)?;
    let definitions=rules.definitions.iter().map(|definition|json!({"subjectKind":definition.subject_kind,"predicate":definition.predicate,"schema":definition.schema,"exceptions":definition.exceptions})).collect::<Vec<_>>();
    let input = json!({"event":event,"rule":rule,"definitions":definitions,"ruleRevisionId":rules.revision_id,"identityRevisionId":identity.revision_id,"identity":identity.decisions,"sourceProofId":sources.coverage["sourceProofId"]});
    let input_hash = format!("{:x}", Sha256::digest(input.to_string().as_bytes()));
    if progress
        .input_hash
        .as_ref()
        .is_some_and(|previous| previous != &input_hash)
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if progress.comparisons.len() > 8192
        || progress.subject_kinds.len() > 16
        || (progress.input_hash.is_none()
            && (!progress.comparisons.is_empty() || !progress.subject_kinds.is_empty()))
    {
        return Err(RepositoryError::CorruptState);
    }
    progress.input_hash = Some(input_hash);
    let participants = event["value"]["participants"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?;
    if participants.len() > 16 {
        return Err(RepositoryError::CorruptState);
    }
    for participant in participants {
        let id = identity.canonical_id(
            participant["entityId"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?,
        );
        if let Some(kind) = sources.kinds.get(&id)
            && progress
                .subject_kinds
                .insert(id, kind.clone())
                .is_some_and(|previous| previous != *kind)
        {
            return Err(RepositoryError::CorruptState);
        }
    }
    let mut evaluation = Evaluation {
        sources,
        rules,
        identity,
        event,
        remaining,
        evidence: Vec::new(),
        seen: HashSet::new(),
        gaps: 0,
        evidence_truncated: false,
        progress,
        leaf_index: 0,
        budget_exhausted: false,
    };
    let observed = if rule.exceptions.is_empty() {
        evaluation.condition(&rule.condition)?
    } else {
        evaluation.gaps += 1;
        Truth::default()
    };
    let complete = sources.complete && evaluation.gaps == 0 && !evaluation.evidence_truncated;
    let report = json!({"basis":"authorRulesSourceClaims","status":"interpreted","result":if complete { observed.label() } else { "unknown" },"observedResult":observed.label(),
        "ruleRevisionId":rules.revision_id,"identityRevisionId":identity.revision_id,
        "sourceClaims":evaluation.evidence,"coverage":{"complete":complete,"sources":sources.coverage,"unresolvedChecks":evaluation.gaps,"evidenceTruncated":evaluation.evidence_truncated},
        "stateChangesExecuted":false,"worldTruthCertified":false});
    if report.to_string().len() > 6144 {
        return Err(RepositoryError::CorruptState);
    }
    Ok(ConditionPageAssessment {
        report,
        evaluation_complete: sources.complete && evaluation.gaps == 0,
        budget_exhausted: evaluation.budget_exhausted,
    })
}

impl Evaluation<'_> {
    fn participants(&self, role: &str) -> Vec<String> {
        let mut ids = self.event["value"]["participants"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|participant| participant["role"] == role)
            .filter_map(|participant| participant["entityId"].as_str())
            .map(|id| self.identity.canonical_id(id))
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        ids
    }

    fn condition(&mut self, condition: &Condition) -> Result<Truth, RepositoryError> {
        match condition {
            Condition::All { conditions } | Condition::Any { conditions } => {
                let values = conditions
                    .iter()
                    .map(|condition| self.condition(condition))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(combine(
                    &values,
                    if matches!(condition, Condition::All { .. }) {
                        &Quantifier::All
                    } else {
                        &Quantifier::Any
                    },
                ))
            }
            Condition::Not { condition } => {
                let value = self.condition(condition)?;
                Ok(Truth {
                    support: value.refute,
                    refute: value.support,
                })
            }
            Condition::State {
                role,
                subject_kind,
                predicate,
                quantifier,
                operator,
                operand,
            } => {
                let leaf_index = self.leaf_index;
                self.leaf_index += 1;
                let definition = self
                    .rules
                    .definitions
                    .iter()
                    .find(|definition| {
                        definition.subject_kind == *subject_kind
                            && definition.predicate == *predicate
                    })
                    .ok_or(RepositoryError::CorruptState)?;
                if !definition.exceptions.is_empty() {
                    self.gaps += 1;
                    return Ok(Truth::default());
                }
                let schema =
                    parse_schema(&definition.schema).map_err(|_| RepositoryError::CorruptState)?;
                if matches!(schema.cardinality, Cardinality::Multiple)
                    && !matches!(operator, Operator::Eq | Operator::Ne)
                {
                    self.gaps += 1;
                    return Ok(Truth::default());
                }
                let subjects = self.participants(role);
                let (operands, operand_quantifier) = match operand {
                    Operand::Boolean { value } => (vec![json!(value)], &Quantifier::All),
                    Operand::Integer { value } => (vec![json!(value)], &Quantifier::All),
                    Operand::Text { value } => (vec![json!(value)], &Quantifier::All),
                    Operand::Participant { role, quantifier } => (
                        self.participants(role)
                            .into_iter()
                            .map(|id| json!({"entityId":id}))
                            .collect(),
                        quantifier,
                    ),
                };
                if subjects.is_empty() || operands.is_empty() {
                    self.gaps += 1;
                    return Ok(Truth::default());
                }
                let mut values = Vec::new();
                for subject in subjects {
                    if self
                        .progress
                        .subject_kinds
                        .get(&subject)
                        .is_none_or(|kind| kind != subject_kind)
                    {
                        self.gaps += 1;
                        values.push(Truth::default());
                        continue;
                    }
                    let states = self
                        .sources
                        .states
                        .get(&(subject.clone(), predicate.clone()));
                    let mut comparisons = Vec::new();
                    for operand in &operands {
                        let context = ComparisonContext {
                            time: self.event["storyTime"].as_i64(),
                            schema: &schema,
                            operand,
                            operator,
                            identity: self.identity,
                            kinds: &self.sources.kinds,
                        };
                        let key = json!([leaf_index, subject, operand]).to_string();
                        if self.progress.comparisons.len() >= 8192
                            && !self.progress.comparisons.contains_key(&key)
                        {
                            return Err(RepositoryError::CorruptState);
                        }
                        let progress = self.progress.comparisons.entry(key).or_default();
                        for state in states.into_iter().flatten() {
                            if *self.remaining == 0 {
                                self.budget_exhausted = true;
                                self.gaps += 1;
                                break;
                            }
                            *self.remaining -= 1;
                            progress.observe(state, &context)?;
                        }
                        for evidence in progress.witnesses() {
                            if !self.seen.contains(evidence) {
                                if self.evidence.len() == 16 {
                                    self.evidence_truncated = true;
                                } else {
                                    self.seen.insert(evidence.clone());
                                    self.evidence.push(evidence.clone());
                                }
                            }
                        }
                        self.gaps = self
                            .gaps
                            .checked_add(progress.final_unresolved_checks()?)
                            .ok_or(RepositoryError::CorruptState)?;
                        comparisons.push(progress.truth());
                    }
                    values.push(combine(&comparisons, operand_quantifier));
                }
                Ok(combine(&values, quantifier))
            }
        }
    }
}

fn compare(
    value: &Value,
    operand: &Value,
    operator: &Operator,
    cardinality: &Cardinality,
    identity: &IdentityResolution,
) -> Truth {
    let equality = match (value["entityId"].as_str(), operand["entityId"].as_str()) {
        (Some(left), Some(right))
            if left != right && !identity.explicitly_distinct(left, right) =>
        {
            return Truth::default();
        }
        _ => value == operand,
    };
    if matches!(cardinality, Cardinality::Multiple) && !equality {
        return Truth::default();
    }
    let result = match operator {
        Operator::Eq => equality,
        Operator::Ne => !equality,
        Operator::Lt => value.as_i64() < operand.as_i64(),
        Operator::Le => value.as_i64() <= operand.as_i64(),
        Operator::Gt => value.as_i64() > operand.as_i64(),
        Operator::Ge => value.as_i64() >= operand.as_i64(),
    };
    Truth {
        support: result,
        refute: !result,
    }
}

#[cfg(test)]
#[path = "event_rule_evaluation_tests.rs"]
mod tests;
