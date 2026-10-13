//! Separates the model's business arguments from trusted transport identity.
use super::RuntimeClient;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(super) const FIELDS: [&str; 3] = ["projectId", "taskId", "threadId"];

pub(super) fn rejected(reason: &str, field: &str, message: &str) -> String {
    json!({"code":"tool_binding_rejected","origin":"novelMcp","phase":"binding",
        "error":{"code":reason,"message":message,"details":{"origin":"novelMcp","phase":"binding",
        "reason":reason,"instancePath":field,"nextAction":"verify_execution_binding"}}})
    .to_string()
}

pub(super) fn model_tool(mut tool: Value) -> Value {
    if let Some(properties) = tool["inputSchema"]["properties"].as_object_mut() {
        for key in FIELDS {
            properties.remove(key);
        }
    }
    if let Some(required) = tool["inputSchema"]["required"].as_array_mut() {
        required.retain(|key| !FIELDS.iter().any(|field| key == field)
            && key != "expectedRevision" && key != "idempotencyKey"
            && key != "expectedCandidateRevisionId");
    }
    if let Some(properties) = tool["inputSchema"]["properties"].as_object_mut() {
        if let Some(revision) = properties.get_mut("expectedRevision") {
            revision["description"] = json!("Optional assertion of this task's frozen input revision. Runtime binding supplies it when omitted; never guess the current project revision.");
        }
        if let Some(candidate) = properties.get_mut("expectedCandidateRevisionId") {
            candidate["description"] = json!("Optional exact CAS assertion. Omit to bind this task's frozen candidate or its own last saved output through Runtime; unrelated writers' heads are never adopted. Explicit values remain strict assertions; absent candidate is JSON null, not a string.");
        }
        if let Some(key) = properties.get_mut("idempotencyKey") {
            key["description"] = json!("Optional explicit key for a known identical retry. The trusted tool call supplies a stable default; do not invent keys for ordinary calls.");
        }
    }
    tool
}

fn defaults(arguments: &Value, binding: &Value, meta: &Value, schema: &Value) -> Result<Value, String> {
    let mut result = arguments.clone();
    if schema["properties"].get("expectedRevision").is_some()
        && result.get("expectedRevision").is_none() {
        let revision = binding["inputRevision"].as_i64().filter(|r| *r >= 0)
            .ok_or_else(|| rejected("invalid_binding_response", "/inputRevision", "Frozen task revision is missing"))?;
        result["expectedRevision"] = json!(revision);
    }
    if schema["properties"].get("expectedCandidateRevisionId").is_some()
        && result.get("expectedCandidateRevisionId").is_none() {
        let output = &binding["output"];
        let candidate = output.get("expectedCandidateRevisionId")
            .filter(|v| v.is_null() || v.as_str().is_some_and(|s| !s.is_empty()))
            .filter(|_| output["objectId"].as_str().is_some_and(|s| !s.is_empty()))
            .ok_or_else(|| rejected("invalid_binding_response", "/output", "Task-owned candidate expectation is missing"))?;
        result["expectedCandidateRevisionId"] = candidate.clone();
    }
    if schema["properties"].get("idempotencyKey").is_some()
        && result.get("idempotencyKey").is_none() {
        let call = meta["callId"].as_str().filter(|s| !s.is_empty())
            .ok_or_else(|| rejected("invalid_execution_metadata", "/callId", "A trusted tool call identity is required"))?;
        result["idempotencyKey"] = json!(format!("call-{:x}", Sha256::digest(
            json!([binding["taskId"], meta["rootThreadId"], call]).to_string().as_bytes())));
    }
    Ok(result)
}

impl RuntimeClient {
    fn binding(&self, meta: &Value) -> Result<Value, String> {
        if self.tool_binding.borrow().is_none() {
            let (status, binding) = self.request(
                "GET",
                "/api/tool-binding",
                None,
                meta,
                "resolve_tool_binding",
                "",
            )?;
            if status != 200 {
                return Err(json!({"code":"tool_binding_rejected","origin":"novelRuntime","phase":"binding",
                    "httpStatus":status,"error":binding.get("error").cloned().unwrap_or_else(||json!({"code":"invalid_binding_response"}))}).to_string());
            }
            for field in FIELDS {
                if binding[field].as_str().is_none_or(|v| {
                    v.is_empty() || v.len() > 160 || v.chars().any(char::is_control)
                }) {
                    return Err(rejected(
                        "invalid_binding_response",
                        &format!("/{field}"),
                        "Runtime returned an invalid task binding",
                    ));
                }
            }
            if binding["operation"] != self.operation {
                return Err(rejected(
                    "operation_mismatch",
                    "/operation",
                    "Runtime operation does not match this tool process",
                ));
            }
            *self.tool_binding.borrow_mut() = Some(binding);
        }
        let binding = self.tool_binding.borrow().as_ref().unwrap().clone();
        if binding["threadId"] != meta["rootThreadId"] {
            return Err(rejected(
                "root_thread_mismatch",
                "/rootThreadId",
                "Execution root does not match the trusted task binding",
            ));
        }
        Ok(binding)
    }

