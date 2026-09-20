use serde::Deserialize;
use serde_json::Value;

const MAX_BUFFER_BYTES: usize = 32 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "camelCase", deny_unknown_fields)]
pub enum Command {
    Push { event: Value },
    Finish,
}

#[derive(Default)]
enum Phase {
    #[default]
    Open,
    Terminal,
    Closed,
}

/// Owns the ordered, bounded stream commit boundary, not provider authorization.
/// The caller validates tool schemas before pushing events. EOF commits the
/// buffered suffix only after a successful terminal response.
#[derive(Default)]
pub struct StreamGuard {
    phase: Phase,
    pending: Vec<Value>,
    bytes: usize,
}

impl StreamGuard {
    pub fn apply(&mut self, command: Command) -> Result<Vec<Value>, &'static str> {
        let result = match command {
            Command::Push { event } => self.push(event),
            Command::Finish => match self.phase {
                Phase::Terminal => {
                    self.phase = Phase::Closed;
                    Ok(std::mem::take(&mut self.pending))
                }
                Phase::Open => Err("Responses stream ended without completion"),
                Phase::Closed => Err("stream already closed"),
            },
        };
        if result.is_err() {
            self.phase = Phase::Closed;
            self.pending.clear();
        }
        result
    }

    fn push(&mut self, event: Value) -> Result<Vec<Value>, &'static str> {
        match self.phase {
            Phase::Open => {}
            Phase::Terminal => return Err("Responses stream emitted data after completion"),
            Phase::Closed => return Err("stream already closed"),
        }
        let kind = event["type"].as_str().ok_or("event type is required")?;
        if matches!(kind, "error" | "response.failed" | "response.incomplete") {
            return Err("Responses stream did not complete successfully");
        }
        if kind == "response.completed" {
            if event["response"]["status"] != "completed" || !event["response"]["output"].is_array()
            {
                return Err("Responses output did not complete successfully");
            }
            self.phase = Phase::Terminal;
        }
        let readable_item = matches!(
            kind,
            "response.output_item.added" | "response.output_item.done"
        ) && matches!(
            event["item"]["type"].as_str(),
            Some("message" | "reasoning")
        );
        let readable_event = matches!(
            kind,
            "response.created"
                | "response.in_progress"
                | "response.content_part.added"
                | "response.content_part.done"
                | "response.output_text.delta"
                | "response.output_text.done"
                | "response.reasoning_summary_part.added"
                | "response.reasoning_summary_part.done"
                | "response.reasoning_summary_text.delta"
                | "response.reasoning_summary_text.done"
                | "response.reasoning_text.delta"
                | "response.reasoning_text.done"
        );
        if self.pending.is_empty() && (readable_item || readable_event) {
            return Ok(vec![event]);
        }
        self.bytes += serde_json::to_vec(&event)
            .map_err(|_| "invalid event")?
            .len();
        if self.bytes > MAX_BUFFER_BYTES || self.pending.len() >= 100_000 {
            return Err("Responses stream exceeds validation buffer limit");
        }
        self.pending.push(event);
        Ok(Vec::new())
    }
}
