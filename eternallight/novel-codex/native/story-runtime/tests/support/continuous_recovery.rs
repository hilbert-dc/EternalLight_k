//! Loopback HTTP against production Runtime routes and freshly opened SQLite repositories.
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::http::StatusCode;
use eternallight_engine_core::PersistentBudget;
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const AUTHOR: &str = "continuous-author-1234567890123456";
pub const CANDIDATE: &str = "continuous-candidate-1234567890123456";
pub const CAPABILITY: &str = "continuous-capability-1234567890123456";
pub const BODY: &str = "林夕看见灯光。";

pub struct Runtime {
    pub _directory: tempfile::TempDir,
    pub path: PathBuf,
    address: SocketAddr,
    server: tokio::task::JoinHandle<()>,
}

impl Runtime {
    pub async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("story.sqlite3");
        let (address, server) = serve(&path).await;
        Self {
            _directory: directory,
            path,
            address,
            server,
        }
    }

    pub async fn restart(&mut self) {
        self.server.abort();
        let _ = (&mut self.server).await;
        (self.address, self.server) = serve(&self.path).await;
    }

    pub async fn call(
        &self,
        method: &str,
        uri: &str,
        actor: &str,
        payload: Option<Value>,
    ) -> (StatusCode, Value) {
        let address = self.address;
        let body = payload.map_or_else(String::new, |value| value.to_string());
        let request = format!(
            "{method} {uri} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {actor}\r\nX-Novel-Task-Capability: {CAPABILITY}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        tokio::task::spawn_blocking(move || {
            let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(10)).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            stream.write_all(request.as_bytes()).unwrap();
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).unwrap();
            let split = bytes
                .windows(4)
                .position(|part| part == b"\r\n\r\n")
                .unwrap();
            let header = std::str::from_utf8(&bytes[..split]).unwrap();
            let status =
                StatusCode::from_u16(header.split_whitespace().nth(1).unwrap().parse().unwrap())
                    .unwrap();
            let body = &bytes[split + 4..];
            let value = serde_json::from_slice(body)
                .unwrap_or_else(|_| json!(String::from_utf8_lossy(body)));
            (status, value)
        })
        .await
        .unwrap()
    }

    pub async fn ok(&self, method: &str, uri: &str, actor: &str, payload: Option<Value>) -> Value {
        let (status, value) = self.call(method, uri, actor, payload).await;
        assert!(status.is_success(), "{method} {uri}: {status} {value}");
        value
    }

    pub async fn get(&self, uri: &str) -> Value {
        self.ok("GET", uri, AUTHOR, None).await
    }

    pub async fn post(&self, uri: &str, payload: Value) -> Value {
        self.ok("POST", uri, AUTHOR, Some(payload)).await
    }

    pub async fn candidate(&self, uri: &str, payload: Value) -> Value {
        self.ok("POST", uri, CANDIDATE, Some(payload)).await
    }

    pub async fn start(&self) -> (String, String) {
        let created = self
            .post("/api/projects", json!({"title":"C05 港城"}))
            .await;
        let root = format!(
            "/api/projects/{}",
            created["project"]["id"].as_str().unwrap()
        );
        let workspace = self.get(&format!("{root}/creative-workspace")).await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let run = self.post(&format!("{root}/generation-runs"), json!({
            "expectedProjectRevision":workspace["revision"],"expectedBaseSnapshotId":workspace["headSnapshotId"],
            "expectedSourceDigest":workspace["sourceDigest"],"authorIntent":"林夕调查港城灯光","targetChapters":1,
            "maxTaskAttempts":16,"maxProviderRequests":64,"maxProviderTokens":1000000,"deadlineAt":now+600
        })).await;
        let uri = format!("{root}/generation-runs/{}", run["id"].as_str().unwrap());
        (root, uri)
    }

    pub async fn claim(&self, root: &str, action: &Value, thread: &str) -> Value {
        let workspace = self.get(&format!("{root}/creative-workspace")).await;
        let run_uri = format!(
            "{root}/generation-runs/{}",
            action["target"]["runId"].as_str().unwrap()
        );
        let run = self.get(&run_uri).await;
        self.post(&format!("{root}/tasks"), json!({
            "operation":action["operation"],"target":action["target"],"inputRevision":workspace["revision"],
            "threadId":thread,"authorIntent":author_intent(&run),
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":run["deadlineAt"]
        })).await
    }

    pub async fn settle(&self, uri: &str, task: &Value, grant: &Value) -> Value {
        self.spend(task, grant);
        let id = task["id"].as_str().unwrap();
        self.post(&format!("{uri}/provider-usage"), json!({"taskId":id}))
            .await
    }

    pub fn spend(&self, task: &Value, grant: &Value) {
        let id = task["id"].as_str().unwrap();
        let mut budget = PersistentBudget::new(&self.path.with_extension("budget.sqlite3"), id,
            json!({"options":{"id":grant["budgetId"],"maxRequests":grant["maxRequests"],"maxTokens":grant["maxTokens"],"maxDurationMs":600000,"startedAt":1000},"now":1000})).unwrap();
        let reservation = budget
            .apply(
                "budget.reserve",
                json!({"parameters":{"maxOutputTokens":8},"now":1001}),
            )
            .unwrap();
        budget
            .apply(
                "budget.settle",
                json!({"id":reservation,"usage":{"total_tokens":7}}),
            )
            .unwrap();
        budget.complete("terminal").unwrap();
        assert_eq!(
            PersistentBudget::receipt(&self.path.with_extension("budget.sqlite3"), id).unwrap(),
            json!({
                "runId":id,"budgetId":grant["budgetId"],"status":"terminal",
                "usage":{"requests":1,"tokens":7.0,"unknownTokens":0.0,"overrun":false},
                "limits":{"maxRequests":grant["maxRequests"].as_f64().unwrap(),"maxTokens":grant["maxTokens"].as_f64().unwrap(),"maxDurationMs":600000.0},
                "startedAt":1000.0
            })
        );
    }

    pub async fn next_task(&self, root: &str, uri: &str, thread: &str) -> Value {
        let next = self.post(&format!("{uri}/next"), json!({})).await;
        assert_eq!(next["action"]["kind"], "agentTask", "{next}");
        self.claim(root, &next["action"], thread).await
    }

    pub async fn decision_status(&self, root: &str, task: &Value) -> Value {
        self.ok(
            "GET",
            &format!(
                "{root}/tasks/{}/work-decision?threadId={}&expectedRevision={}",
                task["id"].as_str().unwrap(),
                task["threadId"].as_str().unwrap(),
                task["inputRevision"]
            ),
            CANDIDATE,
            None,
        )
        .await
    }

    pub async fn choose(&self, root: &str, task: &Value, operation: &str) -> Value {
        let read = self.decision_status(root, task).await;
        let action = read["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["operation"] == operation)
            .unwrap_or_else(|| panic!("missing agent choice {operation}: {read}"));
        self.candidate(&format!("{root}/tasks/{}/work-decision/actions", task["id"].as_str().unwrap()), json!({
            "taskId":task["id"],"threadId":task["threadId"],"expectedRevision":task["inputRevision"],
            "expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],
            "actionKey":action["actionKey"],"instruction":"根据作者当前要求完成当前章节，不改其他章节", "idempotencyKey":format!("select-{operation}")
        })).await
    }

    pub async fn complete_and_advance(&self, root: &str, uri: &str, task: &Value) -> Value {
        let grant = self
            .get(&format!(
                "{root}/tasks/{}/provider-budget",
                task["id"].as_str().unwrap()
            ))
            .await;
        let mut completion =
            json!({"threadId":task["threadId"],"summary":"已按保存回执完成本次选择"});
        if task["inputManifest"]["workDecision"].is_object() {
            let read = self.decision_status(root, task).await;
            if read["deliveryReviewRequired"] == true {
                let mut evidence = Vec::new();
                let mut page = read.clone();
                loop {
                    for receipt in page["receipts"]["items"].as_array().unwrap() {
                        if receipt["status"] != "completed" {
                            continue;
                        }
                        for output in receipt["outputs"].as_array().unwrap() {
                            if output["revisionId"].is_string() {
                                evidence.push(json!({"taskId":receipt["taskId"],"revisionId":output["revisionId"]}));
                            }
                        }
                    }
                    let Some(cursor) = page["pagination"]["nextCursor"].as_str() else {
                        break;
                    };
                    page = self.ok("GET", &format!("{root}/tasks/{}/work-decision?threadId={}&expectedRevision={}&cursor={}",
                        task["id"].as_str().unwrap(),task["threadId"].as_str().unwrap(),task["inputRevision"],cursor.replace(':',"%3A")),CANDIDATE,None).await;
                }
                completion["deliveryReview"] = json!({"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],
                    "goalStatus":"fulfilled","goalAssessment":"作者要求的当前章节已保存", "checkedScope":"已核对保存的正文回执，不声称完成未运行的语义分析",
                    "unresolved":[],"authorDecisions":[],"evidence":evidence});
            }
        }
        self.candidate(
            &format!("{root}/tasks/{}/complete", task["id"].as_str().unwrap()),
            completion,
        )
        .await;
        let charged = self.settle(uri, task, &grant).await;
        self.post(
            &format!("{uri}/advance-task"),
            json!({"expectedVersion":charged["version"],"taskId":task["id"]}),
        )
        .await
    }

    pub async fn writer(&self, root: &str, uri: &str, thread: &str) -> Value {
        let coordinator = self.next_task(root, uri, "coordinator").await;
        assert_eq!(coordinator["operation"], "advisory");
        let run = self.get(uri).await;
        let operation = if run["currentChapterId"].is_string() {
            "generateChapter"
        } else {
            "writeNextChapter"
        };
        self.choose(root, &coordinator, operation).await;
        self.complete_and_advance(root, uri, &coordinator).await;
        let writer = self.next_task(root, uri, thread).await;
        assert_eq!(writer["operation"], "generateChapter");
        writer
    }

    pub async fn save_writer(&self, root: &str, task: &Value, body: &str, key: &str) -> Value {
        self.candidate(&format!("{root}/drafts/checkpoints"),json!({
            "payload":draft(task["target"]["chapterId"].clone(),body),"expectedRevision":task["inputRevision"],
            "expectedCandidateRevisionId":task["inputManifest"]["expectedCandidateRevisionId"],
            "idempotencyKey":key,"taskId":task["id"],"threadId":task["threadId"]
        })).await
    }

    pub async fn saved_chapter(&self, root: &str, uri: &str, body: &str) -> (Value, Value) {
        let writer = self.writer(root, uri, "writer").await;
        let saved = self.save_writer(root, &writer, body, "writer-save").await;
        self.complete_and_advance(root, uri, &writer).await;
        (writer, saved)
    }

    pub async fn finish_chapter(&self, root: &str, uri: &str) -> Value {
        let coordinator = self.next_task(root, uri, "coordinator").await;
        self.choose(root, &coordinator, "finishChapter").await;
        self.complete_and_advance(root, uri, &coordinator).await
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn serve(path: &std::path::Path) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = router(
        RuntimeState::new(path.to_owned(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("budget.sqlite3")),
    );
    (
        address,
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        }),
    )
}

pub fn author_intent(run: &Value) -> String {
    let history: Vec<novel_codex_story_core::CreativeIntentRevision> =
        serde_json::from_value(run["intentRevisions"].clone()).unwrap();
    novel_codex_story_core::generation_author_intent(
        &history,
        run["completedChapters"].as_i64().unwrap() + 1,
    )
    .unwrap()
}

pub fn draft(chapter: Value, body: &str) -> Value {
    json!({"chapterId":chapter,"chapterNumber":1,"title":"灯光","body":body,"completeness":"chapterComplete","sceneGroups":[]})
}
