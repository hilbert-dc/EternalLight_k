use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::RepositoryError;

/// The literal baseline or a Codex model's candidate judgment over that exact report.
#[derive(Clone, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum QueryRanking {
    Literal,
    ModelSemantic {
        base_report_hash: String,
        judgments: Vec<ModelRelevanceJudgment>,
    },
}

/// Relevance is model advice, never source evidence or permission to alter facts.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelRelevanceJudgment {
    pub result_id: String,
    pub score: u16,
    pub rationale: String,
}

impl QueryRanking {
    pub(crate) fn validate(&self) -> Result<(), RepositoryError> {
        if let Self::ModelSemantic {
            base_report_hash,
            judgments,
        } = self
        {
            let mut ids = BTreeSet::new();
            if !digest_valid(base_report_hash)
                || !(1..=64).contains(&judgments.len())
                || judgments.iter().any(|judgment| {
                    !digest_valid(&judgment.result_id)
                        || !ids.insert(&judgment.result_id)
                        || judgment.score > 1000
                        || judgment.rationale.trim().is_empty()
                        || judgment.rationale.len() > 256
                })
            {
                return Err(RepositoryError::InvalidContent(
                    "invalid semantic judgments".into(),
                ));
            }
        }
        Ok(())
    }
}

fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn apply(report: &mut Value, ranking: &QueryRanking) -> Result<(), RepositoryError> {
    let QueryRanking::ModelSemantic {
        base_report_hash,
        judgments,
    } = ranking
    else {
        return Ok(());
    };
    ranking.validate()?;
    if report["contentHash"].as_str() != Some(base_report_hash) {
        return Err(RepositoryError::InvalidContent(
            "semantic baseline changed; query again".into(),
        ));
    }
    let supplied = judgments
        .iter()
        .map(|judgment| (judgment.result_id.as_str(), judgment))
        .collect::<BTreeMap<_, _>>();
    let expected = report["steps"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?
        .iter()
        .flat_map(|step| step["results"].as_array().into_iter().flatten())
        .map(|row| {
            row["resultId"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if expected.is_empty()
        || expected.len() != supplied.len()
        || expected.iter().any(|id| !supplied.contains_key(id))
    {
        return Err(RepositoryError::InvalidContent(
            "semantic judgments must cover exactly the returned results".into(),
        ));
    }
    for step in report["steps"]
        .as_array_mut()
        .ok_or(RepositoryError::CorruptState)?
    {
        let rows = step["results"]
            .as_array_mut()
            .ok_or(RepositoryError::CorruptState)?;
        for row in rows.iter_mut() {
            let id = row["resultId"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?;
            let judgment = supplied.get(id).ok_or(RepositoryError::CorruptState)?;
            row["literalRelevanceScore"] = row["relevanceScore"].clone();
            row["relevanceScore"] = json!(judgment.score);
            row["modelRelevanceRationale"] = json!(judgment.rationale);
        }
        // Equal scores retain the baseline order; no implicit new authority tie-breaker.
        rows.sort_by_key(|row| {
            std::cmp::Reverse(row["relevanceScore"].as_u64().unwrap_or_default())
        });
    }
    report["ranking"] = json!({"method":"codexModelJudgment","semantic":true,"authority":"modelCandidate",
        "baseReportHash":base_report_hash,"judgedItems":judgments.len(),"qualityVerified":false,
        "selection":"channelFloorThenLowestScore","candidateScope":"authorizedBaselineResults"});
    Ok(())
}

#[cfg(test)]
#[path = "query_semantic_tests.rs"]
mod tests;