    pub(super) fn call_model(
        &self,
        name: &str,
        arguments: &Value,
        meta: &Value,
    ) -> Result<(u16, Value), String> {
        let trusted_meta = super::operation_digest::bind_model(name, arguments, meta)?;
        let meta = &trusted_meta;
        self.request_dispatched.set(false);
        self.argument_validation.bind_sampling_batch(meta)?;
        self.argument_validation.ensure_active(name)?;
        let tool = self
            .operation_tools()
            .into_iter()
            .find(|t| t["name"] == name)
            .ok_or("tool is not allowed for this generation operation")?;
        // A task may save more than one checkpoint. Refresh its own CAS expectation;
        // Native still rejects foreign head changes, and model digest stays unchanged.
        if tool["inputSchema"]["properties"].get("expectedCandidateRevisionId").is_some()
            && arguments.get("expectedCandidateRevisionId").is_none() {
            *self.tool_binding.borrow_mut() = None;
        }
        let binding = if tool["inputSchema"]["properties"].get("projectId").is_some() {
            self.binding(meta)?
        } else {
            json!({"threadId":meta["rootThreadId"]})
        };
        let root = binding["threadId"].as_str().ok_or_else(|| {
            rejected(
                "root_thread_required",
                "/rootThreadId",
                "Trusted execution root is required",
            )
        })?;
        super::validate_execution_identity(meta, root, name)?;
        // Binding lookup is read-only. No business tool has been dispatched yet.
        self.request_dispatched.set(false);
        if arguments.is_object() && FIELDS.iter().all(|field| arguments.get(field).is_none()) {
            let mut receipt_args = defaults(arguments, &binding, meta, &tool["inputSchema"])?;
            // Completion is a terminal fact even if compaction lost a version.
            if let Some(bound) = self.tool_binding.borrow().as_ref() {
                for field in FIELDS {
                    receipt_args[field] = bound[field].clone();
                }
                if let Some(receipt) = self.completion_receipt.recall(name, &receipt_args, meta)? {
                    return Ok(receipt);
                }
            }
        }
        self.argument_validation.validate(
            name,
            arguments,
            &model_tool(tool.clone())["inputSchema"],
        )?;
        let mut expanded = defaults(arguments, &binding, meta, &tool["inputSchema"])?;
        for field in FIELDS {
            if tool["inputSchema"]["properties"].get(field).is_some() {
                expanded[field] = binding[field].clone();
            }
        }
        self.call(name, &expanded, meta)
    }

