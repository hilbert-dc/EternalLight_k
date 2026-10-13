//! Recovery uses real HTTP/SQLite transitions with scripted candidate payloads, not external models.
use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "support/continuous_recovery.rs"]
mod support;
use support::{AUTHOR, BODY, CANDIDATE, CAPABILITY, Runtime, draft};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn author_resume_after_published_model_failure_survives_runtime_restart() {
    let mut runtime = Runtime::new().await;
    let project = runtime
        .post("/api/projects", json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"恢复已保存的知识"}))
        .await;
    let root = format!(
        "/api/projects/{}",
        project["project"]["id"].as_str().unwrap()
    );
    let workspace_uri = format!("{root}/creative-workspace");
    let saved_source = runtime
        .post(
            &format!("{root}/drafts/checkpoints"),
            json!({
                "payload":draft(json!("chapter-one"), BODY),"expectedRevision":0,
                "expectedCandidateRevisionId":null,"idempotencyKey":"saved-source"
            }),
        )
        .await;
    let batch = runtime
        .post(
            &format!("{root}/modeling-batches"),
            json!({
                "objectId":"draft-chapter-one","sourceRevisionId":saved_source["revisionId"],
                "expectedRevision":0,"extractorVersion":"author-request"
            }),
        )
        .await;
    let target = json!({"batchId":batch["id"],"chapterId":"chapter-one"});
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let workspace = runtime.get(&workspace_uri).await;
    let task = runtime.post(&format!("{root}/tasks"), json!({
        "operation":"modelKnowledge","target":target,"inputRevision":0,"threadId":"model-root",
        "authorIntent":"分析已保存章节的知识","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
        "capabilityExpiresAt":expires,"initialBudget":{
            "authorizationId":"recovery-model-budget","authorMessageId":"recovery-model-message",
            "expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],
            "maxRequests":3,"maxTokens":700,"deadlineAt":expires}
    })).await;
    let task_uri = format!("{root}/tasks/{}", task["id"].as_str().unwrap());
    let work = runtime.get(&format!("{task_uri}/work-task")).await;
    let work_uri = format!("{root}/work-tasks/{}", work["id"].as_str().unwrap());
    let authorization = runtime
        .get(&format!("{work_uri}/budget-authorization"))
        .await;
    let grant = runtime.get(&format!("{task_uri}/provider-budget")).await;
    let batch_uri = format!(
        "{root}/modeling-batches/{}",
        target["batchId"].as_str().unwrap()
    );
    runtime.candidate(&format!("{batch_uri}/publish"), json!({
        "taskId":task["id"],"threadId":"model-root","expectedRevision":0,"entities":[],"assertions":[]
    })).await;
    runtime
        .ok(
            "PATCH",
            &task_uri,
            AUTHOR,
            Some(json!({
                "threadId":"model-root","status":"failed","error":"provider_output_limit"
            })),
        )
        .await;
    runtime.spend(&task, &grant);
    let budget = runtime
        .post(
            &format!("{work_uri}/provider-usage"),
            json!({"taskId":task["id"]}),
        )
        .await;
    let blocked = runtime.get(&work_uri).await;
    assert_eq!(blocked["resumeMode"], "recoverSavedResult");
    assert_eq!(runtime.get(&format!("{task_uri}/work-task")).await, blocked);
    let listed = runtime.get(&format!("{root}/work-tasks")).await;
    assert_eq!(listed["tasks"][0], blocked);
    assert_eq!(
        (
            blocked["status"].clone(),
            blocked["resumeBlockReason"].clone()
        ),
        (json!("blocked"), Value::Null)
    );
    let saved = runtime.get(&workspace_uri).await;
    let knowledge_uri = format!("{root}/knowledge");
    let batch = runtime.get(&knowledge_uri).await;
    runtime.restart().await;
    assert_eq!(runtime.get(&work_uri).await, blocked);
    let restored = runtime
        .post(
            &format!("{work_uri}/control"),
            json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":blocked["threadId"],
                "command":"recoverSavedResult","expectedControlRevision":blocked["controlRevision"]
            }),
        )
        .await;
    assert_eq!(restored["status"], "completed");
    assert_eq!(restored["resumeMode"], Value::Null);
    assert_eq!(
        restored["steps"][0]["attempts"].as_array().unwrap().len(),
        1
    );
    assert_eq!(runtime.get(&knowledge_uri).await, batch);
    assert_eq!(
        runtime
            .get(&format!("{work_uri}/budget-authorization"))
            .await,
        authorization
    );
    let ledger: String = rusqlite::Connection::open(&runtime.path)
        .unwrap()
        .query_row(
            "SELECT ledger_json FROM story_work_budgets_v1 WHERE work_task_id=?1",
            [restored["id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&ledger).unwrap(), budget);
    let after = runtime.get(&workspace_uri).await;
    assert_eq!(after["chapters"], saved["chapters"]);
    assert_eq!(after["headSnapshotId"], saved["headSnapshotId"]);
    assert_eq!(after["revision"], saved["revision"]);
    let recovered = runtime.get(&task_uri).await;
    assert_eq!(recovered["status"], "completed");
    assert_eq!(recovered["error"], "provider_output_limit");
    assert_eq!(
        runtime
            .call(
                "POST",
                &format!("{work_uri}/control"),
                AUTHOR,
                Some(json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":blocked["threadId"],"command":"resume","expectedControlRevision":blocked["controlRevision"]}))
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_fences_active_writer_checkpoint_and_completion() {
    let runtime = Runtime::new().await;
    let (root, uri) = runtime.start().await;
    let writer = runtime.writer(&root, &uri, "paused-writer").await;
    let saved = runtime
        .save_writer(&root, &writer, BODY, "before-pause")
        .await;
    let active = runtime.get(&uri).await;
    let paused = runtime
        .post(
            &format!("{uri}/control"),
            json!({"expectedVersion":active["version"],"command":"pause"}),
        )
        .await;
    let before = runtime.get(&format!("{root}/creative-workspace")).await;
    let write=runtime.call("POST",&format!("{root}/drafts/checkpoints"),CANDIDATE,Some(json!({
        "payload":draft(writer["target"]["chapterId"].clone(),"林夕烧掉密函。"),"expectedRevision":before["revision"],
        "expectedCandidateRevisionId":saved["revisionId"],"idempotencyKey":"late-paused-write","taskId":writer["id"],"threadId":"paused-writer"
    }))).await;
    let completion = runtime
        .call(
            "POST",
            &format!("{root}/tasks/{}/complete", writer["id"].as_str().unwrap()),
            CANDIDATE,
            Some(json!({"threadId":"paused-writer","summary":"迟到交付"})),
        )
        .await;
    assert!(
        write.0.is_client_error() && completion.0.is_client_error(),
        "write={write:?};completion={completion:?}"
    );
    assert_eq!(
        runtime.get(&format!("{root}/creative-workspace")).await,
        before
    );
    assert_eq!(runtime.get(&uri).await, paused);
}

async fn pause_edit_replan(
    runtime: &Runtime,
    root: &str,
    uri: &str,
    body: &str,
) -> (Value, Value, Value) {
    let run = runtime.get(uri).await;
    let paused = runtime
        .post(
            &format!("{uri}/control"),
            json!({"expectedVersion":run["version"],"command":"pause"}),
        )
        .await;
    let workspace = runtime.get(&format!("{root}/creative-workspace")).await;
    let saved=runtime.post(&format!("{root}/drafts/checkpoints"),json!({
        "payload":draft(paused["currentChapterId"].clone(),body),"expectedRevision":workspace["revision"],
        "expectedCandidateRevisionId":workspace["chapters"][0]["candidate"]["id"],"idempotencyKey":"author-revised-source"
    })).await;
    let authored = runtime.get(&format!("{root}/creative-workspace")).await;
    assert_ne!(authored["sourceDigest"], paused["sourceDigest"]);
    assert!(
        runtime
            .call(
                "POST",
                &format!("{uri}/control"),
                AUTHOR,
                Some(json!({"expectedVersion":paused["version"],"command":"resume"}))
            )
            .await
            .0
            .is_client_error()
    );
    assert_eq!(runtime.get(uri).await, paused);
    let preview = runtime.get(&format!("{uri}/replan")).await;
    assert_eq!(preview["availableFrom"], json!(["currentChapter"]));
    let replanned=runtime.post(&format!("{uri}/replan"),json!({"expectedVersion":preview["run"]["version"],
        "expectedProjectRevision":preview["projectRevision"],"expectedSourceDigest":preview["sourceDigest"],
        "restartFrom":"currentChapter","instruction":"保留藏信改稿，补全本章"})).await;
    assert_eq!(allowance(&replanned), allowance(&paused));
    assert_eq!(replanned["intentGeneration"], 1);
    assert_eq!(replanned["currentChapterId"], paused["currentChapterId"]);
    assert_eq!(replanned["stage"], "coordinate");
    assert_eq!(replanned["sourceDigest"], authored["sourceDigest"]);
    assert_eq!(replanned["baseSnapshotId"], authored["headSnapshotId"]);
    assert_eq!(
        replanned["replans"][0]["previousSourceDigest"],
        paused["sourceDigest"]
    );
    assert_eq!(
        replanned["replans"][0]["previousSnapshotId"],
        paused["baseSnapshotId"]
    );
    (paused, replanned, saved)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paused_author_edit_replan_reopen_continue_and_cancel_preserve_lineage() {
    let mut runtime = Runtime::new().await;
    let (root, uri) = runtime.start().await;
    let (old_writer, _) = runtime.saved_chapter(&root, &uri, BODY).await;
    let work_uri = format!("{root}/work-tasks/work-{}", uri.rsplit('/').next().unwrap());
    let original_work = runtime.get(&work_uri).await;
    let (paused, replanned, edited) =
        pause_edit_replan(&runtime, &root, &uri, "林夕看见灯光，藏好密函。").await;
    let repaused = runtime
        .post(
            &format!("{uri}/control"),
            json!({"expectedVersion":replanned["version"],"command":"pause"}),
        )
        .await;
    let workspace = runtime.get(&format!("{root}/creative-workspace")).await;
    let work = runtime.get(&work_uri).await;
    runtime.restart().await;
    assert_eq!(runtime.get(&uri).await, repaused);
    assert_eq!(runtime.get(&work_uri).await, work);
    assert_eq!(
        runtime.get(&format!("{root}/creative-workspace")).await,
        workspace
    );
    let resumed = runtime
        .post(
            &format!("{uri}/control"),
            json!({"expectedVersion":repaused["version"],"command":"resume"}),
        )
        .await;
    assert_eq!(allowance(&resumed), allowance(&paused));
    let writer = runtime.writer(&root, &uri, "continued-writer").await;
    assert!(
        writer["inputManifest"]["authorIntent"]
            .as_str()
            .unwrap()
            .contains("保留藏信改稿")
    );
    assert_eq!(
        writer["inputManifest"]["expectedCandidateRevisionId"],
        edited["revisionId"]
    );
    let resumed_workspace = runtime.get(&format!("{root}/creative-workspace")).await;
    let mut source_before = workspace.clone();
    let mut source_after = resumed_workspace.clone();
    source_before.as_object_mut().unwrap().remove("tasks");
    source_after.as_object_mut().unwrap().remove("tasks");
    assert_eq!(source_after, source_before);
    let tasks = resumed_workspace["tasks"].as_array().unwrap();
    assert_eq!(
        tasks.len(),
        workspace["tasks"].as_array().unwrap().len() + 2
    );
    assert!(tasks.iter().any(|task| task["id"] == writer["id"]));
    let mut request = json!({"payload":draft(writer["target"]["chapterId"].clone(),"林夕藏好密函，循灯光走向白塔。"),
        "expectedRevision":workspace["revision"],"expectedCandidateRevisionId":edited["revisionId"],"idempotencyKey":"old-writer-rejected",
        "taskId":old_writer["id"],"threadId":old_writer["threadId"]});
    let before = runtime.get(&uri).await;
    assert!(
        runtime
            .call(
                "POST",
                &format!("{root}/drafts/checkpoints"),
                CANDIDATE,
                Some(request.clone())
            )
            .await
            .0
            .is_client_error()
    );
    assert_eq!(runtime.get(&uri).await, before);
    assert_eq!(
        runtime.get(&format!("{root}/creative-workspace")).await,
        resumed_workspace
    );
    let grant = runtime
        .get(&format!(
            "{root}/tasks/{}/provider-budget",
            writer["id"].as_str().unwrap()
        ))
        .await;
    let cancelled = runtime
        .post(
            &format!("{uri}/control"),
            json!({"expectedVersion":before["version"],"command":"cancel"}),
        )
        .await;
    request["taskId"] = writer["id"].clone();
    request["threadId"] = writer["threadId"].clone();
    request["idempotencyKey"] = json!("cancelled-writer-rejected");
    assert!(
        runtime
            .call(
                "POST",
                &format!("{root}/drafts/checkpoints"),
                CANDIDATE,
                Some(request)
            )
            .await
            .0
            .is_client_error()
    );
    let charged = runtime.settle(&uri, &writer, &grant).await;
    assert_eq!(charged["status"], "cancelled");
    assert_eq!(charged["usedTaskAttempts"], cancelled["usedTaskAttempts"]);
    assert_eq!(charged["providerBudget"]["usedRequests"], 4);
    assert_eq!(charged["providerBudget"]["usedTokens"], 28);
    runtime.restart().await;
    assert_eq!(runtime.get(&uri).await, charged);
    assert_eq!(
        runtime.post(&format!("{uri}/next"), json!({})).await,
        json!({"run":charged,"action":null,"stopReason":null})
    );
    assert!(
        runtime
            .call(
                "POST",
                &format!("{uri}/control"),
                AUTHOR,
                Some(json!({"expectedVersion":charged["version"],"command":"resume"}))
            )
            .await
            .0
            .is_client_error()
    );
    let final_work = runtime.get(&work_uri).await;
    assert_eq!(final_work["status"], "cancelled");
    let old = original_work["steps"].as_array().unwrap();
    assert_eq!(&final_work["steps"].as_array().unwrap()[..old.len()], old);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_active_attempt_can_settle_spent_usage_without_reopening_run() {
    let mut runtime = Runtime::new().await;
    let (root, uri) = runtime.start().await;
    let writer = runtime.writer(&root, &uri, "cancelled-writer").await;
    let grant = runtime
        .get(&format!(
            "{root}/tasks/{}/provider-budget",
            writer["id"].as_str().unwrap()
        ))
        .await;
    let active = runtime.get(&uri).await;
    runtime.spend(&writer, &grant);
    let cancelled = runtime
        .post(
            &format!("{uri}/control"),
            json!({"expectedVersion":active["version"],"command":"cancel"}),
        )
        .await;
    let charged = runtime
        .post(
            &format!("{uri}/provider-usage"),
            json!({"taskId":writer["id"]}),
        )
        .await;
    assert_eq!(charged["status"], "cancelled");
    assert_eq!(charged["usedTaskAttempts"], cancelled["usedTaskAttempts"]);
    assert_eq!(charged["deadlineAt"], cancelled["deadlineAt"]);
    assert_eq!(charged["providerBudget"]["usedRequests"], 2);
    assert_eq!(charged["providerBudget"]["usedTokens"], 14);
    runtime.restart().await;
    assert_eq!(
        runtime
            .post(
                &format!("{uri}/provider-usage"),
                json!({"taskId":writer["id"]})
            )
            .await,
        charged
    );
    assert_eq!(
        runtime.post(&format!("{uri}/next"), json!({})).await,
        json!({"run":charged,"action":null,"stopReason":null})
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replan_after_author_edit_delivers_only_the_new_draft_receipt() {
    let mut runtime = Runtime::new().await;
    let (root, uri) = runtime.start().await;
    let (_, old) = runtime.saved_chapter(&root, &uri, BODY).await;
    let (paused, replanned, edited) =
        pause_edit_replan(&runtime, &root, &uri, "林夕看见灯光，藏好密函。").await;
    runtime.restart().await;
    assert_eq!(runtime.get(&uri).await, replanned);
    // A replan is not a blanket exemption for another chapter or an invented
    // intent generation. Corrupt only the fixture store; HTTP must fail closed
    // without creating a task or mutating the run, then recover after restoration.
    let db = rusqlite::Connection::open(&runtime.path).unwrap();
    let run_id = replanned["id"].as_str().unwrap();
    let original: String = db
        .query_row(
            "SELECT state_json FROM story_generation_runs_v1 WHERE run_id=?1",
            [run_id],
            |r| r.get(0),
        )
        .unwrap();
    let task_count: i64 = db
        .query_row("SELECT COUNT(*) FROM story_tasks_v2", [], |r| r.get(0))
        .unwrap();
    for (field, value) in [
        ("currentChapterId", json!("another-chapter")),
        ("currentChapterNumber", json!(2)),
        ("intentGeneration", json!(2)),
    ] {
        let mut corrupt: Value = serde_json::from_str(&original).unwrap();
        corrupt[field] = value;
        let encoded = corrupt.to_string();
        db.execute(
            "UPDATE story_generation_runs_v1 SET state_json=?1 WHERE run_id=?2",
            rusqlite::params![encoded, run_id],
        )
        .unwrap();
        let (status, response) = runtime
            .call("POST", &format!("{uri}/next"), AUTHOR, Some(json!({})))
            .await;
        assert!(status.is_client_error(), "{field}: {response}");
        assert_eq!(
            db.query_row(
                "SELECT state_json FROM story_generation_runs_v1 WHERE run_id=?1",
                [run_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            encoded
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM story_tasks_v2", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            task_count
        );
        db.execute(
            "UPDATE story_generation_runs_v1 SET state_json=?1 WHERE run_id=?2",
            rusqlite::params![original, run_id],
        )
        .unwrap();
    }
    drop(db);
    let writer = runtime.writer(&root, &uri, "replanned-writer").await;
    assert_eq!(
        writer["inputManifest"]["expectedCandidateRevisionId"],
        edited["revisionId"]
    );
    let saved = runtime
        .save_writer(
            &root,
            &writer,
            "林夕看见灯光，藏好密函，走向白塔。",
            "replanned-prose",
        )
        .await;
    assert_ne!(saved["revisionId"], old["revisionId"]);
    assert_ne!(saved["revisionId"], edited["revisionId"]);
    runtime.complete_and_advance(&root, &uri, &writer).await;
    let completed = runtime.finish_chapter(&root, &uri).await;
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["completedChapters"], 1);
    assert_eq!(completed["intentGeneration"], 1);
    assert_eq!(completed["deadlineAt"], paused["deadlineAt"]);
    assert_eq!(completed["usedTaskAttempts"], 7);
    assert_eq!(completed["providerBudget"]["usedRequests"], 7);
    assert_eq!(completed["providerBudget"]["usedTokens"], 49);
    assert_eq!(
        completed["chapterDeliveries"][0]["draftRevisionId"],
        saved["revisionId"]
    );
    assert_eq!(
        completed["chapterDeliveries"][0]["knowledgeBatchId"],
        Value::Null
    );
    assert_eq!(
        completed["chapterDeliveries"][0]["assessmentRevisionId"],
        Value::Null
    );
    runtime.restart().await;
    assert_eq!(runtime.get(&uri).await, completed);
    assert_eq!(
        runtime.get(&format!("{root}/creative-workspace")).await["chapters"][0]["accepted"],
        Value::Null
    );
}

fn allowance(run: &Value) -> Value {
    [
        "usedTaskAttempts",
        "maxTaskAttempts",
        "retryAttempts",
        "maxRetryAttempts",
        "deadlineAt",
        "completedChapters",
        "targetChapters",
        "providerBudget",
    ]
    .into_iter()
    .map(|key| (key.to_owned(), run[key].clone()))
    .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn author_continuation_recovers_complete_saved_writer_without_rewriting_or_new_allowance() {
    recover_saved_writer("retry-failed-task").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checkpoint_rebind_recovers_complete_saved_writer_without_rewriting_or_new_allowance() {
    recover_saved_writer("rebind").await;
}

async fn recover_saved_writer(route: &str) {
    let mut runtime = Runtime::new().await;
    let (root, uri) = runtime.start().await;
    let writer = runtime.writer(&root, &uri, "saved-writer").await;
    let saved = runtime
        .save_writer(&root, &writer, BODY, "saved-before-failure")
        .await;
    let task_uri = format!("{root}/tasks/{}", writer["id"].as_str().unwrap());
    let grant = runtime.get(&format!("{task_uri}/provider-budget")).await;
    runtime
        .ok(
            "PATCH",
            &task_uri,
            AUTHOR,
            Some(json!({"threadId":writer["threadId"],
        "status":"failed","error":"context_item_limit"})),
        )
        .await;
    let settled = runtime.settle(&uri, &writer, &grant).await;
    let stopped = runtime.post(&format!("{uri}/next"), json!({})).await;
    assert_eq!(stopped["run"]["status"], "blocked");
    let work_uri = format!("{root}/work-tasks/work-{}", settled["id"].as_str().unwrap());
    let blocked = runtime.get(&work_uri).await;
    assert_eq!(blocked["status"], "blocked");
    assert_eq!(blocked["resumeMode"], "continueExecution");
    assert_eq!(
        runtime.get(&format!("{root}/work-tasks")).await["tasks"][0],
        blocked
    );
    // A query index is not a second generation authority. Read projection must
    // remain truthful without writing or repairing a stale index as a side effect.
    let db = rusqlite::Connection::open(&runtime.path).unwrap();
    db.execute(
        "UPDATE story_work_tasks_v1 SET status='running' WHERE id=?1",
        [blocked["id"].as_str().unwrap()],
    )
    .unwrap();
    assert_eq!(runtime.get(&work_uri).await, blocked);
    assert_eq!(
        db.query_row(
            "SELECT status FROM story_work_tasks_v1 WHERE id=?1",
            [blocked["id"].as_str().unwrap()],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "running"
    );
    drop(db);
    let workspace = runtime.get(&format!("{root}/creative-workspace")).await;
    let run = runtime.get(&uri).await;
    runtime.restart().await;
    assert_eq!(runtime.get(&work_uri).await, blocked);
    let request = if route == "rebind" {
        let preview = runtime.get(&format!("{uri}/replan")).await;
        json!({"expectedVersion":run["version"],"expectedProjectRevision":preview["projectRevision"],
            "expectedSourceDigest":preview["sourceDigest"],"restartFrom":"currentChapter"})
    } else {
        json!({"expectedVersion":run["version"]})
    };
    let resumed = runtime.post(&format!("{uri}/{route}"), request).await;
    assert_eq!(resumed["providerBudget"], run["providerBudget"]);
    assert_eq!(resumed["usedTaskAttempts"], run["usedTaskAttempts"]);
    assert_eq!(resumed["deadlineAt"], run["deadlineAt"]);
    assert_eq!(resumed["targetChapters"], run["targetChapters"]);
    let recovered = runtime.get(&task_uri).await;
    assert_eq!(recovered["status"], "completed");
    assert_eq!(recovered["error"], "context_item_limit");
    let restored_workspace = runtime.get(&format!("{root}/creative-workspace")).await;
    for field in [
        "chapters",
        "artifacts",
        "headSnapshotId",
        "revision",
        "sourceDigest",
    ] {
        assert_eq!(
            restored_workspace[field], workspace[field],
            "saved story data changed: {field}"
        );
    }
    assert!(
        runtime
            .call("GET", &format!("{task_uri}/provider-budget"), AUTHOR, None)
            .await
            .0
            .is_client_error(),
        "a recovered completed attempt must not obtain another execution grant"
    );
    let db = rusqlite::Connection::open(&runtime.path).unwrap();
    let (stored_grant, settled): (String, bool) = db
        .query_row(
            "SELECT grant_json,settled FROM story_task_provider_budgets_v1 WHERE task_id=?1",
            [writer["id"].as_str().unwrap()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&stored_grant).unwrap(), grant);
    assert!(settled);
    drop(db);
    let completed = runtime.finish_chapter(&root, &uri).await;
    assert_eq!(completed["status"], "completed");
    assert_eq!(
        completed["chapterDeliveries"][0]["draftRevisionId"],
        saved["revisionId"]
    );
    assert_eq!(completed["usedTaskAttempts"], 5);
    let work = runtime.get(&work_uri).await;
    assert_eq!(
        work["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|step| step["operation"] == "generateChapter")
            .count(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn author_continuation_does_not_recover_partial_draft_as_complete() {
    let runtime = Runtime::new().await;
    let (root, uri) = runtime.start().await;
    let writer = runtime.writer(&root, &uri, "partial-writer").await;
    let mut payload = draft(writer["target"]["chapterId"].clone(), BODY);
    payload["completeness"] = json!("partial");
    let saved = runtime.candidate(&format!("{root}/drafts/checkpoints"), json!({
        "payload":payload,"expectedRevision":writer["inputRevision"],
        "expectedCandidateRevisionId":writer["inputManifest"]["expectedCandidateRevisionId"],
        "idempotencyKey":"partial-before-failure","taskId":writer["id"],"threadId":writer["threadId"]
    })).await;
    let task_uri = format!("{root}/tasks/{}", writer["id"].as_str().unwrap());
    let grant = runtime.get(&format!("{task_uri}/provider-budget")).await;
    runtime
        .ok(
            "PATCH",
            &task_uri,
            AUTHOR,
            Some(json!({"threadId":writer["threadId"],
        "status":"failed","error":"context_item_limit"})),
        )
        .await;
    runtime.settle(&uri, &writer, &grant).await;
    let blocked = runtime.post(&format!("{uri}/next"), json!({})).await;
    let resumed = runtime
        .post(
            &format!("{uri}/retry-failed-task"),
            json!({"expectedVersion":blocked["run"]["version"]}),
        )
        .await;
    assert_eq!(runtime.get(&task_uri).await["status"], "failed");
    assert_eq!(resumed["completedChapters"], 0);
    let retry = runtime.next_task(&root, &uri, "retry-partial").await;
    assert_eq!(retry["operation"], "generateChapter");
    assert_ne!(retry["id"], writer["id"]);
    assert_eq!(
        retry["inputManifest"]["expectedCandidateRevisionId"],
        saved["revisionId"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn author_continuation_rejects_unresolved_write_even_with_a_saved_draft() {
    let runtime = Runtime::new().await;
    let (root, uri) = runtime.start().await;
    let writer = runtime.writer(&root, &uri, "unresolved-writer").await;
    runtime
        .save_writer(&root, &writer, BODY, "saved-before-unknown-write")
        .await;
    let mut repository =
        novel_codex_story_core::SqliteStoryRepository::open(&runtime.path).unwrap();
    repository
        .bind_recovery_operation(novel_codex_story_core::RecoveryOperationBinding {
            project_id: novel_codex_story_core::ProjectId::parse(
                writer["projectId"].as_str().unwrap(),
            )
            .unwrap(),
            task_id: writer["id"].as_str().unwrap().into(),
            thread_id: writer["threadId"].as_str().unwrap().into(),
            turn_id: "interrupted-turn".into(),
            call_id: "unknown-write".into(),
            tool_name: "save_draft_checkpoint".into(),
            request_hash: "0".repeat(64),
            idempotency_ref: Some(format!("sha256:{}", "1".repeat(64))),
            now: 1,
        })
        .unwrap();
    drop(repository);
    let task_uri = format!("{root}/tasks/{}", writer["id"].as_str().unwrap());
    let grant = runtime.get(&format!("{task_uri}/provider-budget")).await;
    runtime
        .ok(
            "PATCH",
            &task_uri,
            AUTHOR,
            Some(json!({"threadId":writer["threadId"],
        "status":"failed","error":"context_item_limit"})),
        )
        .await;
    runtime.settle(&uri, &writer, &grant).await;
    let blocked = runtime.post(&format!("{uri}/next"), json!({})).await;
    assert_eq!(blocked["stopReason"], "pendingRecovery");
    let before = runtime.get(&uri).await;
    let (status, _) = runtime
        .call(
            "POST",
            &format!("{uri}/retry-failed-task"),
            AUTHOR,
            Some(json!({"expectedVersion":before["version"]})),
        )
        .await;
    assert!(status.is_client_error());
    assert_eq!(runtime.get(&uri).await, before);
    assert_eq!(runtime.get(&task_uri).await["status"], "failed");
    let preview = runtime.get(&format!("{uri}/replan")).await;
    let (status, _) = runtime.call("POST", &format!("{uri}/rebind"), AUTHOR,
        Some(json!({"expectedVersion":before["version"],"expectedProjectRevision":preview["projectRevision"],
            "expectedSourceDigest":preview["sourceDigest"],"restartFrom":"currentChapter"}))).await;
    assert!(status.is_client_error());
    assert_eq!(runtime.get(&uri).await, before);
    assert_eq!(runtime.get(&task_uri).await["status"], "failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn author_pause_resumes_after_restart_with_original_budget_and_saved_output() {
    for mode in ["empty", "partial", "complete"] {
        let mut r = Runtime::new().await;
        let (root, uri) = r.start().await;
        let w = r.writer(&root, &uri, "author-paused").await;
        let task_uri = format!("{root}/tasks/{}", w["id"].as_str().unwrap());
        let grant = r.get(&format!("{task_uri}/provider-budget")).await;
        let mut saved = Value::Null;
        if mode != "empty" {
            let mut payload = draft(w["target"]["chapterId"].clone(), BODY);
            if mode == "partial" {
                payload["completeness"] = json!("partial");
            }
            saved=r.candidate(&format!("{root}/drafts/checkpoints"),json!({"payload":payload,"expectedRevision":w["inputRevision"],"expectedCandidateRevisionId":w["inputManifest"]["expectedCandidateRevisionId"],"idempotencyKey":"before-author-pause","taskId":w["id"],"threadId":w["threadId"]})).await;
        }
        let active = r.get(&uri).await;
        let paused = r
            .post(
                &format!("{uri}/control"),
                json!({"expectedVersion":active["version"],"command":"pause"}),
            )
            .await;
        let task = r.get(&task_uri).await;
        assert_eq!(task["status"], "cancelled");
        assert_eq!(task["error"], "author_paused");
        let (status, _) = r
            .call(
                "POST",
                &format!("{uri}/control"),
                AUTHOR,
                Some(json!({"expectedVersion":paused["version"],"command":"resume"})),
            )
            .await;
        assert!(status.is_client_error());
        assert_eq!(r.get(&uri).await, paused);
        r.settle(&uri, &w, &grant).await;
        r.restart().await;
        let before = r.get(&uri).await;
        let resumed = r
            .post(
                &format!("{uri}/control"),
                json!({"expectedVersion":before["version"],"command":"resume"}),
            )
            .await;
        assert_eq!(resumed["status"], "running");
        for field in [
            "retryAttempts",
            "usedTaskAttempts",
            "maxRetryAttempts",
            "maxTaskAttempts",
            "deadlineAt",
            "targetChapters",
            "providerBudget",
        ] {
            assert_eq!(resumed[field], before[field], "{mode}: {field}");
        }
        let db = rusqlite::Connection::open(&r.path).unwrap();
        let (stored_grant, settled): (String, bool) = db
            .query_row(
                "SELECT grant_json,settled FROM story_task_provider_budgets_v1 WHERE task_id=?1",
                [w["id"].as_str().unwrap()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&stored_grant).unwrap(), grant);
        assert!(settled);
        drop(db);
        if mode == "complete" {
            assert_eq!(r.get(&task_uri).await["status"], "completed");
            assert_eq!(
                r.get(&format!("{root}/creative-workspace")).await["chapters"][0]["candidate"]["id"],
                saved["revisionId"]
            );
            let completed = r.finish_chapter(&root, &uri).await;
            assert_eq!(completed["status"], "completed");
            assert_eq!(
                completed["usedTaskAttempts"].as_i64(),
                Some(before["usedTaskAttempts"].as_i64().unwrap() + 3)
            );
        } else {
            let retry = r.next_task(&root, &uri, "resumed-writer").await;
            assert_eq!(retry["operation"], "generateChapter");
            assert_eq!(retry["target"]["chapterId"], w["target"]["chapterId"]);
            assert_ne!(retry["id"], w["id"]);
            assert_eq!(
                retry["inputManifest"]["expectedCandidateRevisionId"],
                saved["revisionId"]
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pausing_a_failed_attempt_does_not_bypass_retry_policy() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let w = r.writer(&root, &uri, "failed-before-pause").await;
    let task_uri = format!("{root}/tasks/{}", w["id"].as_str().unwrap());
    let grant = r.get(&format!("{task_uri}/provider-budget")).await;
    r.ok(
        "PATCH",
        &task_uri,
        AUTHOR,
        Some(
            json!({"threadId":w["threadId"],"status":"failed","error":"provider_quota_exhausted"}),
        ),
    )
    .await;
    r.settle(&uri, &w, &grant).await;
    let blocked = r.post(&format!("{uri}/next"), json!({})).await;
    let paused = r
        .post(
            &format!("{uri}/control"),
            json!({"expectedVersion":blocked["run"]["version"],"command":"pause"}),
        )
        .await;
    assert_eq!(r.get(&task_uri).await["error"], "provider_quota_exhausted");
    let (status, _) = r
        .call(
            "POST",
            &format!("{uri}/control"),
            AUTHOR,
            Some(json!({"expectedVersion":paused["version"],"command":"resume"})),
        )
        .await;
    assert!(status.is_client_error());
    assert_eq!(r.get(&uri).await, paused);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn author_paused_resume_rejects_unknown_write_atomically() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let w = r.writer(&root, &uri, "paused-unknown").await;
    r.save_writer(&root, &w, BODY, "saved-before-pause").await;
    let mut repo = novel_codex_story_core::SqliteStoryRepository::open(&r.path).unwrap();
    repo.bind_recovery_operation(novel_codex_story_core::RecoveryOperationBinding {
        project_id: novel_codex_story_core::ProjectId::parse(w["projectId"].as_str().unwrap())
            .unwrap(),
        task_id: w["id"].as_str().unwrap().into(),
        thread_id: w["threadId"].as_str().unwrap().into(),
        turn_id: "paused-turn".into(),
        call_id: "unknown-write".into(),
        tool_name: "save_draft_checkpoint".into(),
        request_hash: "0".repeat(64),
        idempotency_ref: Some(format!("sha256:{}", "1".repeat(64))),
        now: 1,
    })
    .unwrap();
    drop(repo);
    let grant = r
        .get(&format!(
            "{root}/tasks/{}/provider-budget",
            w["id"].as_str().unwrap()
        ))
        .await;
    let active = r.get(&uri).await;
    r.post(
        &format!("{uri}/control"),
        json!({"expectedVersion":active["version"],"command":"pause"}),
    )
    .await;
    r.settle(&uri, &w, &grant).await;
    let before = r.get(&uri).await;
    let (status, _) = r
        .call(
            "POST",
            &format!("{uri}/control"),
            AUTHOR,
            Some(json!({"expectedVersion":before["version"],"command":"resume"})),
        )
        .await;
    assert!(status.is_client_error());
    assert_eq!(r.get(&uri).await, before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsaved_current_chapter_replan_preserves_scope_and_budget() {
    unsaved_current_chapter_restart(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn current_chapter_rebind_preserves_frozen_acceptance_and_author_identity() {
    unsaved_current_chapter_restart(false).await;
}

async fn unsaved_current_chapter_restart(author_change: bool) {
    let mut r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let writer = r.writer(&root, &uri, "failed-empty-writer").await;
    let task_uri = format!("{root}/tasks/{}", writer["id"].as_str().unwrap());
    let grant = r.get(&format!("{task_uri}/provider-budget")).await;
    r.ok(
        "PATCH",
        &task_uri,
        AUTHOR,
        Some(json!({"threadId":writer["threadId"],"status":"failed","error":"TaskNotWritable"})),
    )
    .await;
    r.settle(&uri, &writer, &grant).await;
    let stopped = r.post(&format!("{uri}/next"), json!({})).await;
    let before = stopped["run"].clone();
    r.restart().await;
    let preview = r.get(&format!("{uri}/replan")).await;
    assert_eq!(preview["availableFrom"], json!(["currentChapter"]));
    let mut request = json!({"expectedVersion":preview["run"]["version"],"expectedProjectRevision":preview["projectRevision"],"expectedSourceDigest":preview["sourceDigest"],"restartFrom":"currentChapter","instruction":"重新起草未保存的当前章，保持原范围与预算"});
    let route = if author_change { "replan" } else { "rebind" };
    if !author_change {
        request.as_object_mut().unwrap().remove("instruction");
    }
    request["expectedSourceDigest"] = json!("0".repeat(64));
    assert!(
        r.call(
            "POST",
            &format!("{uri}/{route}"),
            AUTHOR,
            Some(request.clone())
        )
        .await
        .0
        .is_client_error()
    );
    assert_eq!(r.get(&uri).await, before);
    request["expectedSourceDigest"] = preview["sourceDigest"].clone();
    let replanned = r.post(&format!("{uri}/{route}"), request).await;
    assert_eq!(allowance(&replanned), allowance(&before));
    if !author_change {
        assert_eq!(replanned["intentGeneration"], before["intentGeneration"]);
        assert_eq!(replanned["intentRevisions"], before["intentRevisions"]);
    }
    assert_eq!(replanned["currentChapterId"], writer["target"]["chapterId"]);
    assert_eq!(r.get(&task_uri).await["error"], "TaskNotWritable");
    let next = r.writer(&root, &uri, "replanned-empty-writer").await;
    if !author_change {
        assert_eq!(
            next["target"]["chapterAcceptance"],
            writer["target"]["chapterAcceptance"]
        );
    }
    assert_eq!(
        next["inputManifest"]["expectedCandidateRevisionId"],
        Value::Null
    );
    assert_eq!(next["target"]["chapterId"], writer["target"]["chapterId"]);
    let saved = r
        .save_writer(&root, &next, BODY, "after-empty-replan")
        .await;
    r.complete_and_advance(&root, &uri, &next).await;
    let done = r.finish_chapter(&root, &uri).await;
    assert_eq!(done["status"], "completed");
    assert_eq!(
        done["chapterDeliveries"][0]["draftRevisionId"],
        saved["revisionId"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn duplicate_finish_is_rejected_and_terminal_absent_actions_are_reconciled() {
    use novel_codex_story_core::{
        ProjectId, RecoveryOperationBinding, RepositoryError, SqliteStoryRepository,
        WorkActionRequest,
    };
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let writer = r.writer(&root, &uri, "writer").await;
    r.save_writer(&root, &writer, BODY, "body").await;
    r.complete_and_advance(&root, &uri, &writer).await;
    r.assess_chapter(&root, &uri, "supported").await;
    let coordinator = r.next_task(&root, &uri, "coordinator").await;
    let read = r.decision_status(&root, &coordinator).await;
    let action = read["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == "finishChapter")
        .unwrap();
    r.choose(&root, &coordinator, "finishChapter").await;
    let project = ProjectId::parse(coordinator["projectId"].as_str().unwrap()).unwrap();
    let id = coordinator["id"].as_str().unwrap();
    let mut repo = SqliteStoryRepository::open(&r.path).unwrap();
    let task = repo.task(&project, id).unwrap().unwrap();
    let binding = |call: &str, tool: &str, key: Option<&str>| RecoveryOperationBinding {
        project_id: project.clone(),
        task_id: id.into(),
        thread_id: task.thread_id.clone(),
        turn_id: "turn".into(),
        call_id: call.into(),
        tool_name: tool.into(),
        request_hash: "a".repeat(64),
        idempotency_ref: key.map(|key| {
            format!(
                "sha256:{:x}",
                Sha256::digest(json!(key).to_string().as_bytes())
            )
        }),
        now: 1,
    };
    repo.bind_recovery_operation(binding(
        "duplicate",
        "request_work_action",
        Some("different-key"),
    ))
    .unwrap();
    let request: WorkActionRequest = serde_json::from_value(json!({"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],"actionKey":action["actionKey"],"instruction":"重复登记完稿","idempotencyKey":"different-key"})).unwrap();
    assert!(matches!(
        repo.select_work_action(&task, &request),
        Err(RepositoryError::TaskOutputForbidden)
    ));
    repo.bind_recovery_operation(binding(
        "absent",
        "request_work_action",
        Some("uncommitted-key"),
    ))
    .unwrap();
    repo.bind_recovery_operation(binding(
        "matching",
        "request_work_action",
        Some("select-finishChapter"),
    ))
    .unwrap();
    repo.bind_recovery_operation(binding("unkeyed", "request_work_action", None))
        .unwrap();
    repo.bind_recovery_operation(binding(
        "other-tool",
        "save_draft_checkpoint",
        Some("uncommitted-key"),
    ))
    .unwrap();
    let snapshot = repo
        .recovery_snapshot(&project, &[id.into()], None, 0, 100, 2)
        .unwrap();
    let state = |snapshot: &Value, call: &str| {
        snapshot["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["tool_call_id"] == call)
            .unwrap()["state"]
            .clone()
    };
    assert_eq!(state(&snapshot, "duplicate"), "rejected");
    assert_eq!(state(&snapshot, "absent"), "accepted");
    drop(repo);
    r.complete_and_advance(&root, &uri, &coordinator).await;
    let mut repo = SqliteStoryRepository::open(&r.path).unwrap();
    let snapshot = repo
        .recovery_snapshot(&project, &[id.into()], None, 0, 100, 3)
        .unwrap();
    assert_eq!(state(&snapshot, "absent"), "rejected");
    for call in ["matching", "unkeyed", "other-tool"] {
        assert_eq!(state(&snapshot, call), "accepted");
    }
    assert_eq!(r.get(&uri).await["completedChapters"], 1);
}


#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checkpoint_refresh_does_not_deliver_partial_or_new_intent_from_old_receipt() {
    for author_change in [false, true] {
        let r = Runtime::new().await;
        let (root, uri) = r.start().await;
        let writer = r.writer(&root, &uri, "refresh-writer").await;
        let mut payload = draft(writer["target"]["chapterId"].clone(), BODY);
        if !author_change { payload["completeness"] = json!("partial"); }
        r.candidate(&format!("{root}/drafts/checkpoints"), json!({
            "payload":payload,"expectedRevision":writer["inputRevision"],
            "expectedCandidateRevisionId":writer["inputManifest"]["expectedCandidateRevisionId"],
            "idempotencyKey":"saved-before-refresh","taskId":writer["id"],"threadId":writer["threadId"]
        })).await;
        let task_uri = format!("{root}/tasks/{}", writer["id"].as_str().unwrap());
        let grant = r.get(&format!("{task_uri}/provider-budget")).await;
        r.ok("PATCH", &task_uri, AUTHOR, Some(json!({"threadId":writer["threadId"],
            "status":"failed","error":"context_item_limit"}))).await;
        r.settle(&uri, &writer, &grant).await;
        r.post(&format!("{uri}/next"), json!({})).await;
        let preview = r.get(&format!("{uri}/replan")).await;
        let mut request = json!({"expectedVersion":preview["run"]["version"],
            "expectedProjectRevision":preview["projectRevision"],"expectedSourceDigest":preview["sourceDigest"],
            "restartFrom":"currentChapter"});
        let route = if author_change {
            request["instruction"] = json!("本章改为另一人物视角，保持其他要求");
            "replan"
        } else { "rebind" };
        let resumed = r.post(&format!("{uri}/{route}"), request).await;
        assert_eq!(resumed["providerBudget"], preview["run"]["providerBudget"]);
        assert_eq!(r.get(&task_uri).await["status"], "failed");
        let decision = r.next_task(&root, &uri, "refresh-decision").await;
        let status = r.decision_status(&root, &decision).await;
        let actions = status["readyActions"].as_array().unwrap();
        assert!(actions.iter().any(|a| a["operation"] == "generateChapter"));
        assert!(!actions.iter().any(|a| a["operation"] == "finishChapter"), "{status}");
    }
}

use novel_codex_story_core::{RecoveryOperationsRepository, RecoverySnapshotRepository, TasksRepository, WorkDecisionActionsRepository};
