use serde_json::{Value, json};

use crate::Result;

/// Incremental byte framing preserves split UTF-8 characters and limits the whole stream.
pub struct SseDecoder {
    buffer: Vec<u8>,
    received: usize,
    limit: usize,
    closed: bool,
    scanned: usize,
}

impl SseDecoder {
    pub fn new(limit: usize) -> Result<Self> {
        if limit == 0 || limit > 40 * 1024 * 1024 {
            return Err("invalid streaming response limit".into());
        }
        Ok(Self {
            buffer: Vec::new(),
            received: 0,
            limit,
            closed: false,
            scanned: 0,
        })
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Value>> {
        let result = self.decode(bytes);
        if result.is_err() {
            self.close();
        }
        result
    }

    fn decode(&mut self, bytes: &[u8]) -> Result<Vec<Value>> {
        if self.closed {
            return Err("SSE decoder closed".into());
        }
        if bytes.len() > self.limit.saturating_sub(self.received) {
            return Err("streaming response exceeds codec limit".into());
        }
        self.received += bytes.len();
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        let mut start = 0;
        let mut index = self.scanned;
        while index < self.buffer.len() {
            if self.buffer[index] == b'\n' {
                let next = index + 1;
                let end = if self.buffer.get(next) == Some(&b'\n') {
                    Some(next + 1)
                } else if self.buffer.get(next..next + 2) == Some(b"\r\n") {
                    Some(next + 2)
                } else {
                    None
                };
                if let Some(end) = end {
                    if let Some(event) = parse_block(&self.buffer[start..index])? {
                        events.push(event);
                    }
                    start = end;
                    index = end;
                    continue;
                }
            }
            index += 1;
        }
        self.buffer.drain(..start);
        self.scanned = self.buffer.len().saturating_sub(2);
        Ok(events)
    }

    pub fn finish(&mut self) -> Result<Vec<Value>> {
        if self.closed {
            return Err("SSE decoder closed".into());
        }
        let result = parse_block(&self.buffer).map(|event| event.into_iter().collect());
        self.close();
        result
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.buffer.clear();
    }
}

fn parse_block(bytes: &[u8]) -> Result<Option<Value>> {
    let text = std::str::from_utf8(bytes).map_err(|_| "invalid UTF-8 in provider stream")?;
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() {
        return Ok(None);
    }
    if data == "[DONE]" {
        return Ok(Some(json!({"done":true})));
    }
    let payload: Value =
        serde_json::from_str(&data).map_err(|_| "invalid JSON in provider stream")?;
    if !payload["error"].is_null() {
        return Err(payload["error"]["message"]
            .as_str()
            .unwrap_or("provider stream returned an error frame")
            .chars()
            .take(512)
            .collect());
    }
    Ok(Some(json!({"data":payload})))
}
