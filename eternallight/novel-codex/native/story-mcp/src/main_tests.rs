use serde_json::json;
use sha2::Digest;

use super::{RuntimeClient, validate_execution_identity};

#[test]
fn new_chapter_candidate_error_points_to_the_generation_goal_route_before_http() {
    let client = RuntimeClient {
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(), argument_validation: Default::default(),
        host: "127.0.0.1".into(), port: 1, candidate_token: "candidate".into(),
        capability: "capability".into(), operation: "advisory".into(),
    };
    let root = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"create"});
    let arguments = json!({"projectId":"project","taskId":"advisory","threadId":"root","expectedRevision":0,
        "expectedIntentRevision":0,"operation":"generateChapter","target":{"type":"newChapter","chapterNumber":1},
        "instruction":"生成前100章","idempotencyKey":"creation"});
    let error = client.call("request_candidate_task", &arguments, &root).unwrap_err();
    assert!(error.contains("request_generation_handoff"), "{error}");
    assert!(error.contains("target.chapterId"), "{error}");
}

#[test]
fn revision_outcome_submission_preserves_candidate_cas_and_root_recovery_identity() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let child = json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"root","sessionId":"root"});
    let root = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"submit-outcome"});
    for expected in [json!(null), json!("outcome-checkpoint")] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = RuntimeClient {
            publication_gate: super::publication_gate::PublicationGate::default(),
            read_progress: Default::default(),
            argument_validation: Default::default(),
            host: "127.0.0.1".into(),
            port: listener.local_addr().unwrap().port(),
            candidate_token: "candidate".into(),
            capability: "capability".into(),
            operation: "assessRevisionOutcome".into(),
        };
        let args = json!({"taskId":"assessment","projectId":"project","threadId":"root","expectedRevision":8,
            "writerTaskId":"writer-one","expectedCandidateRevisionId":expected,"idempotencyKey":"outcome-one",
            "payload":{"writerTaskId":"writer-one","chapterId":"chapter-one","originalRevisionId":"original",
                "draftRevisionId":"revised","knowledgeBatchId":"batch-one","outcomes":[{"id":"evidence",
                    "status":"supported","reason":"正文呈现信中依据","evidence":[{"groupId":"letter","quote":"🌙信","start":1,"end":3}]}]}});
        let names = client
            .exposed_tools()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            names,
            [
                "read_context",
                "read_creative_strategy",
                "read_task_source",
                "search_story",
                "search_relations",
                "query_story",
                "resolve_story_evidence",
                "submit_revision_outcome",
                "complete_task"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<std::collections::BTreeSet<_>>()
        );
        assert_eq!(
            client.call("submit_revision_outcome", &args, &child),
            Err("Subagents may only use read-only tools".into())
        );
        for name in [
            "save_draft_checkpoint",
            "request_candidate_task",
            "submit_scene_assessment",
            "checkpoint_reading",
            "read_reading_progress",
        ] {
            assert_eq!(
                client.call(name, &args, &root),
                Err("tool is not allowed for this generation operation".into())
            );
        }
        let mut missing_cas = args.clone();
        missing_cas
            .as_object_mut()
            .unwrap()
            .remove("expectedCandidateRevisionId");
        let missing: serde_json::Value = serde_json::from_str(
            &client
                .call("submit_revision_outcome", &missing_cas, &root)
                .unwrap_err(),
        )
        .unwrap();
        assert_eq!(missing["code"], "invalid_tool_arguments");
        assert!(
            missing["requiredFields"]
                .as_array()
                .unwrap()
                .contains(&json!("expectedCandidateRevisionId"))
        );
        let mut wrong_route = args.clone();
        wrong_route["chapterId"] = json!("chapter-one");
        let extra: serde_json::Value = serde_json::from_str(
            &client
                .call("submit_revision_outcome", &wrong_route, &root)
                .unwrap_err(),
        )
        .unwrap();
        assert_eq!(extra["code"], "invalid_tool_arguments");
        assert_eq!(extra["reason"], "additionalProperties");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            loop {
                let mut bytes = [0; 1024];
                let count = stream.read(&mut bytes).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&bytes[..count]);
                if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let header = std::str::from_utf8(&request[..end]).unwrap();
                    let length = header
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                        .unwrap()
                        .parse::<usize>()
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            stream
                .write_all(
                    b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                )
                .unwrap();
            String::from_utf8(request).unwrap()
        });
        assert_eq!(
            client
                .call("submit_revision_outcome", &args, &root)
                .unwrap(),
            (201, json!({}))
        );
        let request = server.join().unwrap();
        let (headers, body) = request.split_once("\r\n\r\n").unwrap();
        assert!(
            headers.starts_with("POST /api/projects/project/revision-outcomes/writer-one HTTP/1.1")
        );
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("x-novel-tool-name: submit_revision_outcome")
        );
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("x-novel-root-thread: root")
        );
        let mut expected = args;
        expected.as_object_mut().unwrap().remove("projectId");
        expected.as_object_mut().unwrap().remove("writerTaskId");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(body).unwrap(),
            expected
        );
    }
}

