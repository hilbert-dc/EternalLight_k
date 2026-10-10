#[path = "work_control_response.rs"]
mod control_fixture;
// Loopback HTTP against production Runtime routes and freshly opened SQLite repositories.
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
        let control_uri = uri.to_owned();
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
            (status, control_fixture::work_control_response(&control_uri, status, value))
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
        self.start_with_chapters(1).await
    }
    pub async fn start_with_chapters(&self, chapters: i64) -> (String, String) {
        let created = self
            .post("/api/projects", json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"C05 港城"}))
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
            "expectedSourceDigest":workspace["sourceDigest"],"authorIntent":"林夕调查港城灯光","targetChapters":chapters,
            "maxTaskAttempts":16,"maxProviderRequests":64,"maxProviderTokens":1000000,"deadlineAt":now+600
        })).await;
        let uri = format!("{root}/generation-runs/{}", run["id"].as_str().unwrap());
        (root, uri)
    }

    pub async fn claim(&self, root: &str, action: &Value, thread: &str) -> Value {
        let thread = if action["operation"] == "advisory" {
            "coordinator"
        } else {
            thread
        };
        let workspace = self.get(&format!("{root}/creative-workspace")).await;
        let run_uri = format!(
            "{root}/generation-runs/{}",
            action["target"]["runId"].as_str().unwrap()
        );
        let run = self.get(&run_uri).await;
        let (code,task)=self.call("POST",&format!("{root}/tasks"),AUTHOR, Some(json!({
            "operation":action["operation"],"target":action["target"],"inputRevision":workspace["revision"],
            "threadId":thread,"authorIntent":author_intent(&run),
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":run["deadlineAt"]
        }))).await;
        assert!(code.is_success(), "claim action={action} error={task}");
        if task["operation"] == "advisory" {
            let id = task["id"].as_str().unwrap();
            let binding = format!(
                "threadId={}&expectedRevision={}",
                task["threadId"].as_str().unwrap(),
                task["inputRevision"]
            );
            let plan = self
                .ok(
                    "GET",
                    &format!("{root}/tasks/{id}/work-plan?{binding}"),
                    CANDIDATE,
                    None,
                )
                .await;
            if plan["version"] == 0 {
                self.candidate(&format!("{root}/tasks/{id}/work-plan"),json!({"taskId":id,"threadId":task["threadId"],"expectedRevision":task["inputRevision"],"expectedPlanVersion":0,"idempotencyKey":"chapter-delivery-plan","plan":{"intentMode":"executeGoals","goals":[{"id":"fixture-chapters","title":"交付原授权范围章节","acceptance":"按原作者要求保存并完成章节验收","kind":"deliverable","status":"inProgress","dependsOn":[],"readingQuestion":"","checkScope":"原范围章节","approach":"按真实回执选择起草、修订与验收","assessment":"","evidence":[],"sourceIds":[],"sourceChecks":[],"continuation":null}]}})).await;
            }
        }
        task
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
                "startedAt":1000.0,
                "failureObservations":[],"usageReconciliations":[]
            })
        );
    }

    pub async fn next_task(&self, root: &str, uri: &str, thread: &str) -> Value {
        let next = self.post(&format!("{uri}/next"), json!({})).await;
        assert_eq!(next["action"]["kind"], "agentTask", "{next}");
        self.claim(root, &next["action"], thread).await
    }

    pub async fn decision_status(&self, root: &str, task: &Value) -> Value {
        let mut result = self
            .ok(
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
            .await;
        assert!(result.to_string().len() <= 6500);
        let mut cursor = result["pagination"]["nextCursor"].clone();
        while let Some(next) = cursor.as_str() {
            let page = self.ok("GET", &format!("{root}/tasks/{}/work-decision?threadId={}&expectedRevision={}&cursor={next}",task["id"].as_str().unwrap(),task["threadId"].as_str().unwrap(),task["inputRevision"]),CANDIDATE,None).await;
            assert_eq!(page["version"], result["version"]);
            assert!(page.to_string().len() <= 6500);
            for field in ["requirements", "readyActions"] {
                result[field]
                    .as_array_mut()
                    .unwrap()
                    .extend(page[field].as_array().unwrap().iter().cloned());
            }
            cursor = page["pagination"]["nextCursor"].clone();
        }
        result
    }

    pub async fn choose(&self, root: &str, task: &Value, operation: &str) -> Value {
        self.choose_with_acceptance(root, task, operation, None)
            .await
    }
    pub async fn choose_with_acceptance(
        &self,
        root: &str,
        task: &Value,
        operation: &str,
        acceptance: Option<Value>,
    ) -> Value {
        let read = self.decision_status(root, task).await;
        let action = read["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["operation"] == operation)
            .unwrap_or_else(|| panic!("missing agent choice {operation}: {read}"));
        let mut request = json!({
            "taskId":task["id"],"threadId":task["threadId"],"expectedRevision":task["inputRevision"],
            "expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],
            "actionKey":action["actionKey"],"instruction":"根据作者当前要求完成当前章节，不改其他章节", "idempotencyKey":format!("select-{operation}")
        });
        if matches!(operation, "generateChapter" | "writeNextChapter") {
            request["chapterAcceptance"] = if let Some(acceptance) = acceptance {
                acceptance
            } else if read["chapterAcceptance"].is_object() {
                read["chapterAcceptance"].clone()
            } else {
                json!({"requirements":[{"id":"whole-chapter","text":"按作者当前要求完成章节"}],"length":{"requirement":"本测试作者没有指定篇幅","minCharacters":null,"maxCharacters":null}})
            };
        }
        if operation == "finishChapter" {
            let assessment = read["requirements"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["kind"] == "revisionOutcome")
                .unwrap()["assessment"]
                .clone();
            let report = self
                .get(&format!(
                    "{root}/revision-outcomes/{}",
                    assessment["writerTaskId"].as_str().unwrap()
                ))
                .await;
            request["chapterReview"] = json!({"sourceRevisionId":action["sourceRevisionId"],"assessmentRevisionId":report["revisionId"],"checks":read["chapterAcceptance"]["requirements"].as_array().unwrap().iter().map(|r|{let judgment=report["assessment"]["outcomes"].as_array().unwrap().iter().find(|o|o["id"]==r["id"]).unwrap();json!({"criterionId":r["id"],"issueId":judgment["issue"]["issueId"]})}).collect::<Vec<_>>(),"length":read["chapterAcceptance"]["length"]});
        }
        self.candidate(
            &format!(
                "{root}/tasks/{}/work-decision/actions",
                task["id"].as_str().unwrap()
            ),
            request,
        )
        .await
    }

    pub async fn source_check(&self, root: &str, task: &Value, revision: &str) -> Value {
        let pack=self.ok("GET",&format!("{root}/context?taskId={}&threadId={}&projectRevision={}&purpose={}&byteBudget=60000",task["id"].as_str().unwrap(),task["threadId"].as_str().unwrap(),task["inputRevision"],task["purpose"].as_str().unwrap()),CANDIDATE,None).await;
        let source = pack["taskSources"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["sectionPath"] == "/relevantText")
            .unwrap();
        let endpoint = format!(
            "{root}/tasks/{}/task-source-content?threadId={}&expectedRevision={}&sourceId={}",
            task["id"].as_str().unwrap(),
            task["threadId"].as_str().unwrap(),
            task["inputRevision"],
            source["sourceId"].as_str().unwrap()
        );
        let mut index_cursor = String::new();
        let selected = loop {
            let index = self
                .ok(
                    "GET",
                    &format!("{endpoint}&view=index{index_cursor}"),
                    CANDIDATE,
                    None,
                )
                .await;
            if let Some(record) = index["items"].as_array().unwrap().iter().find(|record| {
                record["identity"]["sourceRevisionId"] == revision
                    || record["identity"]["revisionId"] == revision
            }) {
                break record.clone();
            }
            index_cursor = format!(
                "&cursor={}",
                index["nextCursor"]
                    .as_str()
                    .expect("current draft must be readable")
            );
        };
        let mut cursor = selected["readCursor"].as_str().unwrap().to_owned();
        loop {
            let page = self
                .ok(
                    "GET",
                    &format!("{endpoint}&cursor={cursor}"),
                    CANDIDATE,
                    None,
                )
                .await;
            for field in page["entries"].as_array().unwrap() {
                if field["path"].as_str().is_some_and(|path| {
                    path.starts_with(selected["path"].as_str().unwrap()) && path.ends_with("/body")
                }) {
                    if let Some(body) = field["text"]
                        .as_str()
                        .or_else(|| field["value"].as_str())
                        .filter(|text| !text.trim().is_empty())
                    {
                        let quote: String = body.chars().take(24).collect();
                        return json!({"sourceId":source["sourceId"],"revisionId":revision,"contentHash":source["contentHash"],"evidence":{"quote":quote,"start":0,"end":quote.chars().count()}});
                    }
                }
            }
            cursor = page["nextCursor"]
                .as_str()
                .expect("current draft body must be readable")
                .to_owned();
        }
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
            let mut read = self.decision_status(root, task).await;
            if read["deliveryReviewRequired"] == true {
                self.read_author_requirements(root, task).await;
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
                if read["workPlan"]["activeGoalId"] == "fixture-chapters" {
                    let id = task["id"].as_str().unwrap();
                    let plan=self.ok("GET",&format!("{root}/tasks/{id}/work-plan?threadId={}&expectedRevision={}&goalId=fixture-chapters",task["threadId"].as_str().unwrap(),task["inputRevision"]),CANDIDATE,None).await;
                    let mut goal = plan["goal"].clone();
                    goal["status"] = json!("satisfied");
                    goal["assessment"] = json!("原范围完成状态与当前保存稿回执已核对");
                    goal["evidence"] = json!([evidence
                        .first()
                        .expect("final delivery must have its saved output")]);
                    self.candidate(&format!("{root}/tasks/{id}/work-plan"),json!({"taskId":id,"threadId":task["threadId"],"expectedRevision":task["inputRevision"],"expectedPlanVersion":plan["version"],"idempotencyKey":"chapter-delivery-verified","plan":{"intentMode":"executeGoals","goals":[goal]}})).await;
                    read = self.decision_status(root, task).await;
                }
                completion["deliveryReview"] = json!({"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],
                    "goalStatus":"fulfilled","goalAssessment":"作者要求的当前章节已保存", "checkedScope":"已核对保存的正文回执，不声称完成未运行的语义分析",
                    "unresolved":[],"issueResolutions":[],"authorDecisions":[],"evidence":evidence});
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
            "payload":{"chapterId":task["target"]["chapterId"],"chapterNumber":task["target"]["chapterNumber"],"title":"灯光","body":body,"completeness":"chapterComplete","sceneGroups":[]},"expectedRevision":task["inputRevision"],
            "expectedCandidateRevisionId":task["inputManifest"]["expectedCandidateRevisionId"],
            "idempotencyKey":key,"taskId":task["id"],"threadId":task["threadId"]
        })).await
    }

    pub async fn saved_chapter(&self, root: &str, uri: &str, body: &str) -> (Value, Value) {
        let writer = self.writer(root, uri, "writer").await;
        let saved = self
            .save_writer(
                root,
                &writer,
                body,
                &format!("writer-save-{}", writer["target"]["chapterNumber"]),
            )
            .await;
        self.complete_and_advance(root, uri, &writer).await;
        (writer, saved)
    }

    pub async fn assess_chapter(&self, root: &str, uri: &str, status: &str) -> Value {
        self.assess_chapter_reason(root, uri, status, "独立读取实际保存正文后给出本测试判断")
            .await
    }
    pub async fn assess_chapter_reason(
        &self,
        root: &str,
        uri: &str,
        status: &str,
        reason: &str,
    ) -> Value {
        self.assess_chapter_report(root, uri, status, reason, &[])
            .await
    }
    pub async fn read_author_requirements(&self, root: &str, task: &Value) -> Value {
        let pack=self.ok("GET",&format!("{root}/context?taskId={}&threadId={}&projectRevision={}&purpose={}&byteBudget=60000",task["id"].as_str().unwrap(),task["threadId"].as_str().unwrap(),task["inputRevision"],task["purpose"].as_str().unwrap()),CANDIDATE,None).await;
        let source = pack["taskSources"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["sectionPath"] == "/authorRequirementsSource")
            .unwrap()
            .clone();
        let mut cursor = String::new();
        loop {
            let page=self.ok("GET",&format!("{root}/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}{cursor}",task["id"].as_str().unwrap(),task["threadId"].as_str().unwrap(),task["inputRevision"],source["sourceId"].as_str().unwrap()),CANDIDATE,None).await;
            let Some(next) = page["nextCursor"].as_str() else {
                break;
            };
            cursor = format!("&cursor={next}");
        }
        source
    }
    pub async fn assess_chapter_report(
        &self,
        root: &str,
        uri: &str,
        status: &str,
        reason: &str,
        uncovered: &[&str],
    ) -> Value {
        self.assess_chapter_report_with_prior(root, uri, status, reason, uncovered, None)
            .await
    }
    pub async fn assess_chapter_report_with_prior(
        &self,
        root: &str,
        uri: &str,
        status: &str,
        reason: &str,
        uncovered: &[&str],
        prior_chapter: Option<i64>,
    ) -> Value {
        self.assess_chapter_options(
            root,
            uri,
            status,
            reason,
            uncovered,
            prior_chapter,
            true,
            true,
        )
        .await
    }
    pub async fn assess_chapter_options(
        &self,
        root: &str,
        uri: &str,
        status: &str,
        reason: &str,
        uncovered: &[&str],
        prior_chapter: Option<i64>,
        resolve: bool,
        complete: bool,
    ) -> Value {
        self.assess_chapter_transformed(
            root,
            uri,
            status,
            reason,
            uncovered,
            prior_chapter,
            resolve,
            complete,
            |_, p| p,
        )
        .await
    }
    pub async fn assess_chapter_transformed<F: Fn(&Value, Value) -> Value>(
        &self,
        root: &str,
        uri: &str,
        status: &str,
        reason: &str,
        uncovered: &[&str],
        prior_chapter: Option<i64>,
        resolve: bool,
        complete: bool,
        transform: F,
    ) -> Value {
        let decision = self.next_task(root, uri, "assessment-coordinator").await;
        self.choose(root, &decision, "assessRevisionOutcome").await;
        self.complete_and_advance(root, uri, &decision).await;
        let critic = self.next_task(root, uri, "independent-critic").await;
        assert_eq!(critic["operation"], "assessRevisionOutcome");
        let pack=self.ok("GET",&format!("{root}/context?taskId={}&threadId={}&projectRevision={}&purpose=critic&byteBudget=60000",critic["id"].as_str().unwrap(),critic["threadId"].as_str().unwrap(),critic["inputRevision"]),CANDIDATE,None).await;
        let author_source = self.read_author_requirements(root, &critic).await;
        if let Some(number) = prior_chapter {
            let directory = pack["taskSources"]["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["sectionPath"] == "/taskWorkspace")
                .unwrap();
            let url = format!(
                "{root}/tasks/{}/task-source-content?threadId={}&expectedRevision={}&sourceId={}&view=workspace&chapterNumber={number}",
                critic["id"].as_str().unwrap(),
                critic["threadId"].as_str().unwrap(),
                critic["inputRevision"],
                directory["sourceId"].as_str().unwrap()
            );
            let page = self.ok("GET", &url, CANDIDATE, None).await;
            let source = page["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["purpose"] == "priorText")
                .unwrap();
            let mut cursor = String::new();
            loop {
                let returned=self.ok("GET",&format!("{root}/tasks/{}/task-source-content?threadId={}&expectedRevision={}&sourceId={}{cursor}",critic["id"].as_str().unwrap(),critic["threadId"].as_str().unwrap(),critic["inputRevision"],source["sourceId"].as_str().unwrap()),CANDIDATE,None).await;
                let Some(next) = returned["nextCursor"].as_str() else {
                    break;
                };
                cursor = format!("&cursor={next}");
            }
        }
        let source = pack["taskSources"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["sectionPath"] == "/operationContext/revisionOutcomeInput")
            .unwrap();
        assert_eq!(
            source["sectionPath"],
            "/operationContext/revisionOutcomeInput"
        );
        let mut cursor = String::new();
        loop {
            let page=self.ok("GET",&format!("{root}/tasks/{}/task-source-content?threadId={}&expectedRevision={}&sourceId={}{cursor}",critic["id"].as_str().unwrap(),critic["threadId"].as_str().unwrap(),critic["inputRevision"],source["sourceId"].as_str().unwrap()),CANDIDATE,None).await;
            let Some(next) = page["nextCursor"].as_str() else {
                break;
            };
            cursor = format!("&cursor={next}");
        }
        let project =
            novel_codex_story_core::ProjectId::parse(root.rsplit('/').next().unwrap()).unwrap();
        let repo = novel_codex_story_core::SqliteStoryRepository::open(&self.path).unwrap();
        let task = repo
            .task(&project, critic["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        let input = repo.revision_outcome_input(&task).unwrap();
        let scene = &input["afterScenes"][0];
        let body = scene["body"].as_str().unwrap();
        let quote: String = body.chars().take(24).collect();
        let mut existing = Vec::new();
        let mut page_cursor = 0;
        loop {
            let page = repo.task_work_issues(&task, None, page_cursor, 16).unwrap();
            for issue in page["entries"].as_array().unwrap() {
                let views = repo
                    .task_work_issues(&task, issue["issueId"].as_str(), 0, 16)
                    .unwrap();
                existing.push((issue["issueId"].clone(), views["entries"].clone()));
            }
            let Some(next) = page["nextCursor"].as_u64() else {
                break;
            };
            page_cursor = next;
        }
        let judgments=input["desiredOutcomes"].as_array().unwrap().iter().enumerate().map(|(n,criterion)|{
            let old=existing.iter().find(|(_,views)|views.as_array().unwrap().iter().any(|v|v["criterionId"]==criterion["id"] || n==0 && v["criterionId"].is_null()));
            let issue=if let Some((id,views))=old {json!({"issueId":id,"newIssue":null,"resolutionOf":if resolve{views.as_array().unwrap().iter().filter(|v|v["superseded"]==false).map(|v|v["observationId"].clone()).collect::<Vec<_>>()}else{vec![]}})}else if status!="supported"{json!({"issueId":format!("problem-{}-{}",critic["target"]["chapterId"].as_str().unwrap(),criterion["id"].as_str().unwrap()),"newIssue":{"summary":reason},"resolutionOf":[]})}else{Value::Null};
            json!({"id":criterion["id"],"status":status,"reason":reason,"issue":issue,"evidence":if status=="insufficientEvidence"{json!([])}else{json!([{"groupId":scene["groupId"],"quote":quote,"start":0,"end":quote.chars().count()}])}})
        }).collect::<Vec<_>>();
        let omitted=uncovered.iter().enumerate().map(|(n,requirement)|json!({"requirement":requirement,"reason":reason,"issue":{"issueId":format!("omission-{}-{n}",critic["target"]["chapterId"].as_str().unwrap()),"newIssue":{"summary":requirement},"resolutionOf":[]},"authorSourceChecks":[{"sourceId":author_source["sourceId"],"contentHash":author_source["contentHash"],"quote":"林夕调查港城灯光"}]})).collect::<Vec<_>>();
        let payload = transform(
            &critic,
            json!({"writerTaskId":critic["target"]["writerTaskId"],"chapterId":critic["target"]["chapterId"],"originalRevisionId":critic["target"]["originalRevisionId"],"draftRevisionId":critic["target"]["draftRevisionId"],"knowledgeBatchId":critic["target"]["knowledgeBatchId"],"uncoveredRequirements":omitted,"outcomes":judgments}),
        );
        let current_report = repo
            .revision_outcome(&project, critic["target"]["writerTaskId"].as_str().unwrap())
            .unwrap()
            .map(|r| r.revision_id);
        drop(repo);
        self.candidate(&format!("{root}/revision-outcomes/{}",critic["target"]["writerTaskId"].as_str().unwrap()),json!({"taskId":critic["id"],"threadId":critic["threadId"],"expectedRevision":critic["inputRevision"],"expectedCandidateRevisionId":current_report,"idempotencyKey":format!("independent-report-{}",critic["id"].as_str().unwrap()),"payload":payload})).await;
        if complete {
            self.complete_and_advance(root, uri, &critic).await;
        }
        critic
    }
    pub async fn finish_chapter(&self, root: &str, uri: &str) -> Value {
        self.assess_chapter(root, uri, "supported").await;
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
