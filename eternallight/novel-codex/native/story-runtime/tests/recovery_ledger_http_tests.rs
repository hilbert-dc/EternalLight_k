use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::*;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const AUTHOR: &str = "recovery-author-token-1234567890";
const CANDIDATE: &str = "recovery-candidate-token-1234567890";
const CAP: &str = "recovery-task-capability-1234567890";

async fn exact_operation(app: &Router, actor: &str, call: &str, hash: &str, caller: &str) -> (StatusCode, Value) {
    let uri=format!("/api/projects/recovery-http/tasks/recovery-operations?taskId=task&threadId=root&turnId=turn&callId={call}&toolName=save_brief&requestHash={hash}");
    let request=Request::builder().uri(uri)
        .header("authorization",format!("Bearer {actor}"))
        .header("x-novel-task-capability",CAP)
        .header("x-novel-tool-name","save_brief")
        .header("x-novel-root-thread","root")
        .header("x-novel-caller-thread",caller)
        .header("x-novel-turn","turn")
        .header("x-novel-tool-call",call)
        .header("x-novel-request-hash",hash)
        .body(Body::empty()).unwrap();
    let response=app.clone().oneshot(request).await.unwrap();
    let status=response.status();
    let bytes=response.into_body().collect().await.unwrap().to_bytes();
    (status,serde_json::from_slice(&bytes).unwrap())
}