#[test]
fn model_supplied_call_ids_cannot_inject_http_headers() {
    let client = RuntimeClient {
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: 1,
        candidate_token: "candidate".into(),
        capability: "capability".into(),
        operation: "generateBrief".into(),
    };
    let args = json!({"taskId":"task","projectId":"project","threadId":"root","expectedRevision":0,"payload":{"premise":"合法简报"},"expectedCandidateRevisionId":null,"idempotencyKey":"save"});
    let meta = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"call\r\nAuthorization: forged"});
    assert_eq!(
        client.call("save_brief", &args, &meta),
        Err("execution metadata contains an invalid header value".into())
    );
}

#[test]
fn reading_progress_is_root_owned_and_cannot_be_forged_by_a_child() {
    let client = RuntimeClient {
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: 1,
        candidate_token: "token".into(),
        capability: "capability".into(),
        operation: "advisory".into(),
    };
    let args =
        json!({"taskId":"task","projectId":"project","threadId":"root","expectedRevision":0});
    let child = json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"root","sessionId":"root"});
    for name in ["read_reading_progress", "checkpoint_reading"] {
        assert!(
            client
                .exposed_tools()
                .iter()
                .any(|tool| tool["name"] == name)
        );
        assert!(validate_execution_identity(&child, "root", name).is_err());
    }
    assert_eq!(
        client.call("read_reading_progress", &args, &child),
        Err("Reading checkpoints belong to the root WorkStep".into())
    );
    let mut args = args;
    args["plan"] = json!({});
    args["reportHash"] = json!("a".repeat(64));
    args["expectedVersion"] = json!(0);
    args["summary"] = json!("child");
    assert_eq!(
        client.call("checkpoint_reading", &args, &child),
        Err("Reading checkpoints belong to the root WorkStep".into())
    );
}

