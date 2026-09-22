use crate::{
    Result,
    canonical::{array, string},
    config,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub(super) struct Workspace {
    directory: tempfile::TempDir,
    pub(super) nonce: String,
    pub(super) followup: String,
    pub(super) receipt: String,
    pub(super) proof: PathBuf,
    pub(super) gate: PathBuf,
    pub(super) release: PathBuf,
    pub(super) trace: PathBuf,
}
impl Workspace {
    pub(super) fn new() -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("engine-qualification-")
            .tempdir()
            .map_err(|e| e.to_string())?;
        let path = directory.path();
        Ok(Self {
            nonce: uuid::Uuid::new_v4().simple().to_string(),
            followup: uuid::Uuid::new_v4().simple().to_string(),
            receipt: uuid::Uuid::new_v4().simple().to_string(),
            proof: path.join("mcp-proof.json"),
            gate: path.join("child-gate.json"),
            release: path.join("child-release.json"),
            trace: path.join("mcp-trace.jsonl"),
            directory,
        })
    }
    pub(super) fn bridge_options(&self, input: &Value) -> Value {
        let scenario = string(&input["scenario"]);
        if scenario == "core" {
            return json!({"sessionPlan":input["sessionPlan"],"connections":input["connections"]});
        }
        let selected = array(&input["connections"]).iter().find(|c| {
            c["id"]
                == input["childModel"]
                    .as_str()
                    .map_or_else(|| input["sessionPlan"]["rootRoute"].clone(), Value::from)
        });
        let forced = scenario == "message-running"
            && selected.is_some_and(|c| c["protocolOptions"]["toolChoiceMode"] == "full");
        json!({"sessionPlan":input["sessionPlan"],"connections":input["connections"],"toolPolicy":{"rootAllowed":match scenario{"spawn-route"=>vec!["spawn_agent"],"wait"=>vec!["spawn_agent","wait_agent"],_=>vec!["spawn_agent","send_message","wait_agent"]},"childPhaseFile":if scenario=="message-running"{Some(&self.gate)}else{None},"childCompletionFile":if scenario=="message-running"{Some(&self.proof)}else{None},"childBeforePhaseAllowed":if scenario=="message-running"{vec!["engine_conformance.engine_conformance_gate","engine_conformance_gate"]}else{vec![]},"childAfterPhaseAllowed":if scenario=="message-running"{vec!["engine_conformance.engine_conformance_echo","engine_conformance_echo"]}else{vec![]},"childToolChoiceRequired":forced},"responseBarrier":if scenario=="message-running"{json!({"rootTool":"send_message","releaseFile":self.gate,"timeoutMs":input["timeoutMs"].as_u64().unwrap_or(90_000).min(30_000)})}else{Value::Null}})
    }
    pub(super) fn prepare(&self, input: &Value, address: &Value) -> Result<Value> {
        let root = self.directory.path();
        let catalog = root.join("model-catalog.json");
        let sqlite = root.join("sqlite");
        let model_catalog = config::catalog(
            &json!({"plan":input["sessionPlan"],"baseInstructions":input["baseInstructions"]}),
        )?;
        write_private(&catalog, &model_catalog.to_string())?;
        let core = input["scenario"] == "core";
        let env_keys = if core {
            vec![
                "ELECTRON_RUN_AS_NODE",
                "ENGINE_CORE_NONCE",
                "ENGINE_CORE_INSTRUCTION",
                "ENGINE_CORE_RECEIPT",
                "ENGINE_CORE_PROOF_PATH",
            ]
        } else {
            vec![
                "ELECTRON_RUN_AS_NODE",
                "ENGINE_CONFORMANCE_FOLLOWUP",
                "ENGINE_CONFORMANCE_RECEIPT",
                "ENGINE_CONFORMANCE_PROOF_PATH",
                "ENGINE_CONFORMANCE_TRACE_PATH",
                "ENGINE_CONFORMANCE_GATE_PATH",
                "ENGINE_CONFORMANCE_RELEASE_PATH",
            ]
        };
        let content = config::compile(
            &json!({"sessionPlan":input["sessionPlan"],"modelCatalogPath":catalog,"bridge":{"baseUrl":address["baseUrl"],"credentialRef":"AGENT_ENGINE_BRIDGE_TOKEN"},"mcpServers":[{"id":if core{"engine_core"}else{"engine_conformance"},"command":input["nodeCommand"],"args":[input["mcpPath"]],"envVars":env_keys,"approvalMode":"approve"}]}),
        )?;
        write_private(&root.join("config.toml"), string(&content))?;
        fs::create_dir(&sqlite).map_err(|e| e.to_string())?;
        let mut env = json!({});
        for key in [
            "PATH",
            "PATHEXT",
            "SystemRoot",
            "WINDIR",
            "ComSpec",
            "TEMP",
            "TMP",
            "TMPDIR",
            "LANG",
            "LC_ALL",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "NO_PROXY",
        ] {
            if let Some(value) = input["baseEnvironment"][key]
                .as_str()
                .filter(|s| !s.is_empty())
            {
                env[key] = value.into();
            }
        }
        env.as_object_mut().unwrap().extend(json!({"CODEX_HOME":root,"CODEX_SQLITE_HOME":sqlite,"ELECTRON_RUN_AS_NODE":"1","AGENT_ENGINE_BRIDGE_TOKEN":address["token"],"ENGINE_CONFORMANCE_FOLLOWUP":self.followup,"ENGINE_CONFORMANCE_RECEIPT":self.receipt,"ENGINE_CONFORMANCE_PROOF_PATH":self.proof,"ENGINE_CONFORMANCE_TRACE_PATH":self.trace,"ENGINE_CONFORMANCE_GATE_PATH":self.gate,"ENGINE_CONFORMANCE_RELEASE_PATH":self.release}).as_object().unwrap().clone());
        if core {
            env.as_object_mut().unwrap().extend(json!({"ENGINE_CORE_NONCE":self.nonce,"ENGINE_CORE_INSTRUCTION":format!("core-{}",self.followup),"ENGINE_CORE_RECEIPT":self.receipt,"ENGINE_CORE_PROOF_PATH":self.proof}).as_object().unwrap().clone());
        }
        Ok(
            json!({"command":input["command"],"cwd":input["cwd"],"env":env,"model":input["sessionPlan"]["defaultModel"],"turnTimeoutMs":input["timeoutMs"]}),
        )
    }
    pub(super) fn prompt(&self, input: &Value) -> String {
        let scenario = string(&input["scenario"]);
        if scenario == "core" {
            return format!(
                "Call engine_core_echo exactly once with nonce {} and instruction core-{}. Return only its exact result.",
                self.nonce, self.followup
            );
        }
        let child = if scenario == "message-running" {
            "First call engine_conformance_gate exactly once with an empty object. After that tool returns, consume the follow-up queued by the parent, extract only the value immediately after FOLLOWUP_TOKEN=, call engine_conformance_echo exactly once with that value as followup, then return only its exact result. The token is not present in this initial task. Do not call any other tool.".into()
        } else {
            format!(
                "Return exactly CHILD_TERMINAL:{} without calling a tool.",
                self.receipt
            )
        };
        let model = input["childModel"]
            .as_str()
            .map(|m| format!(" Use the exact child model override {m}."))
            .unwrap_or_default();
        match scenario {
            "spawn-route" => format!(
                "Use spawn_agent exactly once to create one child named engine_conformance with this exact instruction: {child}.{model} After spawn_agent completes, return exactly SPAWN_ACCEPTED and do not call another tool."
            ),
            "wait" => format!(
                "Use spawn_agent exactly once to create one child named engine_conformance with this exact instruction: {child}.{model} Then use wait_agent for that child exactly once. After the wait result arrives, return exactly WAIT_ACCEPTED and do not call another tool."
            ),
            _ => format!(
                "Use only spawn_agent, send_message, and wait_agent. Spawn exactly one child named engine_conformance with this exact instruction: {child}.{model} Immediately queue this exact new follow-up to the running child using send_message: FOLLOWUP_TOKEN={}. Then wait for the child. Return only the exact CONFORMANCE_RECEIPT from the child's real MCP result and end without another tool call.",
                self.followup
            ),
        }
    }
    pub(super) fn close(self) -> Result<()> {
        self.directory.close().map_err(|e| e.to_string())
    }
}
pub(super) fn read_json(path: &Path) -> Value {
    read_bounded(path)
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null)
}
pub(super) fn read_bounded(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut data = String::new();
    file.take(1_048_577).read_to_string(&mut data).ok()?;
    if data.len() > 1_048_576 {
        None
    } else {
        Some(data)
    }
}
pub(super) fn write_private(path: &Path, text: &str) -> Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())
}
