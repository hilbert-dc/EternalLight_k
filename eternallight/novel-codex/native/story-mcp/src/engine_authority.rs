//! MCP transport adapter. Model arguments never supply execution identity.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

pub(super) struct Authority {
    port: u16,
    token: String,
    scope: String,
    catalog: RefCell<Option<String>>,
}
impl Authority {
    pub fn from_environment() -> Result<Self, String> {
        let url = super::required_environment("NOVEL_CODEX_ENGINE_AUTHORITY_URL")?;
        let port = url
            .strip_prefix("http://127.0.0.1:")
            .and_then(|s| s.parse::<u16>().ok())
            .filter(|v| *v > 0)
            .ok_or("Engine authority must use an explicit loopback origin")?;
        let token = super::required_environment("NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN")?;
        if token.chars().any(char::is_control) {
            return Err("Invalid Engine authority token".into());
        }
        let capability = super::required_environment("NOVEL_CODEX_TASK_CAPABILITY")?;
        Ok(Self {
            port,
            token,
            scope: format!("{:x}", Sha256::digest(capability.as_bytes())),
            catalog: RefCell::new(None),
        })
    }
    fn request(&self, route: &str, body: Value) -> Result<Value, String> {
        let body = body.to_string();
        let mut stream = TcpStream::connect(("127.0.0.1", self.port))
            .map_err(|_| "Engine authority unavailable")?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| e.to_string())?;
        write!(stream,"POST /_engine/mcp/{route} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",self.port,self.token,body.len()).map_err(|_| "Engine authority write failed")?;
        let mut bytes = Vec::new();
        stream
            .take((super::MAX_RESPONSE_BYTES + 65536) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "Engine authority read failed")?;
        let (status, value) = super::parse_http_response(&bytes)?;
        if status != 200 {
            return Err(value["error"]
                .as_str()
                .unwrap_or("Engine authority rejected invocation")
                .to_owned());
        }
        Ok(value)
    }
    fn register(&self, tools: &Value) -> Result<(), String> {
        let reply = self.request(
            "catalog",
            json!({"scope":self.scope,"namespace":"mcp__novel_workspace","tools":tools}),
        )?;
        let digest = reply["catalogDigest"]
            .as_str()
            .filter(|s| s.len() == 64)
            .ok_or("Missing Engine catalog receipt")?;
        *self.catalog.borrow_mut() = Some(digest.into());
        Ok(())
    }
    fn identity(&self, params: &Value) -> Result<Value, String> {
        let digest = self
            .catalog
            .borrow()
            .clone()
            .ok_or("MCP catalog has not been registered")?;
        self.request(
            "call",
            json!({"scope":self.scope,"namespace":"mcp__novel_workspace","catalogDigest":digest,
            "name":params["name"],"arguments":params["arguments"],"meta":params["_meta"]}),
        )
    }
}

pub(super) fn handle(
    message: &Value,
    client: &Result<super::RuntimeClient, String>,
    authority: &Result<Authority, String>,
) -> Option<Value> {
    if message["jsonrpc"] != "2.0" || message.get("id").is_none() {
        return super::handle_message(message, client);
    }
    if message["method"] == "tools/list" {
        let mut response = super::handle_message(message, client)?;
        if response["error"].is_null() {
            let result = authority
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|a| a.register(&response["result"]["tools"]));
            if let Err(error) = result {
                response = super::rpc_error(message["id"].clone(), -32603, &error);
            }
        }
        return Some(response);
    }
    if message["method"] == "tools/call" {
        let identity = authority
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|a| a.identity(&message["params"]));
        return match identity {
            Ok(meta) => {
                let mut request = message.clone();
                request["params"]["_meta"] = meta;
                super::handle_message(&request, client)
            }
            Err(error) => Some(super::rpc_result(
                message["id"].clone(),
                json!({"isError":true,"content":[{"type":"text","text":super::tool_binding::rejected("engine_authority_rejected","/_meta",&error)}]}),
            )),
        };
    }
    super::handle_message(message, client)
}