#[test]
fn context_query_plan_is_encoded_as_a_read_only_task_request() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    for operation in [
        "advisory",
        "generateBrief",
        "generateOutline",
        "generatePlan",
        "reviewPlanningSeat",
        "generateChapter",
        "modelKnowledge",
        "assessSceneOutcome",
        "assessRevisionOutcome",
        "extractCommitments",
        "reviseCommitment",
        "generatePropertyRules",
    ] {
        for caller in ["root", "child"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buffer = [0; 1024];
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                    if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                        break;
                    }
                }
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"readOnly\":true}").unwrap();
                String::from_utf8(request).unwrap()
            });
            let client = RuntimeClient {
                publication_gate: super::publication_gate::PublicationGate::default(),
                read_progress: Default::default(),
                argument_validation: Default::default(),
                host: "127.0.0.1".into(),
                port,
                candidate_token: "candidate-token".into(),
                capability: "task-capability".into(),
                operation: operation.into(),
            };
            let child = json!({"rootThreadId":"root","callerThreadId":caller,"parentThreadId":if caller == "root" {json!(null)} else {json!("root")},"sessionId":"root"});
            let mut plan = json!({"question":"港城 & 白塔","byteBudget":4000,"ranking":{"type":"literal"},"steps":[{"id":"text","operation":{"type":"text","query":"港城","limit":2}}]});
            if caller == "child" {
                plan["ranking"] = json!({"type":"modelSemantic","baseReportHash":"a".repeat(64),
                    "judgments":[{"resultId":"b".repeat(64),"score":750,"rationale":"原任务允许范围内的相关性候选"}]});
            }
            let expected_purpose = match operation {
                "generateChapter" => "writer",
                "modelKnowledge"
                | "assessSceneOutcome"
                | "assessRevisionOutcome"
                | "extractCommitments"
                | "reviseCommitment" => "critic",
                _ => "planner",
            };
            let arguments = json!({"taskId":"task","projectId":"project","threadId":"root","expectedRevision":0,"budget":24,"byteBudget":60000,"queryPlan":plan});
            assert_eq!(
                client.call("read_context", &arguments, &child).unwrap(),
                (200, json!({"readOnly":true}))
            );
            let request = server.join().unwrap();
            assert!(request.starts_with("GET /api/projects/project/context?"));
            assert!(request.contains(&format!(
                "queryPlan={}",
                super::percent_encode(&plan.to_string())
            )));
            assert!(request.contains("threadId=root"));
            assert!(request.contains(&format!("purpose={expected_purpose}")));
            assert!(request.contains("Authorization: Bearer candidate-token"));
            let expected_hash = format!(
                "{:x}",
                sha2::Sha256::digest(
                    json!({"tool":"read_context","arguments":arguments})
                        .to_string()
                        .as_bytes()
                )
            );
            assert!(request.contains(&format!("X-Novel-Request-Hash: {expected_hash}")));
            let other = RuntimeClient {
                operation: "unknown".into(),
                ..client
            };
            assert_eq!(
                other.call("read_context", &arguments, &child).unwrap_err(),
                "tool is not allowed for this generation operation"
            );
        }
    }
}

#[test]
fn commitment_submission_is_root_only_and_never_granted_to_other_operations() {
    let mut client = RuntimeClient {
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: 1,
        candidate_token: "candidate".into(),
        capability: "capability".into(),
        operation: "extractCommitments".into(),
    };
    let arguments = json!({"taskId":"task","projectId":"project","threadId":"root","expectedRevision":0,
        "chapterId":"chapter","expectedCandidateRevisionId":null,"idempotencyKey":"save","payload":{}});
    let child = json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"root","sessionId":"root"});
    assert_eq!(
        client.call("submit_commitment_extraction", &arguments, &child),
        Err("Subagents may only use read-only tools".into())
    );
    let root = json!({"rootThreadId":"root","callerThreadId":"root","parentThreadId":null,"sessionId":"root"});
    for operation in ["advisory", "generateChapter", "modelKnowledge"] {
        client.operation = operation.into();
        assert_eq!(
            client.call("submit_commitment_extraction", &arguments, &root),
            Err("tool is not allowed for this generation operation".into())
        );
    }
}

