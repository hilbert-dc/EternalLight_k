use serde_json::Value;

pub(super) struct Failure {
    pub(super) message: String,
    pub(super) status: u16,
    pub(super) auth: bool,
    pub(super) downstream: Value,
    pub(super) budget_code: Option<&'static str>,
    pub(super) stream_code: Option<&'static str>,
    pub(super) retryable: bool,
}

impl Failure {
    pub(super) fn diagnostic_code(&self) -> &'static str {
        if let Some(code) = self.budget_code {
            return code;
        }
        if let Some(code) = self.stream_code {
            return code;
        }
        if !self.downstream.is_null() {
            return "provider_http_error";
        }
        match self.message.as_str() {
            "engine HTTP connection failed" => "provider_connection_failed",
            "Responses stream tool output differs from terminal output" => "stream_tool_mismatch",
            "Responses output did not complete successfully"
            | "Responses stream did not complete successfully" => "response_incomplete",
            "Responses stream ended without completion" => "response_stream_disconnected",
            message
                if message.starts_with("arguments for ")
                    && message.contains(" violate the declared schema:") =>
            {
                "invalid_tool_arguments"
            }
            message if message.ends_with(" exceeds the 10K-token conservative item limit") => {
                "context_item_limit"
            }
            _ => "bridge_error",
        }
    }

    pub(super) fn reservation(message: String) -> Self {
        // Only classify the budget owner's exact, fixed diagnostics. Arbitrary
        // host failures must not masquerade as an exhausted author allowance.
        let code = match message.as_str() {
            "task token budget exhausted" => "task_token_budget_exhausted",
            "task request budget exhausted" => "task_request_budget_exhausted",
            "task duration budget exhausted" => "task_duration_budget_exhausted",
            _ => return message.into(),
        };
        Self {
            message,
            // Codex's sampling layer retries unexpected HTTP statuses even
            // after the HTTP client stops retrying. 400 is its terminal
            // InvalidRequest path; retain the specific budget code in the body.
            status: 400,
            auth: false,
            downstream: Value::Null,
            budget_code: Some(code),
            stream_code: None,
            retryable: false,
        }
    }

    /// Translate a terminal provider failure, never its partially generated output.
    pub(super) fn stream(event: &Value) -> Option<Self> {
        let kind = event["type"].as_str();
        if !matches!(
            kind,
            Some("response.failed" | "response.incomplete" | "error")
        ) && event["error"].is_null()
            && !matches!(event["status"].as_str(), Some("failed" | "incomplete"))
        {
            return None;
        }
        let response = event.get("response").unwrap_or(event);
        let reason = response["incomplete_details"]["reason"].as_str();
        let error = response.get("error").unwrap_or(event);
        let (code, retryable) = match reason.or(error["code"].as_str()) {
            Some("max_output_tokens") => ("provider_output_limit", false),
            Some("content_filter") => ("provider_content_filter", false),
            Some("context_length_exceeded") => ("provider_context_limit", false),
            Some("insufficient_quota") => ("provider_quota_exhausted", false),
            Some("invalid_prompt" | "invalid_request_error") => ("provider_invalid_request", false),
            Some("rate_limit_exceeded" | "slow_down") => ("provider_rate_limited", true),
            Some("server_error" | "server_overloaded") => ("provider_server_error", true),
            _ => ("provider_stream_failed", false),
        };
        Some(Self {
            message: format!("Provider response terminated: {code}"),
            status: if retryable { 502 } else { 400 },
            auth: false,
            downstream: Value::Null,
            budget_code: None,
            stream_code: Some(code),
            retryable,
        })
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            message,
            status: 502,
            auth: false,
            downstream: Value::Null,
            budget_code: None,
            stream_code: None,
            retryable: true,
        }
    }
}

impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

pub(super) type Outcome<T> = std::result::Result<T, Failure>;

pub(super) fn status(error: impl Into<String>, status: u16) -> Failure {
    Failure {
        status,
        ..error.into().into()
    }
}