    pub(super) fn bound_recovery_proof(
        &self,
        name: &str,
        args: &Value,
        meta: &Value,
        result: &Result<(u16, Value), String>,
    ) -> Option<Value> {
        // Unknown or spoofed input identities must never become trusted evidence.
        if FIELDS.iter().any(|field| args.get(field).is_some()) {
            return None;
        }
        let trusted_meta = super::operation_digest::bind_model(name, args, meta).ok()?;
        let meta = &trusted_meta;
        let binding = self.tool_binding.borrow();
        let binding = binding.as_ref()?;
        let tool = self.operation_tools().into_iter().find(|t| t["name"] == name)?;
        let mut expanded = defaults(args, binding, meta, &tool["inputSchema"]).ok()?;
        if !expanded.is_object() {
            return None;
        }
        for field in FIELDS {
            expanded[field] = binding[field].clone();
        }
        self.recovery_proof(name, &expanded, meta, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn client() -> RuntimeClient {
        RuntimeClient {
            tool_binding: std::cell::RefCell::new(Some(
                json!({"projectId":"trusted-project","taskId":"trusted-task","threadId":"root","operation":"generateChapter","inputRevision":3}),
            )),
            request_dispatched: Default::default(),
            completion_receipt: Default::default(),
            argument_validation: Default::default(),
            publication_gate: Default::default(),
            read_progress: Default::default(),
            host: "127.0.0.1".into(),
            port: 1,
            candidate_token: "fixture".into(),
            capability: "fixture".into(),
            operation: "generateChapter".into(),
        }
    }
    fn meta() -> Value {
        json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"call","samplingRequestId":"sample"})
    }
    #[test]
    fn mechanical_defaults_are_stable_and_do_not_replace_explicit_cas() {
        let binding=json!({"taskId":"task","inputRevision":7});
        let schema=json!({"properties":{"expectedRevision":{},"idempotencyKey":{}}});
        let supplied=json!({"instruction":"写作"});
        let first=defaults(&supplied,&binding,&meta(),&schema).unwrap();
        assert_eq!(first["expectedRevision"],7);
        assert_eq!(first,defaults(&supplied,&binding,&meta(),&schema).unwrap());
        let mut next=meta();next["callId"]=json!("next-call");
        assert_ne!(first["idempotencyKey"],defaults(&supplied,&binding,&next,&schema).unwrap()["idempotencyKey"]);
        let explicit=json!({"expectedRevision":6,"idempotencyKey":"known-retry"});
        assert_eq!(defaults(&explicit,&binding,&meta(),&schema).unwrap(),explicit);
        let mut missing=meta();missing.as_object_mut().unwrap().remove("callId");
        assert!(defaults(&supplied,&binding,&missing,&schema).is_err());
        assert!(defaults(&supplied,&json!({"taskId":"task"}),&meta(),&schema).is_err());
        let tool=model_tool(json!({"inputSchema":{"type":"object","properties":schema["properties"],
            "required":["expectedRevision","idempotencyKey"]}}));
        assert!(jsonschema::validator_for(&tool["inputSchema"]).unwrap().is_valid(&json!({})));
    }

    #[test]
    fn candidate_default_requires_bound_output_and_preserves_explicit_assertions() {
        let schema=json!({"properties":{"expectedCandidateRevisionId":{"type":["string","null"]}}});
        for revision in [Value::Null, json!("own-output")] {
            let binding=json!({"output":{"objectId":"draft-chapter","expectedCandidateRevisionId":revision}});
            assert_eq!(defaults(&json!({}),&binding,&meta(),&schema).unwrap()["expectedCandidateRevisionId"],revision);
            for assertion in [Value::Null,json!("null"),json!("stale-explicit")] {
                let args=json!({"expectedCandidateRevisionId":assertion});
                assert_eq!(defaults(&args,&binding,&meta(),&schema).unwrap(),args);
            }
        }
        assert!(defaults(&json!({}),&json!({}),&meta(),&schema).is_err());
    }