#[test]
fn property_rule_submission_uses_the_artifact_route_and_blocks_children_and_other_tasks() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = RuntimeClient {
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: listener.local_addr().unwrap().port(),
        candidate_token: "candidate".into(),
        capability: "capability".into(),
        operation: "generatePropertyRules".into(),
    };
    let args = json!({"taskId":"task","projectId":"project","threadId":"root","expectedRevision":0,
        "payload":{"eventRules":[],"definitions":[]},"expectedCandidateRevisionId":null,"idempotencyKey":"rules"});
    let child = json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"root","sessionId":"root"});
    let root = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"call"});
    assert_eq!(
        client.call("save_property_rules", &args, &child),
        Err("Subagents may only use read-only tools".into())
    );
    for operation in [
        "advisory",
        "generateBrief",
        "generateOutline",
        "generatePlan",
        "generateChapter",
        "modelKnowledge",
        "assessSceneOutcome",
        "extractCommitments",
        "reviseCommitment",
        "reviewPlanningSeat",
    ] {
        client.operation = operation.into();
        assert_eq!(
            client.call("save_property_rules", &args, &root),
            Err("tool is not allowed for this generation operation".into())
        );
    }
    client.operation = "generatePropertyRules".into();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        loop {
            let mut buffer = [0; 1024];
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0);
            request.extend_from_slice(&buffer[..count]);
            if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .map(str::to_owned)
                    })
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                if request.len() >= end + 4 + length {
                    break;
                }
            }
        }
        stream
            .write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .unwrap();
        String::from_utf8(request).unwrap()
    });
    assert_eq!(
        client.call("save_property_rules", &args, &root).unwrap(),
        (201, json!({}))
    );
    let request = server.join().unwrap();
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("POST /api/projects/project/artifacts/propertyRules "));
    let mut expected = args;
    expected.as_object_mut().unwrap().remove("projectId");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(body).unwrap(),
        expected
    );
}

#[test]
fn commitment_revision_write_is_root_only_and_reads_do_not_expand_writer_permissions() {
    let mut client = RuntimeClient {
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: 1,
        candidate_token: "candidate".into(),
        capability: "capability".into(),
        operation: "reviseCommitment".into(),
    };
    let args = json!({"taskId":"task","projectId":"project","threadId":"root","expectedRevision":0,"commitmentId":"lamp","payload":{"premise":"合法简报"},"expectedCandidateRevisionId":null,"idempotencyKey":"save"});
    let child = json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"root","sessionId":"root"});
    assert_eq!(
        client.call("submit_commitment_candidate", &args, &child),
        Err("Subagents may only use read-only tools".into())
    );
    for operation in [
        "advisory",
        "generateChapter",
        "modelKnowledge",
        "extractCommitments",
    ] {
        client.operation = operation.into();
        assert_eq!(
            client.call("submit_commitment_candidate", &args, &child),
            Err("tool is not allowed for this generation operation".into())
        );
    }
    for operation in ["generateChapter", "modelKnowledge", "extractCommitments"] {
        client.operation = operation.into();
        assert_eq!(
            client.call("read_commitment", &args, &child),
            Err("tool is not allowed for this generation operation".into())
        );
    }
}

#[test]
fn commitment_input_transport_preserves_task_identity_and_percent_encodes_the_target() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    for operation in ["advisory", "reviseCommitment"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            loop {
                let mut bytes = [0; 1024];
                let count = stream.read(&mut bytes).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&bytes[..count]);
                if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    break;
                }
            }
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"readOnly\":true}").unwrap();
            String::from_utf8(request).unwrap()
        });
        let client = RuntimeClient {
            publication_gate: super::publication_gate::PublicationGate::default(),
            read_progress: Default::default(),
            argument_validation: Default::default(),
            host: "127.0.0.1".into(),
            port,
            candidate_token: "candidate-token".into(),
            capability: "bound-capability".into(),
            operation: operation.into(),
        };
        let identity = json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"root","sessionId":"root"});
        let args = json!({"taskId":"task","projectId":"project","threadId":"root","expectedRevision":7,"commitmentId":"灯 & promise"});
        assert_eq!(
            client.call("read_commitment", &args, &identity).unwrap(),
            (200, json!({"readOnly":true}))
        );
        let request = server.join().unwrap();
        assert!(request.starts_with(&format!("GET /api/projects/project/commitments/{}/task-input?taskId=task&threadId=root&expectedRevision=7",super::percent_encode("灯 & promise"))));
        assert!(request.contains("Authorization: Bearer candidate-token"));
        assert!(
            request
                .to_lowercase()
                .contains("x-novel-task-capability: bound-capability")
        );
    }
}

