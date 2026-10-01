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
            "threadId":thread,"authorIntent":action["authorIntent"],
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

    pub async fn advance_until(&self, root: &str, uri: &str, stage: &str, body: &str) -> Value {
        for _ in 0..12 {
            let next = self.post(&format!("{uri}/next"), json!({})).await;
            if next["run"]["stage"] == stage {
                return next;
            }
            let action = &next["action"];
            if action["kind"] == "materializeChapter" {
                self.post(
                    &format!("{uri}/materialize"),
                    json!({"expectedVersion":next["run"]["version"]}),
                )
                .await;
                continue;
            }
            assert_eq!(action["kind"], "agentTask", "{next}");
            let operation = action["operation"].as_str().unwrap();
            let workspace = self.get(&format!("{root}/creative-workspace")).await;
            let mut action = action.clone();
            let chapter = action["target"]["chapterId"]
                .as_str()
                .unwrap_or("")
                .to_owned();
            if operation == "modelKnowledge" {
                let batch = self.post(&format!("{root}/modeling-batches"), json!({
                    "objectId":format!("draft-{chapter}"),"sourceRevisionId":workspace["chapters"][0]["candidate"]["id"],
                    "expectedRevision":workspace["revision"],"extractorVersion":"continuous-test"
                })).await;
                action["target"]["batchId"] = batch["id"].clone();
            }
            let task = self.claim(root, &action, operation).await;
            let target = &task["target"];
            let (endpoint, payload) = match operation {
                "generateBrief" => ("artifacts/brief".into(), json!({"premise":"港城灯光"})),
                "generateOutline" => (
                    "artifacts/outline".into(),
                    json!({"nodes":[{"id":"book","kind":"book","position":0,"title":"港城","purpose":"调查","summary":"灯光"}]}),
                ),
                "generatePlan" => (
                    "artifacts/plan".into(),
                    json!({"chapters":[{"chapterPlanId":"plan-1","chapterNumber":1,"goal":"看见灯光",
                    "scenes":[{"id":"scene-1","goal":"看见灯光","beats":[],"expectedEffects":[{"id":"effect-1","dimension":"knowledge","description":"林夕看见灯光","commitmentRefs":[]}]}]}]}),
                ),
                "generateChapter" => (
                    "drafts/checkpoints".into(),
                    draft(target["chapterId"].clone(), body),
                ),
                "modelKnowledge" => {
                    self.candidate(&format!("{root}/modeling-batches/{}/publish", target["batchId"].as_str().unwrap()), json!({
                        "taskId":task["id"],"threadId":operation,"expectedRevision":workspace["revision"],
                        "entities":[{"id":"linxi","kind":"character","displayName":"林夕","aliases":[],"evidence":{"quote":"林夕","start":0,"end":2}},
                            {"id":"light","kind":"event","displayName":"看见灯光","aliases":[],"evidence":{"quote":"看见灯光","start":2,"end":6}}],
                        "assertions":[{"kind":"event","subjectId":"light","predicate":"sawLight","value":{"participants":[{"role":"actor","entityId":"linxi"}],"preconditions":[],"effects":[],"untilExclusive":null},"storyTime":1,"evidence":{"quote":"看见灯光","start":2,"end":6}}]
                    })).await;
                    (String::new(), Value::Null)
                }
                "extractCommitments" => (
                    format!("chapters/{chapter}/commitment-extractions"),
                    json!({
                        "chapterId":chapter,"sourceRevisionId":target["sourceRevisionId"],"examinedRanges":[{"start":0,"end":body.chars().count()}],"candidates":[]
                    }),
                ),
                "assessSceneOutcome" => (
                    format!("chapters/{chapter}/scene-assessments"),
                    json!({
                        "chapterId":chapter,"planRevisionId":target["planRevisionId"],"draftRevisionId":target["draftRevisionId"],
                        "knowledgeBatchId":target["knowledgeBatchId"],"inputSnapshotId":target["inputSnapshotId"],"intentGeneration":target["intentGeneration"],
                        "assessments":[{"sceneId":"scene-1","verdict":"realized","rationale":"看见灯光","recommendation":"continue",
                            "effectAssessments":[{"effectId":"effect-1","verdict":"realized","rationale":"正文事件","actualEventRefs":[{"modelingBatchId":target["knowledgeBatchId"],"assertionIndex":0}],"commitmentAssessments":[]}]}]
                    }),
                ),
                other => panic!("unexpected operation {other}"),
            };
            if !endpoint.is_empty() {
                self.candidate(
                    &format!("{root}/{endpoint}"),
                    json!({
                        "payload":payload,"expectedRevision":workspace["revision"],"expectedCandidateRevisionId":task["inputManifest"]["expectedCandidateRevisionId"],
                        "idempotencyKey":format!("{operation}-{}", task["id"].as_str().unwrap()),"taskId":task["id"],"threadId":operation
                    }),
                )
                .await;
            }
            let grant = self
                .get(&format!(
                    "{root}/tasks/{}/provider-budget",
                    task["id"].as_str().unwrap()
                ))
                .await;
            self.candidate(
                &format!("{root}/tasks/{}/complete", task["id"].as_str().unwrap()),
                json!({"threadId":operation,"summary":"已保存"}),
            )
            .await;
            let charged = self.settle(uri, &task, &grant).await;
            self.post(
                &format!("{uri}/advance-task"),
                json!({"expectedVersion":charged["version"],"taskId":task["id"]}),
            )
            .await;
        }
        panic!("did not reach {stage}");
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

pub fn draft(chapter: Value, body: &str) -> Value {
    json!({"chapterId":chapter,"chapterNumber":1,"title":"灯光","body":body,"completeness":"chapterComplete",
        "sceneGroups":[{"id":"group-1","sceneId":"scene-1","body":body,"locked":false}]})
}