    #[test]
    fn cached_root_mismatch_reaches_rpc_as_local_structured_error() {
        let mut meta = meta();
        meta["rootThreadId"] = "foreign".into();
        let result = super::super::handle_message(
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
            "name":"read_context","arguments":{"expectedRevision":0},"_meta":meta}}),
            &Ok(client()),
        )
        .unwrap();
        let value = &result["result"]["structuredContent"];
        assert_eq!(value["code"], "tool_binding_rejected");
        assert_eq!(value["origin"], "novelMcp");
        assert_eq!(value["error"]["details"]["reason"], "root_thread_mismatch");
        assert_eq!(value["error"]["details"]["instancePath"], "/rootThreadId");
        assert_eq!(result["result"]["isError"], true);
    }
    #[test]
    fn model_schema_excludes_fixed_identity_and_preserves_versions_and_current_guidance() {
        let client = client();
        let tools = client.exposed_tools();
        for tool in &tools {
            for field in FIELDS {
                assert!(
                    tool["inputSchema"]["properties"].get(field).is_none(),
                    "{tool}"
                );
            }
            // Guidance correctness is checked against the admitted contract below;
            // tool_guidance tests separately verify deferred detail and aggregate
            // reduction. A per-description byte cap is not a transport constraint.
            assert!(!tool["description"].as_str().unwrap().trim().is_empty());
            jsonschema::validator_for(&tool["inputSchema"]).unwrap();
        }
        assert!(tools.iter().all(|t| t["name"] != "read_task_source"));
        let save = tools
            .iter()
            .find(|t| t["name"] == "save_draft_checkpoint")
            .unwrap();
        let schema = jsonschema::validator_for(&save["inputSchema"]).unwrap();
        let patch = json!({"expectedRevision":1,"expectedCandidateRevisionId":"prior","idempotencyKey":"edit","patch":{"chapterId":"c","sourceRevisionId":"prior","edits":[{"groupId":"g","start":1,"end":2,"oldText":"风","newText":"雨"}]}});
        assert!(schema.is_valid(&patch));
        let mut ambiguous = patch.clone();
        ambiguous["payload"] = json!({});
        assert!(!schema.is_valid(&ambiguous));
        let mut no_cas = patch;
        no_cas
            .as_object_mut()
            .unwrap()
            .remove("expectedCandidateRevisionId");
        assert!(schema.is_valid(&no_cas));
        assert!(
            client
                .call_model(
                    "read_tool_guide",
                    &json!({"toolName":"save_draft_checkpoint"}),
                    &meta()
                )
                .unwrap()
                .1["instructions"]
                .as_str()
                .unwrap()
                .contains("sourceRevisionId")
        );
        let rejection = client.call_model(
            "read_tool_guide",
            &json!({"toolName":"publish_knowledge_batch"}),
            &meta(),
        ).unwrap_err();
        let rejection: Value = serde_json::from_str(&rejection).unwrap();
        assert_eq!(rejection["code"], "invalid_tool_arguments");
        assert!(!client.request_dispatched.get());
    }
    #[test]
    fn forged_model_identity_is_rejected_without_dispatch_or_binding_override() {
        for field in FIELDS {
            let client = client();
            let mut args = json!({"expectedRevision":0});
            args[field] = "foreign".into();
            let response = client
                .call_model("read_context", &args, &meta())
                .unwrap_err();
            let error: Value = serde_json::from_str(&response).unwrap();
            assert_eq!(error["code"], "invalid_tool_arguments");
            assert!(!client.request_dispatched.get());
            assert_eq!(
                client.tool_binding.borrow().as_ref().unwrap()["taskId"],
                "trusted-task"
            );
        }
        let client = client();
        let mut foreign = meta();
        foreign["rootThreadId"] = "foreign".into();
        assert!(
            client
                .call_model("read_context", &json!({"expectedRevision":0}), &foreign)
                .unwrap_err()
                .contains("trusted task binding")
        );
        assert!(!client.request_dispatched.get());
    }
    #[test]
    fn static_guidance_rejects_missing_execution_metadata_without_panicking() {
        let client = client();
        for metadata in [Value::Null, json!({}), json!({"rootThreadId":42})] {
            assert!(
                client
                    .call_model(
                        "read_tool_guide",
                        &json!({"toolName":"read_context"}),
                        &metadata
                    )
                    .is_err()
            );
            assert!(!client.request_dispatched.get());
        }
    }

    #[test]
    fn compacted_model_call_retains_acknowledged_completion_without_dispatch() {
        let client = client();
        let args = json!({"projectId":"trusted-project","taskId":"trusted-task","threadId":"root","expectedRevision":3});
        let mut response = (
            200,
            json!({"id":"trusted-task","projectId":"trusted-project","threadId":"root","status":"completed","resultSummary":"已保存"}),
        );
        client
            .completion_receipt
            .observe(&args, &mut response)
            .unwrap();
        for tool in ["read_context", "complete_task"] {
            let (status, result) = client.call_model(tool, &json!({}), &meta()).unwrap();
            assert_eq!(status, if tool == "complete_task" { 200 } else { 409 });
            if tool == "complete_task" {
                assert_eq!(result["status"], "completed");
            } else {
                assert_eq!(result["requestedOperationExecuted"], false);
                assert_eq!(result["completion"]["status"], "completed");
            }
            assert!(!client.request_dispatched.get());
        }
    }

    #[test]
    fn lazy_binding_uses_capability_and_injects_only_runtime_transport() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = client();
        client.port = listener.local_addr().unwrap().port();
        *client.tool_binding.borrow_mut() = None;
        let server = std::thread::spawn(move || {
            for index in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    let mut part = [0u8; 8192];
                    let n = stream.read(&mut part).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&part[..n]);
                }
                let request = String::from_utf8(bytes).unwrap();
                assert!(request.contains("X-Novel-Task-Capability: fixture"));
                assert!(request.contains("X-Novel-Root-Thread: root"));
                let body = if index == 0 {
                    assert!(request.starts_with("GET /api/tool-binding HTTP"));
                    json!({"projectId":"trusted-project","taskId":"trusted-task","threadId":"root","operation":"generateChapter","inputRevision":3})
                } else {
                    assert!(request.starts_with("GET /api/projects/trusted-project/context?taskId=trusted-task&threadId=root&projectRevision=3"));
                    json!({"taskSources":{"entries":[]}})
                };
                let body = body.to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            }
        });
        let args = json!({});
        assert_eq!(
            client.call_model("read_context", &args, &meta()).unwrap().0,
            200
        );
        assert_eq!(args, json!({}));
        server.join().unwrap();
    }
}
