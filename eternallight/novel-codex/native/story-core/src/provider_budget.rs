use crate::RepositoryError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageCharge {
    pub task_id: String,
    pub budget_id: String,
    pub status: String,
    pub requests: u64,
    pub tokens: u64,
    pub unknown_tokens: u64,
    pub overrun: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(try_from = "BudgetLedger")]
pub struct ProviderBudget {
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    pub max_requests: Option<u64>,
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    pub max_tokens: Option<u64>,
    pub used_requests: u64,
    pub used_tokens: u64,
    pub unknown_tokens: u64,
    pub overrun: bool,
    pub charges: Vec<ProviderUsageCharge>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BudgetLedger {
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    max_requests: Option<u64>,
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    max_tokens: Option<u64>,
    used_requests: u64,
    used_tokens: u64,
    unknown_tokens: u64,
    overrun: bool,
    charges: Vec<ProviderUsageCharge>,
}

impl TryFrom<BudgetLedger> for ProviderBudget {
    type Error = &'static str;

    fn try_from(ledger: BudgetLedger) -> Result<Self, Self::Error> {
        let budget = Self {
            max_requests: ledger.max_requests,
            max_tokens: ledger.max_tokens,
            used_requests: ledger.used_requests,
            used_tokens: ledger.used_tokens,
            unknown_tokens: ledger.unknown_tokens,
            overrun: ledger.overrun,
            charges: ledger.charges,
        };
        budget
            .validate()
            .map_err(|_| "inconsistent provider budget ledger")?;
        Ok(budget)
    }
}

impl ProviderBudget {
    fn validate(&self) -> Result<(), RepositoryError> {
        if self.max_requests == Some(0) || self.max_tokens == Some(0) {
            return Err(RepositoryError::CorruptState);
        }
        let mut requests = 0_u64;
        let mut tokens = 0_u64;
        let mut unknown = 0_u64;
        let mut overrun = false;
        let mut tasks = std::collections::HashSet::new();
        let mut budgets = std::collections::HashSet::new();
        for charge in &self.charges {
            if charge.task_id.is_empty()
                || charge.task_id.len() > 160
                || charge.budget_id.is_empty()
                || charge.budget_id.len() > 160
                || !tasks.insert(&charge.task_id)
                || !budgets.insert(&charge.budget_id)
                || !matches!(
                    charge.status.as_str(),
                    "terminal"
                        | "completed"
                        | "failed"
                        | "cancelled"
                        | "outcome-unknown"
                        | "recovered_unknown"
                )
            {
                return Err(RepositoryError::CorruptState);
            }
            requests = requests
                .checked_add(charge.requests)
                .ok_or(RepositoryError::CorruptState)?;
            tokens = tokens
                .checked_add(charge.tokens)
                .ok_or(RepositoryError::CorruptState)?;
            unknown = unknown
                .checked_add(charge.unknown_tokens)
                .ok_or(RepositoryError::CorruptState)?;
            overrun |= charge.overrun;
        }
        overrun |= self.max_requests.is_some_and(|limit| requests > limit)
            || self
                .max_tokens
                .is_some_and(|limit| tokens.saturating_add(unknown) > limit);
        if (requests, tokens, unknown, overrun)
            != (
                self.used_requests,
                self.used_tokens,
                self.unknown_tokens,
                self.overrun,
            )
        {
            return Err(RepositoryError::CorruptState);
        }
        Ok(())
    }

    pub fn remaining_requests(&self) -> Option<u64> {
        self.max_requests
            .map(|limit| limit.saturating_sub(self.used_requests))
    }

    pub fn remaining_tokens(&self) -> Option<u64> {
        self.max_tokens
            .map(|limit| limit.saturating_sub(self.used_tokens.saturating_add(self.unknown_tokens)))
    }

    pub fn exhausted(&self) -> bool {
        self.overrun || self.remaining_requests() == Some(0) || self.remaining_tokens() == Some(0)
    }