async fn request(
    app: &Router,
    path: &str,
    actor: &str,
    body: Value,
    metadata: Option<(&str, &str, &str)>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {actor}"))
        .header("content-type", "application/json")
        .header("x-novel-task-capability", CAP);
    if let Some((tool, call, caller)) = metadata {
        for (name, value) in [
            ("x-novel-tool-name", tool),
            ("x-novel-tool-call", call),
            ("x-novel-root-thread", "root"),
            ("x-novel-caller-thread", caller),
            ("x-novel-turn", "turn"),
        ] {
            builder = builder.header(name, value);
        }
        builder = builder.header(
            "x-novel-request-hash",
            format!(
                "{:x}",
                Sha256::digest(json!({"tool":tool,"arguments":body}).to_string().as_bytes())
            ),
        );
    }
    let response = app
        .clone()
        .author_fixture_request(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn candidate_write_ledger_is_author_only_and_keeps_exact_proofs_after_router_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite");
    let project = ProjectId::parse("recovery-http").unwrap();
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "恢复".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    repo.create_task(CreateTaskRequest {
        task_id: "task".into(),
        project_id: project.clone(),
        thread_id: "root".into(),
        operation: "generateBrief".into(),
        input_revision: 0,
        model: None,
        target_json: "{}".into(),
        input_manifest_json: "{}".into(),
        capability_hash: format!("{:x}", Sha256::digest(CAP.as_bytes())),
        capability_expires_at: 9999999999,
    })
    .unwrap();
    repo.bind_task_turn(&project, "task", "root", "turn")
        .unwrap();
    drop(repo);
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let route = "/api/projects/recovery-http/artifacts/brief";
    let body = json!({"taskId":"task","threadId":"root","expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"save-key","payload":{"premise":"正文不是恢复元数据"}});
    let (status, _) = request(
        &app,
        route,
        CANDIDATE,
        body.clone(),
        Some(("save_brief", "child-call", "child")),
    )
    .await;
    assert!(!status.is_success());
    let (status, saved) = request(
        &app,
        route,
        CANDIDATE,
        body.clone(),
        Some(("save_brief", "save-call", "root")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let saved_hash=format!("{:x}",Sha256::digest(json!({"tool":"save_brief","arguments":body}).to_string().as_bytes()));
    let (code,exact)=exact_operation(&app,CANDIDATE,"save-call",&saved_hash,"root").await;
    assert_eq!(code,StatusCode::OK,"{exact}");
    assert_eq!(exact["operation"]["state"],"committed");
    assert_eq!(exact["operation"]["receipt_ref"],format!("runtime:candidate:{}",saved["revisionId"].as_str().unwrap()));
    let (code,missing)=exact_operation(&app,CANDIDATE,"save-call",&"f".repeat(64),"root").await;
    assert_eq!(code,StatusCode::OK,"{missing}");
    assert!(missing["operation"].is_null());
    assert_eq!(exact_operation(&app,CANDIDATE,"save-call",&saved_hash,"child").await.0,StatusCode::UNAUTHORIZED);
    let db=rusqlite::Connection::open(&path).unwrap();
    assert_eq!(db.query_row("SELECT COUNT(*) FROM story_recovery_fences_v1",[],|r|r.get::<_,i64>(0)).unwrap(),0,"exact reads must not acquire a recovery fence");
    drop(db);
    drop(app); // Drop response observer and reopen Runtime over the persisted database.
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let recovery = "/api/projects/recovery-http/tasks/recovery-snapshot";
    let query = json!({"taskIds":["task"],"limit":1});
    let (status, _) = request(&app, recovery, CANDIDATE, query.clone(), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, proof) = request(&app, recovery, AUTHOR, query.clone(), None).await;
    assert_eq!(status, StatusCode::OK, "{proof}");
    assert_eq!(proof["operations"][0]["tool_call_id"], "save-call");
    assert_eq!(
        proof["operations"][0]["receipt_ref"],
        format!(
            "runtime:candidate:{}",
            saved["revisionId"].as_str().unwrap()
        )
    );
    assert!(!proof.to_string().contains("正文不是恢复元数据"));
    let (status, replayed) = request(
        &app,
        route,
        CANDIDATE,
        body.clone(),
        Some(("save_brief", "replay-call", "root")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{replayed}");
    assert_eq!(replayed["revisionId"], saved["revisionId"]);
    let (status, page) = request(&app, recovery, AUTHOR, query.clone(), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["complete"], false);
    let (status, next) = request(
        &app,
        recovery,
        AUTHOR,
        json!({"taskIds":["task"],"fence":page["fence"],"cursor":1,"limit":1}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        next["operations"][0]["receipt_ref"],
        proof["operations"][0]["receipt_ref"]
    );
    let mut different = body.clone();
    different["payload"] = json!({"premise":"different"});
    let (status, _) = request(
        &app,
        route,
        CANDIDATE,
        different,
        Some(("save_brief", "save-call", "root")),
    )
    .await;
    assert!(!status.is_success());
    let mut failed = body.clone();
    failed["idempotencyKey"] = json!("bad-cas");
    let failed_hash=format!("{:x}",Sha256::digest(json!({"tool":"save_brief","arguments":failed}).to_string().as_bytes()));
    let (status, failure) = request(
        &app,
        route,
        CANDIDATE,
        failed.clone(),
        Some(("save_brief", "failed-call", "root")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{failure}");
    assert_eq!(failure["error"]["details"]["reason"], "candidate_expectation_mismatch");
    assert_eq!(failure["error"]["details"]["instancePath"], "/expectedCandidateRevisionId");
    assert_eq!(failure["error"]["details"]["outputObjectId"], "artifact-brief");
    assert_eq!(failure["error"]["details"]["expectedCandidateRevisionId"], saved["revisionId"]);
    assert!(!failure.to_string().contains("正文不是恢复元数据"));
    let (status, rejected) = exact_operation(&app, CANDIDATE, "failed-call", &failed_hash, "root").await;
    assert_eq!(status, StatusCode::OK, "{rejected}");
    assert_eq!(rejected["operation"]["state"], "rejected");
    assert!(rejected["operation"]["receipt_ref"].is_null());
    let (status, latest) = request(&app, recovery, AUTHOR, json!({"taskIds":["task"]}), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["operations"][2]["state"], "rejected");
    assert_eq!(latest["operations"][2]["receipt_ref"], Value::Null);
    assert_eq!(latest["operations"].as_array().unwrap().len(), 3);

    // This generic candidate writer can correct CAS in the same task; the
    // report-specific contract is not the authority for candidate saves.
    let mut corrected = failed;
    corrected["expectedCandidateRevisionId"] = saved["revisionId"].clone();
    corrected["idempotencyKey"] = json!("correct-brief-cas");
    let (status, revised) = request(&app, route, CANDIDATE, corrected.clone(), Some(("save_brief", "correct-brief-call", "root"))).await;
    assert_eq!(status, StatusCode::CREATED, "{revised}");
    let corrected_hash=format!("{:x}",Sha256::digest(json!({"tool":"save_brief","arguments":corrected}).to_string().as_bytes()));
    let (status, corrected_proof) = exact_operation(&app, CANDIDATE, "correct-brief-call", &corrected_hash, "root").await;
    assert_eq!(status, StatusCode::OK, "{corrected_proof}");
    assert_eq!(corrected_proof["operation"]["state"], "committed");
    assert_eq!(corrected_proof["operation"]["receipt_ref"], format!("runtime:candidate:{}", revised["revisionId"].as_str().unwrap()));
    assert_eq!(exact_operation(&app, CANDIDATE, "failed-call", &failed_hash, "root").await.1, rejected);

    // Preserve the unknown result contract using a real persistence failure,
    // rather than a definite, constructor-owned expected-head rejection.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_brief_save BEFORE INSERT ON story_task_outputs_v2 WHEN NEW.output_kind='brief' BEGIN SELECT RAISE(ABORT,'synthetic brief persistence failure'); END;").unwrap();
    let before: (i64, i64, i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_revisions_v2),(SELECT COUNT(*) FROM story_task_outputs_v2),(SELECT COUNT(*) FROM story_object_heads_v2)", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    drop(db);
    let mut uncertain = corrected;
    uncertain["expectedCandidateRevisionId"] = revised["revisionId"].clone();
    uncertain["idempotencyKey"] = json!("uncertain-brief-save");
    uncertain["payload"] = json!({"premise":"另一份未获保存回执的候选"});
    let uncertain_hash=format!("{:x}",Sha256::digest(json!({"tool":"save_brief","arguments":uncertain}).to_string().as_bytes()));
    let (status, error) = request(&app, route, CANDIDATE, uncertain, Some(("save_brief", "uncertain-call", "root"))).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{error}");
    let db = rusqlite::Connection::open(&path).unwrap();
    let after: (i64, i64, i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_revisions_v2),(SELECT COUNT(*) FROM story_task_outputs_v2),(SELECT COUNT(*) FROM story_object_heads_v2)", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(after, before);
    let current: String = db.query_row("SELECT candidate_revision_id FROM story_object_heads_v2 WHERE project_id=?1 AND object_id='artifact-brief'", [project.as_str()], |r| r.get(0)).unwrap();
    assert_eq!(current, revised["revisionId"].as_str().unwrap());
    drop(db);
    let (status, unknown) = exact_operation(&app, CANDIDATE, "uncertain-call", &uncertain_hash, "root").await;
    assert_eq!(status, StatusCode::OK, "{unknown}");
    assert_eq!(unknown["operation"]["state"], "accepted");
    assert!(unknown["operation"]["receipt_ref"].is_null());
    rusqlite::Connection::open(&path).unwrap().execute_batch("DROP TRIGGER fail_brief_save;").unwrap();
    SqliteStoryRepository::open(&path).unwrap().update_task(&project,"task","root",TaskUpdate {
        status:"cancelled".into(),error:Some("author_paused".into()),result_summary:None,now:1,
    }).unwrap();
    // Revoked write authority cannot hide its own terminal receipt or turn an
    // accepted unknown write into a committed/rejected outcome.
    let (code,terminal)=exact_operation(&app,CANDIDATE,"save-call",&saved_hash,"root").await;
    assert_eq!(code,StatusCode::OK,"{terminal}");
    assert_eq!(terminal["operation"]["receipt_ref"],exact["operation"]["receipt_ref"]);
    let (code,unknown)=exact_operation(&app,CANDIDATE,"uncertain-call",&uncertain_hash,"root").await;
    assert_eq!(code,StatusCode::OK,"{unknown}");
    assert_eq!(unknown["operation"]["state"],"accepted");
    assert!(unknown["operation"]["receipt_ref"].is_null());
    assert!(!unknown.to_string().contains("正文不是恢复元数据"));
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;

#[path = "../../../shared/novel-operation-digest.rs"]
mod operation_digest;

const RECONCILE: &str = "/api/projects/recovery-http/tasks/recovery-operations/reconcile";

/// Actual Native admission followed by process interruption, rather than
/// changing an already rejected ledger row back to accepted.
struct ReconciliationFixture {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
    app: Router,
    project: ProjectId,
}

impl ReconciliationFixture {
    async fn new(with_candidate: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reconcile.sqlite3");
        let project = ProjectId::parse("recovery-http").unwrap();
        SqliteStoryRepository::open(&path).unwrap().create_project(ProjectRecord {
            project_id: project.clone(), title: "核定".into(), logline: String::new(), created_at: 1, updated_at: 1,
        }, BranchId::parse("main").unwrap(), SnapshotId::parse("initial").unwrap()).unwrap();
        let app = router(task_fixture::state(path.clone(), AUTHOR.into(), CANDIDATE.into()));
        if with_candidate {
            let (status, saved) = request(&app, "/api/projects/recovery-http/artifacts/brief", AUTHOR,
                json!({"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"seed-brief",
                    "payload":{"premise":"真实非空候选来源"}}), None).await;
            assert_eq!(status, StatusCode::CREATED, "{saved}");
        }
        let mut repo = SqliteStoryRepository::open(&path).unwrap();
        repo.create_task(CreateTaskRequest {
            task_id:"task".into(), project_id:project.clone(), thread_id:"root".into(), operation:"generateBrief".into(),
            input_revision:0, model:None, target_json:"{}".into(), input_manifest_json:"{}".into(),
            capability_hash:format!("{:x}", Sha256::digest(CAP.as_bytes())), capability_expires_at:9999999999,
        }).unwrap();
        repo.bind_task_turn(&project, "task", "root", "turn").unwrap();
        Self { _dir:dir, path, app, project }
    }

    fn db(&self) -> rusqlite::Connection { rusqlite::Connection::open(&self.path).unwrap() }
    fn repo(&self) -> SqliteStoryRepository { SqliteStoryRepository::open(&self.path).unwrap() }
    fn frozen(&self) -> Value {
        serde_json::from_str::<Value>(&self.repo().task(&self.project, "task").unwrap().unwrap().input_manifest_json)
            .unwrap()["expectedCandidateRevisionId"].clone()
    }
    fn arguments(&self, candidate: Value, key: &str) -> Value {
        json!({"expectedRevision":0,
            "expectedCandidateRevisionId":candidate,"idempotencyKey":key,"payload":{"premise":"PRIVATE_ORIGINAL_CONTENT"}})
    }
    fn evidence(&self, arguments: Value, call: &str) -> Value {
        json!({"taskId":"task","threadId":"root","turnId":"turn","callId":call,"toolName":"save_brief",
            "requestHash":operation_digest::operation_digest("save_brief", &arguments).unwrap(),"arguments":arguments})
    }
    fn admit(&self, arguments: Value, call: &str) -> Value {
        let evidence = self.evidence(arguments, call);
        self.repo().bind_recovery_operation(RecoveryOperationBinding {
            project_id:self.project.clone(),task_id:"task".into(),thread_id:"root".into(),turn_id:"turn".into(),
            call_id:call.into(),tool_name:"save_brief".into(),request_hash:evidence["requestHash"].as_str().unwrap().into(),
            idempotency_ref:Some(format!("sha256:{:x}", Sha256::digest(evidence["arguments"]["idempotencyKey"].to_string().as_bytes()))),now:1,
        }).unwrap();
        evidence
    }
    fn transition(&self, status: &str) {
        self.repo().update_task(&self.project, "task", "root", TaskUpdate {
            status:status.into(), error:None, result_summary:None, now:2,
        }).unwrap();
    }
    async fn post(&self, actor: &str, evidence: Value) -> (StatusCode, Value) {
        let response = self.app.clone().oneshot(Request::builder().method("POST").uri(RECONCILE)
            .header("authorization", format!("Bearer {actor}")).header("content-type", "application/json")
            .body(Body::from(evidence.to_string())).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes))))
    }
    async fn candidate_save(&self, arguments: &Value, call: &str) -> (StatusCode, Value) {
        let mut body = arguments.clone();
        body["taskId"]="task".into(); body["threadId"]="root".into();
        let response = self.app.clone().oneshot(Request::builder().method("POST")
            .uri("/api/projects/recovery-http/artifacts/brief")
            .header("authorization", format!("Bearer {CANDIDATE}")).header("content-type", "application/json")
            .header("x-novel-task-capability", CAP).header("x-novel-tool-name", "save_brief")
            .header("x-novel-root-thread", "root").header("x-novel-caller-thread", "root")
            .header("x-novel-turn", "turn").header("x-novel-tool-call", call)
            .header("x-novel-request-hash", operation_digest::operation_digest("save_brief", arguments).unwrap())
            .body(Body::from(body.to_string())).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    fn domain_state(&self) -> Value {
        let db = self.db();
        let task: (String,String,String,String,Option<String>,Option<i64>) = db.query_row(
            "SELECT status,stage,delivery_status,output_refs_json,error,capability_revoked_at FROM story_tasks_v2 WHERE task_id='task'",
            [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap();
        let counts = ["story_revisions_v2","story_task_outputs_v2","story_candidate_receipts_v2","story_receipts_v2",
            "story_snapshots_v2","story_commits_v2","story_tasks_v2","story_work_steps_v1","story_work_attempts_v1"]
            .map(|table| db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_,i64>(0)).unwrap());
        let mut works = db.prepare("SELECT id,status,version FROM story_work_tasks_v1 ORDER BY id").unwrap();
        let works = works.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?)))
            .unwrap().collect::<Result<Vec<_>,_>>().unwrap();
        let mut budgets = db.prepare("SELECT work_task_id,ledger_json FROM story_work_budgets_v1 ORDER BY work_task_id").unwrap();
        let budgets = budgets.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))
            .unwrap().collect::<Result<Vec<_>,_>>().unwrap();
        let heads = self.repo().object_heads(&self.project).unwrap().into_iter().map(|h| json!({
            "id":h.object_id.as_str(),"candidate":h.candidate_revision_id.as_ref().map(|r|r.as_str()),
            "accepted":h.accepted_revision_id.as_ref().map(|r|r.as_str()),"locked":h.locked})).collect::<Vec<_>>();
        json!({"task":task,"counts":counts,"works":works,"budgets":budgets,"heads":heads})
    }
}

#[tokio::test]
async fn terminal_brief_reconciliation_uses_nonempty_frozen_cas_and_canonical_numeric_arguments() {
    let f = ReconciliationFixture::new(true).await;
    assert!(f.frozen().is_string());
    let mut arguments = f.arguments(Value::Null, "original-null-cas");
    arguments["expectedRevision"] = json!(0.0);
    let evidence = f.admit(arguments.clone(), "interrupted-call");
    let mut integer = arguments;
    integer["expectedRevision"] = json!(0);
    assert_eq!(f.evidence(integer, "interrupted-call")["requestHash"], evidence["requestHash"]);
    f.transition("cancelled");
    let before = f.domain_state();
    let (status, proof) = f.post(AUTHOR, evidence.clone()).await;
    assert_eq!(status, StatusCode::OK, "{proof}");
    assert_eq!(proof["operation"]["state"], "rejected");
    assert!(proof["operation"]["receipt_ref"].is_null());
    assert!(!proof.to_string().contains("PRIVATE_ORIGINAL_CONTENT"));
    assert_eq!(f.post(AUTHOR, evidence).await, (StatusCode::OK, proof));
    assert_eq!(f.domain_state(), before);
}

#[tokio::test]
async fn reconciliation_keeps_active_unsealed_contradictory_or_receipted_calls_unknown() {
    for edge in ["running","waiting","unsealed","completed","refs","candidate-receipt","commit-receipt"] {
        let f = ReconciliationFixture::new(false).await;
        let evidence = f.admit(f.arguments(json!("wrong-head"), "interrupted-key"), "interrupted-call");
        if matches!(edge, "running" | "waiting") { f.transition(edge); } else { f.transition("failed"); }
        match edge {
            "unsealed" => { f.db().execute("UPDATE story_tasks_v2 SET capability_revoked_at=NULL WHERE task_id='task'", []).unwrap(); }
            // Explicit current-schema contradiction fixtures: neither can be
            // promoted to a rejection, and neither authorizes execution.
            "completed" => { f.db().execute("UPDATE story_tasks_v2 SET status='completed' WHERE task_id='task'", []).unwrap(); }
            "refs" => { f.db().execute("UPDATE story_tasks_v2 SET output_refs_json='[\"retained-output-ref\"]' WHERE task_id='task'", []).unwrap(); }
            "candidate-receipt" => { f.db().execute("INSERT INTO story_candidate_receipts_v2 VALUES ('recovery-http','another-branch','interrupted-key','domain-digest','retained-revision','retained-snapshot',0)", []).unwrap(); }
            "commit-receipt" => { f.db().execute("INSERT INTO story_receipts_v2 VALUES ('recovery-http','another-branch','interrupted-key','domain-digest','retained-commit','retained-snapshot','applied')", []).unwrap(); }
            _ => {}
        }
        let before = f.domain_state();
        let original = exact_operation(&f.app, CANDIDATE, "interrupted-call", evidence["requestHash"].as_str().unwrap(), "root").await.1;
        assert_eq!(original["operation"]["state"], "accepted");
        assert_eq!(f.post(AUTHOR, evidence).await, (StatusCode::OK, original), "{edge}");
        assert_eq!(f.domain_state(), before, "{edge}");
    }
}

#[tokio::test]
async fn reconciliation_preserves_real_storage_unknown_outputs_and_committed_receipts() {
    let f = ReconciliationFixture::new(false).await;
    let arguments = f.arguments(f.frozen(), "storage-fault");
    let evidence = f.evidence(arguments.clone(), "storage-call");
    f.db().execute_batch("CREATE TRIGGER fail_reconciliation_save BEFORE INSERT ON story_task_outputs_v2 WHEN NEW.output_kind='brief' BEGIN SELECT RAISE(ABORT,'real persistence fault'); END;").unwrap();
    let (status, error) = f.candidate_save(&arguments, "storage-call").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{error}");
    f.db().execute_batch("DROP TRIGGER fail_reconciliation_save;").unwrap();
    f.transition("failed");
    let before = f.domain_state();
    let (status, unknown) = f.post(AUTHOR, evidence).await;
    assert_eq!(status, StatusCode::OK, "{unknown}");
    assert_eq!(unknown["operation"]["state"], "accepted");
    assert!(unknown["operation"]["receipt_ref"].is_null());
    assert_eq!(f.domain_state(), before);

    let f = ReconciliationFixture::new(false).await;
    let saved_arguments = f.arguments(f.frozen(), "saved-key");
    let (status, saved) = f.candidate_save(&saved_arguments, "saved-call").await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let saved_evidence = f.evidence(saved_arguments, "saved-call");
    let original = exact_operation(&f.app, CANDIDATE, "saved-call", saved_evidence["requestHash"].as_str().unwrap(), "root").await.1;
    assert_eq!(original["operation"]["state"], "committed");
    // A later Author head and sealed task cannot replace the original receipt.
    let (status, advanced) = request(&f.app, "/api/projects/recovery-http/artifacts/brief", AUTHOR,
        json!({"expectedRevision":0,"expectedCandidateRevisionId":saved["revisionId"],"idempotencyKey":"later-author-head",
            "payload":{"premise":"作者后来保存的候选"}}), None).await;
    assert_eq!(status, StatusCode::CREATED, "{advanced}");
    let interrupted = f.admit(f.arguments(json!("wrong-original-head"), "after-output-key"), "after-output-call");
    f.transition("failed");
    let before = f.domain_state();
    assert_eq!(f.post(AUTHOR, saved_evidence).await, (StatusCode::OK, original));
    let (status, proof) = f.post(AUTHOR, interrupted).await;
    assert_eq!(status, StatusCode::OK, "{proof}");
    assert_eq!(proof["operation"]["state"], "accepted", "Own output prevents retrospective CAS inference");
    assert_eq!(f.domain_state(), before);
}

#[tokio::test]
async fn reconciliation_rejects_inexact_original_arguments_without_leaking_content_or_changing_facts() {
    let f = ReconciliationFixture::new(false).await;
    let evidence = f.admit(f.arguments(json!("wrong-head"), "original-key"), "original-call");
    f.transition("failed");
    let before = f.domain_state();
    let mut variants = Vec::new();
    for (field, value) in [("projectId",json!("another-project")),("taskId",json!("another-task")),
        ("threadId",json!("another-thread")),("expectedRevision",json!("0")),("expectedRevision",json!(false)),
        ("expectedRevision",json!(9_007_199_254_740_992_u64)),("expectedCandidateRevisionId",json!(false)),
        ("idempotencyKey",json!("changed-key"))] {
        let mut altered = evidence.clone(); altered["arguments"][field] = value;
        variants.push(altered);
    }
    let mut missing = evidence.clone(); missing["arguments"].as_object_mut().unwrap().remove("expectedCandidateRevisionId");
    variants.push(missing);
    for (field, value) in [("taskId",json!("another-task")),("threadId",json!("another-thread")),
        ("toolName",json!("submit_revision_outcome")),("callId",json!("another-call")),("requestHash",json!("f".repeat(64)))] {
        let mut altered = evidence.clone(); altered[field] = value; variants.push(altered);
    }
    let mut oversized = evidence.clone(); oversized["arguments"]["payload"] = json!({"premise":"x".repeat(129*1024)});
    variants.push(oversized);
    let mut supplied_outcome = evidence.clone(); supplied_outcome["outcome"] = json!("rejected");
    variants.push(supplied_outcome);
    for altered in variants {
        let (status, response) = f.post(AUTHOR, altered).await;
        assert!(!status.is_success() || response["operation"].is_null(), "{response}");
        assert!(!response.to_string().contains("PRIVATE_ORIGINAL_CONTENT"));
        assert_eq!(f.domain_state(), before);
        assert_eq!(exact_operation(&f.app, CANDIDATE, "original-call", evidence["requestHash"].as_str().unwrap(), "root").await.1["operation"]["state"], "accepted");
    }
    // Equal canonical hash does not make malformed explicit values
    // admissible; omitted CAS is a valid default assertion. Use a freshly admitted original call to test shape directly.
    for (index, version) in [json!("0"),json!(false)].into_iter().enumerate() {
        let f = ReconciliationFixture::new(false).await;
        let mut arguments = f.arguments(json!("wrong-head"), "malformed-version"); arguments["expectedRevision"] = version;
        let evidence = f.admit(arguments, &format!("malformed-call-{index}"));
        f.transition("failed");
        assert_eq!(f.post(AUTHOR, evidence).await.0, StatusCode::BAD_REQUEST);
    }
    for missing in [false, true] {
        let f = ReconciliationFixture::new(false).await;
        let mut arguments = f.arguments(json!(false), "malformed-cas");
        if missing { arguments.as_object_mut().unwrap().remove("expectedCandidateRevisionId"); }
        let evidence = f.admit(arguments, "malformed-cas-call");
        f.transition("failed");
        let before = f.domain_state();
        assert_eq!(f.post(AUTHOR, evidence.clone()).await.0, if missing { StatusCode::OK } else { StatusCode::BAD_REQUEST });
        assert_eq!(exact_operation(&f.app, CANDIDATE, "malformed-cas-call", evidence["requestHash"].as_str().unwrap(), "root").await.1["operation"]["state"], "accepted");
        assert_eq!(f.domain_state(), before);
    }
}

#[tokio::test]
async fn reconciliation_keeps_missing_frozen_cas_or_invalid_output_refs_unresolved() {
    for edge in ["missing-cas","null-refs","malformed-refs"] {
        let f = ReconciliationFixture::new(false).await;
        let evidence = f.admit(f.arguments(json!("wrong-head"), "corrupt-key"), "corrupt-call");
        f.transition("failed");
        match edge {
            "missing-cas" => { f.db().execute("UPDATE story_tasks_v2 SET input_manifest_json=json_remove(input_manifest_json,'$.expectedCandidateRevisionId') WHERE task_id='task'", []).unwrap(); }
            "null-refs" => { f.db().execute("UPDATE story_tasks_v2 SET output_refs_json='null' WHERE task_id='task'", []).unwrap(); }
            _ => { f.db().execute("UPDATE story_tasks_v2 SET output_refs_json='{}' WHERE task_id='task'", []).unwrap(); }
        }
        let before = f.domain_state();
        let (status, _) = f.post(AUTHOR, evidence.clone()).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{edge}");
        assert_eq!(exact_operation(&f.app, CANDIDATE, "corrupt-call", evidence["requestHash"].as_str().unwrap(), "root").await.1["operation"]["state"], "accepted");
        assert_eq!(f.domain_state(), before);
    }
}

#[tokio::test]
async fn rejected_candidate_proposal_is_known_zero_but_storage_failure_remains_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("proposal-recovery.sqlite3");
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (status, created) = request(
        &app,
        "/api/projects",
        AUTHOR,
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"提议拒绝恢复"}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project}");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let (status,task) = request(&app,&format!("{base}/tasks"),AUTHOR,json!({
        "operation":"advisory","inputRevision":0,"threadId":"root","target":{"type":"project"},
        "authorIntent":"为小说拟一份创作简报","capabilityHash":format!("{:x}",Sha256::digest(CAP.as_bytes())),
        "capabilityExpiresAt":expires}),None).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    SqliteStoryRepository::open(&path)
        .unwrap()
        .bind_task_turn(&ProjectId::parse(project).unwrap(), task_id, "root", "turn")
        .unwrap();
    let route = format!("{base}/tasks/{task_id}/actions");
    let mut body = json!({"taskId":task_id,"threadId":"root","expectedRevision":0,"expectedIntentRevision":99,
        "idempotencyKey":"stale-proposal","operation":"generateBrief","target":{},"instruction":"只调整简报"});
    let (status, rejected) = request(
        &app,
        &route,
        CANDIDATE,
        body.clone(),
        Some(("request_candidate_task", "rejected-call", "root")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    assert!(
        rejected["error"]["message"]
            .as_str()
            .unwrap()
            .contains("authorIntentState.expectedIntentRevision"),
        "{rejected}"
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT state FROM story_tool_operations_v1 WHERE call_id='rejected-call'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "rejected"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_task_action_requests_v1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_task_outputs_v2", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    // Correct a definite rejection before any unknown operation exists.
    body["expectedIntentRevision"] = json!(0);
    body["idempotencyKey"] = json!("corrected-proposal");
    let (status, saved) = request(
        &app,
        &route,
        CANDIDATE,
        body.clone(),
        Some(("request_candidate_task", "corrected-call", "root")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    // A later proposal suffers a storage failure. Do not retry it or submit a
    // replacement after this point: only read its receipt and verify the fence.
    db.execute_batch("CREATE TRIGGER fail_action BEFORE INSERT ON story_task_action_requests_v1 BEGIN SELECT RAISE(ABORT,'simulated storage fault'); END;").unwrap();
    body["idempotencyKey"] = json!("unknown-proposal");
    let (status, failed) = request(
        &app,
        &route,
        CANDIDATE,
        body,
        Some(("request_candidate_task", "unknown-call", "root")),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{failed}");
    assert_eq!(
        db.query_row(
            "SELECT state FROM story_tool_operations_v1 WHERE call_id='unknown-call'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "accepted"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_task_action_requests_v1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    db.execute_batch("DROP TRIGGER fail_action;").unwrap();
    let (status, snapshot) = request(
        &app,
        &format!("{base}/tasks/recovery-snapshot"),
        AUTHOR,
        json!({"taskIds":[task_id]}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{snapshot}");
    let operations = snapshot["operations"].as_array().unwrap();
    let by_call = |call: &str| {
        operations
            .iter()
            .find(|o| o["tool_call_id"] == call)
            .unwrap()
    };
    assert_eq!(by_call("rejected-call")["state"], "rejected");
    assert_eq!(by_call("rejected-call")["receipt_ref"], Value::Null);
    assert_eq!(by_call("unknown-call")["state"], "accepted");
    assert_eq!(by_call("unknown-call")["receipt_ref"], Value::Null);
    assert_eq!(by_call("corrected-call")["state"], "committed");
    assert_eq!(
        by_call("corrected-call")["receipt_ref"],
        format!("runtime:action:{}", saved["id"].as_str().unwrap())
    );
    // End the model turn and settle its zero-request provider receipt, so the
    // remaining blocker is the unknown tool write rather than an active worker.
    let (status, completed) = request(
        &app,
        &format!("{base}/tasks/{task_id}/complete"),
        CANDIDATE,
        json!({"threadId":"root","summary":"提议保存结果尚未确认，停止执行并等待核对"}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    settle_fixture::settle(&path, project, task_id);
    let repo = SqliteStoryRepository::open(&path).unwrap();
    let project_id = ProjectId::parse(project).unwrap();
    let work = repo.work_task_for_attempt(&project_id, task_id).unwrap();
    let watermarks = repo.project_source_watermarks(&project_id).unwrap();
    let (status, decision) = request(&app,&format!("{base}/work-tasks/{}/decision",work.id),AUTHOR,
        json!({"expectedWorkVersion":work.version,"expectedRevision":0,"expectedSourceDigest":watermarks["digest"]}),None).await;
    assert_eq!(status, StatusCode::OK, "{decision}");
    assert_eq!(decision["stopReason"], "pendingRecovery");
    assert_eq!(decision["step"], Value::Null);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_tasks_v2", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM story_work_steps_v1", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_task_action_requests_v1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
#[path = "support/settle_task_fixture.rs"]
mod settle_fixture;

#[tokio::test]
async fn omitted_transport_defaults_preserve_exact_ledger_facts_without_inferred_rejection() {
    for omitted in ["expectedRevision","expectedCandidateRevisionId","idempotencyKey"] {
        let f=ReconciliationFixture::new(true).await;
        let mut args=f.arguments(json!("wrong-head"),"original");
        args.as_object_mut().unwrap().remove(omitted);
        let evidence=f.admit(args,"omitted-default-call");
        f.transition("cancelled");
        let before=f.domain_state();
        let (status,proof)=f.post(AUTHOR,evidence.clone()).await;
        assert_eq!(status,StatusCode::OK,"{proof}");
        assert_eq!(proof["operation"]["state"],"accepted");
        assert_eq!(f.domain_state(),before);
        let mut tampered=evidence;
        tampered["arguments"]["payload"]["premise"]=json!("changed");
        assert!(f.post(AUTHOR,tampered).await.0.is_client_error());
    }
}

use novel_codex_story_core::{ProjectRevisionRepository, RecoveryOperationsRepository, TaskInputRepository, TasksRepository, WorkTasksRepository};
