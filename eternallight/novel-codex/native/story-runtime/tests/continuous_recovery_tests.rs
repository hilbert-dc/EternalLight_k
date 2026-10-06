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
        .post("/api/projects", json!({"title":"恢复已保存的知识"}))
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
            json!({
                "command":"recoverSavedResult","expectedVersion":blocked["version"]
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
                Some(json!({"command":"resume","expectedVersion":blocked["version"]}))
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
    assert_eq!(tasks.len(), workspace["tasks"].as_array().unwrap().len() + 2);
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
    let original: String = db.query_row("SELECT state_json FROM story_generation_runs_v1 WHERE run_id=?1",
        [run_id], |r| r.get(0)).unwrap();
    let task_count: i64 = db.query_row("SELECT COUNT(*) FROM story_tasks_v2", [], |r| r.get(0)).unwrap();
    for (field, value) in [("currentChapterId", json!("another-chapter")),
        ("currentChapterNumber", json!(2)), ("intentGeneration", json!(2))] {
        let mut corrupt: Value = serde_json::from_str(&original).unwrap();
        corrupt[field] = value;
        let encoded = corrupt.to_string();
        db.execute("UPDATE story_generation_runs_v1 SET state_json=?1 WHERE run_id=?2",
            rusqlite::params![encoded, run_id]).unwrap();
        let (status, response) = runtime.call("POST", &format!("{uri}/next"), AUTHOR, Some(json!({}))).await;
        assert!(status.is_client_error(), "{field}: {response}");
        assert_eq!(db.query_row("SELECT state_json FROM story_generation_runs_v1 WHERE run_id=?1",
            [run_id], |r| r.get::<_, String>(0)).unwrap(), encoded);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM story_tasks_v2", [], |r| r.get::<_, i64>(0)).unwrap(), task_count);
        db.execute("UPDATE story_generation_runs_v1 SET state_json=?1 WHERE run_id=?2",
            rusqlite::params![original, run_id]).unwrap();
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
    assert_eq!(completed["usedTaskAttempts"], 5);
    assert_eq!(completed["providerBudget"]["usedRequests"], 5);
    assert_eq!(completed["providerBudget"]["usedTokens"], 35);
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