#[test]
fn scoped_query_plan_is_forwarded_read_only_for_a_trusted_child() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    for operation in [
        "advisory",
        "generateBrief",
        "generateOutline",
        "generatePlan",
        "reviewPlanningSeat",
        "generateChapter",
        "modelKnowledge",
        "assessSceneOutcome",
        "assessRevisionOutcome",
        "extractCommitments",
    ] {
        for caller in ["root", "child"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let (header_end, body_length) = loop {
                    let mut buffer = [0; 1024];
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                    if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                        let header = std::str::from_utf8(&request[..end]).unwrap();
                        let length = header
                            .lines()
                            .find_map(|line| line.strip_prefix("Content-Length: "))
                            .unwrap()
                            .parse::<usize>()
                            .unwrap();
                        break (end + 4, length);
                    }
                };
                while request.len() < header_end + body_length {
                    let mut buffer = [0; 1024];
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                }
                let header = std::str::from_utf8(&request[..header_end])
                    .unwrap()
                    .to_owned();
                let body: serde_json::Value =
                    serde_json::from_slice(&request[header_end..header_end + body_length]).unwrap();
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"readOnly\":true}").unwrap();
                (header, body)
            });
            let client = RuntimeClient {
                publication_gate: super::publication_gate::PublicationGate::default(),
                read_progress: Default::default(),
                argument_validation: Default::default(),
                host: "127.0.0.1".into(),
                port,
                candidate_token: "candidate-token".into(),
                capability: "task-capability".into(),
                operation: operation.into(),
            };
            let child = json!({"rootThreadId":"root","callerThreadId":caller,"parentThreadId":if caller == "root" {json!(null)} else {json!("root")},"sessionId":"root"});
            let plan = json!({"question":"港城","byteBudget":9000,"ranking":{"type":"literal"},"steps":[{"id":"passage","operation":{"type":"passages","cursor":null,"maxChars":160,"limit":2}}]});
            let arguments = json!({"taskId":"task","projectId":"project","threadId":"root","expectedRevision":0,"plan":plan});
            assert_eq!(
                client.call("query_story", &arguments, &child).unwrap(),
                (200, json!({"readOnly":true}))
            );
            let (header, body) = server.join().unwrap();
            assert!(header.starts_with("POST /api/projects/project/tasks/task/query HTTP/1.1"));
            assert_eq!(
                body,
                json!({"threadId":"root","expectedRevision":0,"plan":plan})
            );
            let other = RuntimeClient {
                operation: "unknown".into(),
                ..client
            };
            assert_eq!(
                other.call("query_story", &arguments, &child).unwrap_err(),
                "tool is not allowed for this generation operation"
            );
        }
    }
}

#[test]
fn writer_relation_search_is_read_only_and_not_exposed_to_other_operations() {
    let client = RuntimeClient {
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: 1,
        candidate_token: "token".into(),
        capability: "capability".into(),
        operation: "generateChapter".into(),
    };
    let names = client
        .exposed_tools()
        .into_iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert!(names.contains(&"search_relations".to_owned()));
    let child = json!({"rootThreadId":"root","callerThreadId":"child","parentThreadId":"root","sessionId":"root"});
    assert!(validate_execution_identity(&child, "root", "search_relations").is_ok());
    assert!(validate_execution_identity(&child, "root", "complete_task").is_err());
    let invalid = json!({"taskId":"task","projectId":"project","threadId":"root",
        "expectedRevision":0,"entityId":"linxi","maxHops":"many"});
    let error: serde_json::Value = serde_json::from_str(
        &client
            .call("search_relations", &invalid, &child)
            .unwrap_err(),
    )
    .unwrap();
    assert_eq!(error["code"], "invalid_tool_arguments");
    assert_eq!(error["reason"], "type");
    assert!(error["schemaPath"].as_str().unwrap().contains("maxHops"));
    let advisory = RuntimeClient {
        operation: "advisory".into(),
        ..client
    };
    assert!(
        !advisory
            .exposed_tools()
            .iter()
            .any(|tool| tool["name"] == "search_relations")
    );
}