    pub fn has_charge(&self, task_id: &str) -> bool {
        self.charges.iter().any(|charge| charge.task_id == task_id)
    }
}

fn integer(value: &Value) -> Result<u64, RepositoryError> {
    let number = value.as_f64().ok_or(RepositoryError::TaskBindingInvalid)?;
    if !number.is_finite()
        || number < 0.0
        || number.fract() != 0.0
        || number > 9_007_199_254_740_991.0
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(number as u64)
}

/// Requires an explicit limit value or null on the current wire contract.
/// Missing authority fields must not silently become unlimited execution.
pub fn deserialize_execution_limit<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

fn limit(value: &Value) -> Result<Option<u64>, RepositoryError> {
    if value.is_null() {
        Ok(None)
    } else {
        integer(value).map(Some)
    }
}

fn charge(task_id: &str, receipt: &Value) -> Result<ProviderUsageCharge, RepositoryError> {
    if task_id.is_empty() || task_id.len() > 160 {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let budget_id = receipt["budgetId"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 160)
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let status = receipt["status"]
        .as_str()
        .filter(|status| {
            matches!(
                *status,
                "terminal"
                    | "completed"
                    | "failed"
                    | "cancelled"
                    | "outcome-unknown"
                    | "recovered_unknown"
            )
        })
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    if ["maxRequests", "maxTokens", "maxDurationMs"]
        .iter()
        .any(|key| receipt["limits"].get(key).is_none())
        || receipt["runId"] != task_id
        || receipt["usage"]["overrun"].as_bool().is_none()
        || limit(&receipt["limits"]["maxRequests"])? == Some(0)
        || limit(&receipt["limits"]["maxTokens"])? == Some(0)
        || limit(&receipt["limits"]["maxDurationMs"])? == Some(0)
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let requests = integer(&receipt["usage"]["requests"])?;
    if limit(&receipt["limits"]["maxRequests"])?.is_some_and(|maximum| requests > maximum) {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    Ok(ProviderUsageCharge {
        task_id: task_id.into(),
        budget_id: budget_id.into(),
        status: status.into(),
        requests,
        tokens: integer(&receipt["usage"]["tokens"])?,
        unknown_tokens: integer(&receipt["usage"]["unknownTokens"])?,
        overrun: receipt["usage"]["overrun"] == true,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageChargeOutcome {
    Applied,
    AlreadyCharged,
}

impl ProviderBudget {
    /// Accounts one terminal attempt. The caller verifies ownership and persists
    /// the resulting budget atomically with its task/run state.
    pub fn charge_receipt(
        &mut self,
        task_id: &str,
        receipt: &Value,
    ) -> Result<UsageChargeOutcome, RepositoryError> {
        self.validate()?;
        let charge = charge(task_id, receipt)?;
        if let Some(existing) = self.charges.iter().find(|item| item.task_id == task_id) {
            return if existing == &charge {
                Ok(UsageChargeOutcome::AlreadyCharged)
            } else {
                Err(RepositoryError::TaskBindingInvalid)
            };
        }
        if self
            .charges
            .iter()
            .any(|item| item.budget_id == charge.budget_id)
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let requests = self
            .used_requests
            .checked_add(charge.requests)
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let tokens = self
            .used_tokens
            .checked_add(charge.tokens)
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let unknown_tokens = self
            .unknown_tokens
            .checked_add(charge.unknown_tokens)
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let overrun = charge.overrun
            || self.max_requests.is_some_and(|limit| requests > limit)
            || self
                .max_tokens
                .is_some_and(|limit| tokens.saturating_add(unknown_tokens) > limit);
        self.used_requests = requests;
        self.used_tokens = tokens;
        self.unknown_tokens = unknown_tokens;
        self.overrun |= overrun;
        self.charges.push(charge);
        Ok(UsageChargeOutcome::Applied)
    }
}
