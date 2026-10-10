use super::{AUTHOR, CANDIDATE, call};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{
    ProjectId, RepositoryError, SavePlanningReviewReport, SqliteStoryRepository,
};
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use rusqlite::params;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const RAW: &str = "  保留白塔。\n仅检查人物 🕯️，不改正文。  ";
const CAPABILITY: &str = "review-test-capability-1234567890123";
type RootRecord = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    String,
);

async fn agent_context(path: &std::path::Path, uri: &str) -> (StatusCode, Value) {
    let response = router(
        RuntimeState::new(path.to_owned(), AUTHOR.into(), CANDIDATE.into())
            .with_engine_budget_database(path.with_extension("engine.sqlite3")),
    )
    .oneshot(
        Request::builder()
            .uri(uri)
            .header("authorization", format!("Bearer {CANDIDATE}"))
            .header("x-novel-task-capability", CAPABILITY)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn setup(path: &std::path::Path) -> (String, Value) {
    let (_, created) = call(
        path,
        "POST",
        "/api/projects",
        json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"来源检查"}),
        AUTHOR,
    )
    .await;
    let project = created["project"]["id"].as_str().unwrap().to_owned();
    let (status, outline) = call(path,"POST",&format!("/api/projects/{project}/artifacts/outline"),json!({
        "payload":{"nodes":[{"id":"book","kind":"book","position":1,"title":"港城","purpose":"记忆","summary":"城市消失",
            "dependencyKeys":[],"certainty":"tentative","protection":"free"}]},
        "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"outline"
    }),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{outline}");
    (
        project,
        json!({"artifactKind":"outline","revisionId":outline["revisionId"],"targetNodeId":null,
        "authorIntent":RAW,"roles":["structure","character"]}),
    )
}

#[tokio::test]
async fn author_root_and_review_work_commit_atomically_and_reject_client_identity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("root.sqlite3");
    let (project, request) = setup(&path).await;
    let uri = format!("/api/projects/{project}/planning-reviews");
    let db = rusqlite::Connection::open(&path).unwrap();
    for field in [
        "authorMessageId",
        "sourcePacketHash",
        "planningReviewAuthorIntentBinding",
    ] {
        let mut forged = request.clone();
        forged[field] = json!("forged");
        assert_eq!(
            call(&path, "POST", &uri, forged, AUTHOR).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_eq!(
        call(&path, "POST", &uri, request.clone(), CANDIDATE)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    for table in ["story_author_messages_v1", "story_work_steps_v1"] {
        db.execute_batch(&format!("CREATE TRIGGER reject_root BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT,'root failure'); END;")).unwrap();
        assert!(
            !call(&path, "POST", &uri, request.clone(), AUTHOR)
                .await
                .0
                .is_success()
        );
        let counts:(i64,i64,i64,i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_planning_review_runs_v2),(SELECT COUNT(*) FROM story_planning_review_seats_v2),(SELECT COUNT(*) FROM story_work_tasks_v1),(SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_author_messages_v1)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
        assert_eq!(counts, (0, 0, 0, 0, 0));
        db.execute_batch("DROP TRIGGER reject_root;").unwrap();
    }
    let (status, run) = call(&path, "POST", &uri, request, AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{run}");
    let run_id = run["id"].as_str().unwrap();
    assert_eq!(run["workTaskId"], format!("work-{run_id}"));
    let record:RootRecord = db.query_row(
        "SELECT m.message_id,m.project_id,m.work_task_id,m.author_text,m.task_id,m.generation_run_id,m.thread_id,m.created_at,r.source_packet_json FROM story_author_messages_v1 m JOIN story_planning_review_runs_v2 r ON r.run_id=m.review_run_id WHERE r.run_id=?1",
        [run_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?))).unwrap();
    assert_eq!(
        (
            record.0,
            record.1,
            record.2,
            record.3,
            record.4,
            record.5,
            record.6,
            record.7.to_string()
        ),
        (
            run["authorMessageId"].as_str().unwrap().into(),
            project,
            format!("work-{run_id}"),
            RAW.into(),
            None,
            None,
            None,
            run["createdAt"].as_str().unwrap().into()
        )
    );
    let packet: Value = serde_json::from_str(&record.8).unwrap();
    assert_eq!(
        (
            packet["authorIntent"].clone(),
            packet["authorMessageId"].clone(),
            run["sourcePacketHash"].clone()
        ),
        (
            json!(RAW),
            run["authorMessageId"].clone(),
            json!(format!("{:x}", Sha256::digest(record.8.as_bytes())))
        )
    );
}

#[tokio::test]
async fn persisted_review_owner_drives_source_budget_and_close_without_cross_work_writes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("owner.sqlite3");
    let (project, request) = setup(&path).await;
    let root = format!("/api/projects/{project}");
    let create_uri = format!("{root}/planning-reviews");
    let (status, first) = call(&path, "POST", &create_uri, request.clone(), AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let (_, second) = call(&path, "POST", &create_uri, request, AUTHOR).await;
    let run_id = first["id"].as_str().unwrap();
    let old_owner = first["workTaskId"].as_str().unwrap();
    let owner = "work-persisted-owner";
    let other_owner = "work-other-persisted-owner";
    let run_uri = format!("{create_uri}/{run_id}");
    let work_uri = format!("{root}/work-tasks/{owner}");
    let db = rusqlite::Connection::open(&path).unwrap();
    // A different persisted owner name exercises every read path without adding a caller-selected owner API.
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE; PRAGMA defer_foreign_keys=ON;")
        .unwrap();
    for (new_owner, prior_owner) in [
        (owner, old_owner),
        (other_owner, second["workTaskId"].as_str().unwrap()),
    ] {
        db.execute(
            "UPDATE story_work_tasks_v1 SET id=?1 WHERE id=?2",
            params![new_owner, prior_owner],
        )
        .unwrap();
        for table in [
            "story_planning_review_runs_v2",
            "story_author_messages_v1",
            "story_work_steps_v1",
        ] {
            db.execute(
                &format!("UPDATE {table} SET work_task_id=?1 WHERE work_task_id=?2"),
                params![new_owner, prior_owner],
            )
            .unwrap();
        }
    }
    db.execute_batch("COMMIT;").unwrap();
    let (status, restored) = call(&path, "GET", &run_uri, Value::Null, AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert_eq!(restored["workTaskId"], owner);
    let (_, other_before) = call(
        &path,
        "GET",
        &format!("{root}/work-tasks/{other_owner}"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let (_, work) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    let (_, workspace) = call(
        &path,
        "GET",
        &format!("{root}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(call(&path,"POST",&format!("{work_uri}/budget-authorization"),json!({
        "expectedVersion":work["version"],"expectedProjectRevision":workspace["revision"],"expectedSourceDigest":workspace["sourceDigest"],
        "maxRequests":3,"maxTokens":1000,"deadlineAt":now+600
    }),AUTHOR).await.0,StatusCode::OK);
    let before: (i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_task_provider_budgets_v1)",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    let seat_uri = format!("{run_uri}/seats/structure/tasks");
    let seat = json!({"threadId":"review-thread","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600});
    for wrong_owner in [other_owner, "work-other-project"] {
        if wrong_owner == "work-other-project" {
            let (_, other_project) = call(
                &path,
                "POST",
                "/api/projects",
                json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"另一作品"}),
                AUTHOR,
            )
            .await;
            db.execute("INSERT INTO story_work_tasks_v1(id,project_id,thread_id,goal,kind,status,input_revision) VALUES (?1,?2,'',?3,'direct','running',0)",params![wrong_owner,other_project["project"]["id"].as_str().unwrap(),RAW]).unwrap();
        }
        db.execute(
            "UPDATE story_planning_review_runs_v2 SET work_task_id=?1 WHERE run_id=?2",
            params![wrong_owner, run_id],
        )
        .unwrap();
        for (method, uri, payload) in [
            ("GET", run_uri.clone(), Value::Null),
            ("POST", seat_uri.clone(), seat.clone()),
            ("POST", format!("{run_uri}/cancel"), json!({})),
        ] {
            assert!(
                !call(&path, method, &uri, payload, AUTHOR)
                    .await
                    .0
                    .is_success(),
                "{method} {uri}"
            );
        }
        let after:(i64,i64) = db.query_row("SELECT (SELECT COUNT(*) FROM story_tasks_v2),(SELECT COUNT(*) FROM story_task_provider_budgets_v1)",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(after, before);
        let other_status: (String, i64) = db
            .query_row(
                "SELECT status,version FROM story_work_tasks_v1 WHERE id=?1",
                [other_owner],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            other_status,
            (
                other_before["status"].as_str().unwrap().into(),
                other_before["version"].as_i64().unwrap()
            )
        );
    }
    db.execute(
        "UPDATE story_planning_review_runs_v2 SET work_task_id=?1 WHERE run_id=?2",
        params![owner, run_id],
    )
    .unwrap();
    let (status, task) = call(&path, "POST", &seat_uri, seat, AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{task}");
    let task_id = task["id"].as_str().unwrap();
    assert_eq!(
        call(
            &path,
            "GET",
            &format!("{root}/tasks/{task_id}/provider-budget"),
            Value::Null,
            AUTHOR
        )
        .await
        .1["workTaskId"],
        owner
    );
    assert_eq!(agent_context(&path,&format!("{root}/context?taskId={task_id}&threadId=review-thread&projectRevision=0&purpose={}&budget=24&byteBudget=60000",task["purpose"].as_str().unwrap())).await.0,StatusCode::OK);
    let (status, cancelled) = call(
        &path,
        "POST",
        &format!("{run_uri}/cancel"),
        json!({}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert_eq!(cancelled["workTaskId"], owner);
    assert_eq!(
        call(&path, "GET", &work_uri, Value::Null, AUTHOR).await.1["status"],
        "cancelled"
    );
    assert_eq!(
        call(
            &path,
            "GET",
            &format!("{root}/work-tasks/{other_owner}"),
            Value::Null,
            AUTHOR
        )
        .await
        .1,
        other_before
    );
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{root}/work-tasks/{other_owner}/control"),
            json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":other_before["threadId"],"expectedControlRevision":other_before["controlRevision"],"command":"cancel"}),
            AUTHOR
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &path,
            "GET",
            &format!("{create_uri}/{}", second["id"].as_str().unwrap()),
            Value::Null,
            AUTHOR
        )
        .await
        .1["status"],
        "cancelled"
    );
}

#[tokio::test]
async fn damaged_author_root_or_frozen_seat_blocks_context_budget_report_and_completion() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("damaged.sqlite3");
    let (project, request) = setup(&path).await;
    let root = format!("/api/projects/{project}");
    let (status, run) = call(
        &path,
        "POST",
        &format!("{root}/planning-reviews"),
        request,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{run}");
    let run_id = run["id"].as_str().unwrap();
    let work_id = format!("work-{run_id}");
    let work_uri = format!("{root}/work-tasks/{work_id}");
    let (_, work) = call(&path, "GET", &work_uri, Value::Null, AUTHOR).await;
    let (_, workspace) = call(
        &path,
        "GET",
        &format!("{root}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(call(&path,"POST",&format!("{work_uri}/budget-authorization"),json!({
        "expectedVersion":work["version"],"expectedProjectRevision":workspace["revision"],"expectedSourceDigest":workspace["sourceDigest"],
        "maxRequests":3,"maxTokens":1000,"deadlineAt":now+600
    }),AUTHOR).await.0,StatusCode::OK);
    let seat_uri = format!("{root}/planning-reviews/{run_id}/seats/structure/tasks");
    let seat_request = json!({"threadId":"review-thread","model":"fixture","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600});
    let mut forged = seat_request.clone();
    forged["authorMessageId"] = json!("forged");
    assert!(
        !call(&path, "POST", &seat_uri, forged, AUTHOR)
            .await
            .0
            .is_success()
    );
    let (status, task) = call(&path, "POST", &seat_uri, seat_request, AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{task}");
    let task_id = task["id"].as_str().unwrap();
    assert_eq!(
        (
            task["inputManifest"]["authorIntent"].clone(),
            task["inputManifest"]["authorMessageId"].clone()
        ),
        (json!(RAW), run["authorMessageId"].clone())
    );
    let purpose = task["purpose"].as_str().unwrap();
    let context_uri = format!(
        "{root}/context?taskId={task_id}&threadId=review-thread&projectRevision=0&purpose={purpose}&budget=24&byteBudget=60000"
    );
    let (status, context) = agent_context(&path, &context_uri).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(context["operationContext"]["planningReview"]["textDelivered"],false);
    let source=context["operationContext"]["planningReview"]["sourceId"].as_str().unwrap();
    let reader=SqliteStoryRepository::open(&path).unwrap();
    let bound=reader.task(&ProjectId::parse(&project).unwrap(),task_id).unwrap().unwrap();
    let mut cursor=None;
    let mut text=String::new();
    loop {
        let page=reader.task_source_page(&bound,source,cursor.as_deref()).unwrap();
        text.push_str(page["payloadPart"].as_str().unwrap());
        cursor=page["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none(){break;}
    }
    let packet:Value=serde_json::from_str(&text).unwrap();
    assert_eq!(packet["sourcePacket"]["authorMessageId"],run["authorMessageId"]);
    let db = rusqlite::Connection::open(&path).unwrap();
    let (packet,hash):(String,String) = db.query_row("SELECT source_packet_json,source_packet_hash FROM story_planning_review_runs_v2 WHERE run_id=?1",[run_id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    let original_manifest = task["inputManifest"].to_string();
    let report = json!({"summary":"检查完成","strengths":[],"findings":[],"coverage":[{"checkId":"structure:book","status":"checked"}]});
    let project_id = ProjectId::parse(&project).unwrap();
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    for damage in [
        "message",
        "packet",
        "hash",
        "binding",
        "manifest",
        "target",
        "anonymous",
    ] {
        match damage {
            "message" => {
                db.execute("UPDATE story_author_messages_v1 SET author_text='forged' WHERE review_run_id=?1",[run_id]).unwrap();
            }
            "packet" => {
                db.execute("UPDATE story_planning_review_runs_v2 SET source_packet_json='{}' WHERE run_id=?1",[run_id]).unwrap();
            }
            "hash" => {
                db.execute("UPDATE story_planning_review_runs_v2 SET source_packet_hash='forged' WHERE run_id=?1",[run_id]).unwrap();
            }
            "binding" | "manifest" | "anonymous" => {
                let mut changed = task["inputManifest"].clone();
                if damage == "binding" {
                    changed["planningReviewAuthorIntentBinding"]["role"] = json!("character");
                } else if damage == "manifest" {
                    changed["authorIntent"] = json!("host wrapper");
                } else {
                    changed
                        .as_object_mut()
                        .unwrap()
                        .remove("planningReviewAuthorIntentBinding");
                }
                db.execute(
                    "UPDATE story_tasks_v2 SET input_manifest_json=?2 WHERE task_id=?1",
                    params![task_id, changed.to_string()],
                )
                .unwrap();
            }
            "target" => {
                let target = json!({});
                let mut changed = task["inputManifest"].clone();
                // Repair the independent method checksum so this isolates the author-source gate.
                let binding = &mut changed["creativeCapabilityBinding"];
                binding["targetDigest"] = json!(format!(
                    "{:x}",
                    Sha256::digest(target.to_string().as_bytes())
                ));
                binding.as_object_mut().unwrap().remove("bindingDigest");
                binding["bindingDigest"] = json!(format!(
                    "{:x}",
                    Sha256::digest(binding.to_string().as_bytes())
                ));
                db.execute("UPDATE story_tasks_v2 SET target_json=?2,input_manifest_json=?3 WHERE task_id=?1",params![task_id,target.to_string(),changed.to_string()]).unwrap();
            }
            _ => unreachable!(),
        }
        let before = repo.task(&project_id, task_id).unwrap().unwrap();
        assert!(
            matches!(
                repo.assert_task_inputs_current(&before),
                Err(RepositoryError::TaskBindingInvalid)
            ),
            "{damage}"
        );
        assert!(
            !agent_context(&path, &context_uri).await.0.is_success(),
            "{damage}"
        );
        assert!(
            !call(
                &path,
                "GET",
                &format!("{root}/tasks/{task_id}/provider-budget"),
                Value::Null,
                AUTHOR
            )
            .await
            .0
            .is_success(),
            "{damage}"
        );
        if matches!(damage, "message" | "packet" | "hash") {
            assert!(
                !call(
                    &path,
                    "GET",
                    &format!("{work_uri}/budget-authorization"),
                    Value::Null,
                    AUTHOR
                )
                .await
                .0
                .is_success(),
                "{damage}"
            );
        }
        assert!(
            matches!(
                repo.save_planning_review_report(SavePlanningReviewReport {
                    project_id: &project_id,
                    run_id,
                    task_id,
                    thread_id: "review-thread",
                    expected_revision: 0,
                    report: &report,
                    now: now as i64
                }),
                Err(RepositoryError::TaskBindingInvalid)
            ),
            "{damage}"
        );
        assert_eq!(repo.task(&project_id, task_id).unwrap().unwrap(), before);
        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM story_planning_review_reports_v2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        db.execute(
            "UPDATE story_author_messages_v1 SET author_text=?2 WHERE review_run_id=?1",
            params![run_id, RAW],
        )
        .unwrap();
        db.execute("UPDATE story_planning_review_runs_v2 SET source_packet_json=?2,source_packet_hash=?3 WHERE run_id=?1",params![run_id,packet,hash]).unwrap();
        db.execute(
            "UPDATE story_tasks_v2 SET input_manifest_json=?2,target_json=?3 WHERE task_id=?1",
            params![task_id, original_manifest, task["target"].to_string()],
        )
        .unwrap();
    }
    repo.save_planning_review_report(SavePlanningReviewReport {
        project_id: &project_id,
        run_id,
        task_id,
        thread_id: "review-thread",
        expected_revision: 0,
        report: &report,
        now: now as i64,
    })
    .unwrap();
    let before = repo.task(&project_id, task_id).unwrap().unwrap();
    db.execute(
        "DELETE FROM story_author_messages_v1 WHERE review_run_id=?1",
        [run_id],
    )
    .unwrap();
    assert!(matches!(
        repo.complete_task(
            &project_id,
            task_id,
            "review-thread",
            "done".into(), None,
            now as i64
        ),
        Err(RepositoryError::TaskBindingInvalid)
    ));
    assert_eq!(repo.task(&project_id, task_id).unwrap().unwrap(), before);
    db.execute("INSERT INTO story_author_messages_v1(message_id,project_id,work_task_id,review_run_id,intent_revision,author_text,created_at) VALUES (?1,?2,?3,?4,0,?5,?6)",params![run["authorMessageId"].as_str().unwrap(),project,work_id,run_id,RAW,run["createdAt"].as_str().unwrap().parse::<i64>().unwrap()]).unwrap();
    repo.complete_task(
        &project_id,
        task_id,
        "review-thread",
        "done".into(), None,
        now as i64,
    )
    .unwrap();
}
