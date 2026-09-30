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
    pub max_requests: u64,
    pub max_tokens: u64,
    pub used_requests: u64,
    pub used_tokens: u64,
    pub unknown_tokens: u64,
    pub overrun: bool,
    pub charges: Vec<ProviderUsageCharge>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BudgetLedger {
    max_requests: u64,
    max_tokens: u64,
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
        if self.max_requests == 0 || self.max_tokens == 0 || self.charges.len() > 32 {
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
        overrun |= requests > self.max_requests || tokens.saturating_add(unknown) > self.max_tokens;
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

    pub fn remaining_requests(&self) -> u64 {
        self.max_requests.saturating_sub(self.used_requests)
    }

    pub fn remaining_tokens(&self) -> u64 {
        self.max_tokens
            .saturating_sub(self.used_tokens.saturating_add(self.unknown_tokens))
    }

    pub fn exhausted(&self) -> bool {
        self.overrun
            || self.charges.len() >= 32
            || self.remaining_requests() == 0
            || self.remaining_tokens() == 0
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
    if receipt["runId"] != task_id
        || receipt["usage"]["overrun"].as_bool().is_none()
        || integer(&receipt["limits"]["maxRequests"])? == 0
        || integer(&receipt["limits"]["maxTokens"])? == 0
        || integer(&receipt["limits"]["maxDurationMs"])? == 0
    {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let requests = integer(&receipt["usage"]["requests"])?;
    if requests > integer(&receipt["limits"]["maxRequests"])? {
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
        if self.charges.len() >= 32
            || self
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
            || requests > self.max_requests
            || tokens.saturating_add(unknown_tokens) > self.max_tokens;
        self.used_requests = requests;
        self.used_tokens = tokens;
        self.unknown_tokens = unknown_tokens;
        self.overrun |= overrun;
        self.charges.push(charge);
        Ok(UsageChargeOutcome::Applied)
    }
}