#[test]
fn correction_limit_blocks_later_valid_writes_and_reads_before_runtime_dispatch() {
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let calls = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let (observed, stopped) = (calls.clone(), done.clone());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !stopped.load(Ordering::SeqCst) && Instant::now() < deadline {
            if let Ok((mut stream, _)) = listener.accept() {
                observed.fetch_add(1, Ordering::SeqCst);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                );
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    let client = RuntimeClient {
        publication_gate: Default::default(),
        read_progress: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port,
        candidate_token: "candidate".into(),
        capability: "capability".into(),
        operation: "modelKnowledge".into(),
    };
    let meta = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"call"});
    let valid = json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0,
        "batchId":"batch","entities":[],"assertions":[{"kind":"assertion","subjectId":"person",
        "predicate":"occupation","value":"clockmaker","evidence":{"quote":"沈遥是钟表修理师。"}}]});
    let mut invalid = valid.clone();
    invalid["assertions"][0]
        .as_object_mut()
        .unwrap()
        .remove("value");
    let mut results = Vec::new();
    for _ in 0..3 {
        results.push(client.call("publish_knowledge_batch", &invalid, &meta));
    }
    results.push(client.call("publish_knowledge_batch", &valid, &meta));
    results.push(client.call(
        "read_context",
        &json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0}),
        &meta,
    ));
    done.store(true, Ordering::SeqCst);
    server.join().unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "even later valid calls in a mixed batch must not reach Runtime"
    );
    for (index, result) in results.into_iter().enumerate() {
        let feedback: serde_json::Value = serde_json::from_str(&result.unwrap_err()).unwrap();
        assert_eq!(
            feedback["code"],
            if index < 2 {
                "invalid_tool_arguments"
            } else {
                "tool_argument_correction_limit"
            }
        );
    }
}

#[test]
fn all_local_schema_rejections_are_task_bound_and_never_reach_runtime() {
    use std::net::TcpListener;
    for (operation, tool, extra) in [
        (
            "generateChapter",
            "save_draft_checkpoint",
            json!({"expectedCandidateRevisionId":null,"idempotencyKey":"save","payload":{"chapterId":"chapter","chapterNumber":1,"body":"private prose","completeness":"chapterComplete"}}),
        ),
        (
            "advisory",
            "request_candidate_task",
            json!({"operation":"generateChapter","target":{},"instruction":"write","idempotencyKey":"request"}),
        ),
        (
            "advisory",
            "read_task_source",
            json!({"sourceId":"not-a-source","cursor":null}),
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let client = RuntimeClient {
            publication_gate: Default::default(),
            read_progress: Default::default(),
            argument_validation: Default::default(),
            host: "127.0.0.1".into(),
            port: listener.local_addr().unwrap().port(),
            candidate_token: "candidate".into(),
            capability: "capability".into(),
            operation: operation.into(),
        };
        let mut args =
            json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let meta = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root","turnId":"turn","callId":"call"});
        let mut foreign = meta.clone();
        foreign["sessionId"] = json!("foreign");
        for _ in 0..4 {
            assert!(
                !client
                    .call(tool, &args, &foreign)
                    .unwrap_err()
                    .contains("invalid_tool_arguments")
            );
        }
        for attempt in 1..=3 {
            let feedback: serde_json::Value =
                serde_json::from_str(&client.call(tool, &args, &meta).unwrap_err()).unwrap();
            assert_eq!(
                feedback["code"],
                if attempt == 3 {
                    "tool_argument_correction_limit"
                } else {
                    "invalid_tool_arguments"
                }
            );
            assert_eq!(feedback["tool"], tool);
            if attempt < 3 {
                assert_eq!(feedback["attempt"], attempt);
            }
            assert!(!feedback.to_string().contains("private prose"));
        }
        let stopped:serde_json::Value=serde_json::from_str(&client.call("read_context",&json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0}),&meta).unwrap_err()).unwrap();
        assert_eq!(stopped["tool"], "read_context");
        assert_eq!(stopped["originTool"], tool);
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}
