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
    runtime
        .post(
            &format!("{root}/drafts/checkpoints"),
            json!({
                "payload":draft(json!("chapter-one"), BODY),"expectedRevision":0,
                "expectedCandidateRevisionId":null,"idempotencyKey":"saved-source"
            }),
        )
        .await;
    // Make the debounce due without a wall-clock sleep; all state transitions
    // below still go through production HTTP handlers and durable SQLite.
    rusqlite::Connection::open(&runtime.path)
        .unwrap()
        .execute(
            "UPDATE story_modeling_outbox_v1 SET first_dirty_ms=0,last_dirty_ms=0",
            [],
        )
        .unwrap();
    let action = runtime
        .post(&format!("{root}/modeling-outbox/dispatch"), json!({}))
        .await;
    let work_uri = format!(
        "{root}/work-tasks/{}",
        action["action"]["workTaskId"].as_str().unwrap()
    );
    let work = runtime.get(&work_uri).await;
    let mut target = work["steps"][0]["target"].clone();
    target["resumeStepId"] = work["steps"][0]["id"].clone();
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let workspace = runtime.get(&workspace_uri).await;
    runtime.post(&format!("{work_uri}/budget-authorization"), json!({
        "expectedVersion":work["version"],"expectedProjectRevision":0,"expectedSourceDigest":workspace["sourceDigest"],
        "maxRequests":3,"maxTokens":700,"deadlineAt":expires
    })).await;
    let authorization = runtime
        .get(&format!("{work_uri}/budget-authorization"))
        .await;
    let task = runtime.post(&format!("{root}/tasks"), json!({
        "operation":"modelKnowledge","target":target,"inputRevision":0,"threadId":"model-root",
        "authorIntent":"分析已保存章节的知识","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
        "capabilityExpiresAt":expires
    })).await;
    let task_uri = format!("{root}/tasks/{}", task["id"].as_str().unwrap());
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
                "command":"resume","expectedVersion":blocked["version"]
            }),
        )
        .await;
    assert_eq!(restored["status"], "completed");
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
    let next = runtime.advance_until(&root, &uri, "draft", BODY).await;
    let task = runtime.claim(&root, &next["action"], "paused-writer").await;
    let workspace_uri = format!("{root}/creative-workspace");
    let workspace = runtime.get(&workspace_uri).await;
    let mut request = json!({"payload":draft(task["target"]["chapterId"].clone(), BODY),
        "expectedRevision":workspace["revision"],"expectedCandidateRevisionId":null,
        "idempotencyKey":"before-pause","taskId":task["id"],"threadId":"paused-writer"});
    let saved = runtime
        .candidate(&format!("{root}/drafts/checkpoints"), request.clone())
        .await;
    let claimed = runtime.get(&uri).await;
    let paused = runtime
        .post(
            &format!("{uri}/control"),
            json!({"expectedVersion":claimed["version"],"command":"pause"}),
        )
        .await;
    assert_eq!(paused["status"], "paused");
    let before = runtime.get(&workspace_uri).await;
    request["expectedCandidateRevisionId"] = saved["revisionId"].clone();
    request["idempotencyKey"] = json!("late-paused-checkpoint");
    request["payload"] = draft(task["target"]["chapterId"].clone(), "林夕烧掉密函。");
    let write = runtime
        .call(
            "POST",
            &format!("{root}/drafts/checkpoints"),
            CANDIDATE,
            Some(request),
        )
        .await;
    let completion = runtime
        .call(
            "POST",
            &format!("{root}/tasks/{}/complete", task["id"].as_str().unwrap()),
            CANDIDATE,
            Some(json!({"threadId":"paused-writer","summary":"迟到交付"})),
        )
        .await;
    assert!(
        write.0.is_client_error() && completion.0.is_client_error(),
        "paused writes and completion must be rejected: checkpoint={}; completion={}",
        write.0,
        completion.0
    );
    assert_eq!(runtime.get(&workspace_uri).await, before);
    assert_eq!(runtime.get(&uri).await, paused);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paused_check_author_edit_replan_reopen_continue_and_cancel_preserve_lineage() {
    let mut runtime = Runtime::new().await;
    let (root, uri) = runtime.start().await;
    let next = runtime.advance_until(&root, &uri, "check", BODY).await;
    assert_eq!(next["action"]["kind"], "checkKnowledge");
    let old_check = json!({"expectedVersion":next["run"]["version"]});
    let tasks = runtime.get(&format!("{root}/creative-workspace")).await;
    let old_writer = tasks["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["operation"] == "generateChapter")
        .unwrap()
        .clone();
    let paused = runtime
        .post(
            &format!("{uri}/control"),
            json!({"expectedVersion":next["run"]["version"],"command":"pause"}),
        )
        .await;
    let work_uri = format!("{root}/work-tasks/work-{}", paused["id"].as_str().unwrap());
    let old_work = runtime.get(&work_uri).await;
    let workspace_uri = format!("{root}/creative-workspace");
    let workspace = runtime.get(&workspace_uri).await;
    let chapter = paused["currentChapterId"].clone();
    let edited = runtime.post(&format!("{root}/drafts/checkpoints"), json!({
        "payload":draft(chapter.clone(), "林夕看见灯光，藏好密函。"),"expectedRevision":workspace["revision"],
        "expectedCandidateRevisionId":workspace["chapters"][0]["candidate"]["id"],"idempotencyKey":"author-edited-checkpoint"
    })).await;
    let authored = runtime.get(&workspace_uri).await;
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
    assert_eq!(
        runtime
            .call(
                "POST",
                &format!("{uri}/check"),
                AUTHOR,
                Some(old_check.clone())
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(runtime.get(&uri).await, paused);
    let preview = runtime.get(&format!("{uri}/replan")).await;
    assert_eq!(preview["availableFrom"], json!(["currentChapter"]));
    let request = json!({"expectedVersion":preview["run"]["version"],"expectedProjectRevision":preview["projectRevision"],
        "expectedSourceDigest":preview["sourceDigest"],"restartFrom":"currentChapter","instruction":"保留藏信改稿，补全本章"});
    let replanned = runtime
        .post(&format!("{uri}/replan"), request.clone())
        .await;
    assert_eq!(allowance(&replanned), allowance(&paused));
    assert_eq!(replanned["providerBudget"]["usedRequests"], 7);
    assert_eq!(replanned["providerBudget"]["usedTokens"], 49);
    assert_eq!(replanned["currentChapterId"], chapter);
    assert_eq!(replanned["stage"], "draft");
    assert_eq!(replanned["intentGeneration"], 1);
    assert_eq!(replanned["sourceDigest"], authored["sourceDigest"]);
    assert_eq!(replanned["baseSnapshotId"], authored["headSnapshotId"]);
    assert_eq!(
        replanned["replans"],
        json!([{
            "previousVersion":paused["version"],"previousStage":"check","previousTaskId":null,"restartFrom":"currentChapter",
            "previousSourceDigest":paused["sourceDigest"],"previousSnapshotId":paused["baseSnapshotId"],
            "sourceDigest":authored["sourceDigest"],"baseSnapshotId":authored["headSnapshotId"],"intentGeneration":1,
            "createdAt":replanned["replans"][0]["createdAt"]
        }])
    );
    assert_eq!(runtime.get(&work_uri).await["steps"], old_work["steps"]);
    assert_eq!(runtime.get(&workspace_uri).await, authored);
    let repaused = runtime
        .post(
            &format!("{uri}/control"),
            json!({"expectedVersion":replanned["version"],"command":"pause"}),
        )
        .await;
    let work_before_restart = runtime.get(&work_uri).await;
    runtime.restart().await;
    assert_eq!(runtime.get(&uri).await, repaused);
    assert_eq!(runtime.get(&work_uri).await, work_before_restart);
    assert_eq!(runtime.get(&workspace_uri).await, authored);
    let resumed = runtime
        .post(
            &format!("{uri}/control"),
            json!({"expectedVersion":repaused["version"],"command":"resume"}),
        )
        .await;
    assert_eq!(allowance(&resumed), allowance(&paused));
    let fresh = runtime.post(&format!("{uri}/next"), json!({})).await;
    assert_eq!(fresh["action"]["target"]["intentGeneration"], 1);
    assert!(
        fresh["action"]["authorIntent"]
            .as_str()
            .unwrap()
            .contains("保留藏信改稿")
    );
    let writer = runtime
        .claim(&root, &fresh["action"], "continued-writer")
        .await;
    assert_eq!(
        writer["inputManifest"]["expectedCandidateRevisionId"],
        edited["revisionId"]
    );
    assert_eq!(
        writer["inputManifest"]["authorIntent"],
        fresh["action"]["authorIntent"]
    );
    let before_late = runtime.get(&uri).await;
    let before_late_workspace = runtime.get(&workspace_uri).await;
    assert_eq!(before_late["usedTaskAttempts"], 8);
    let mut output = json!({"payload":draft(chapter.clone(), "林夕藏好密函，循灯光走向白塔。"),
        "expectedRevision":authored["revision"],"expectedCandidateRevisionId":edited["revisionId"],
        "idempotencyKey":"continued-output","taskId":writer["id"],"threadId":"continued-writer"});
    let mut late = output.clone();
    late["taskId"] = old_writer["id"].clone();
    late["threadId"] = old_writer["threadId"].clone();
    late["idempotencyKey"] = json!("old-writer-output");
    assert!(
        runtime
            .call(
                "POST",
                &format!("{root}/drafts/checkpoints"),
                CANDIDATE,
                Some(late)
            )
            .await
            .0
            .is_client_error()
    );
    assert_eq!(
        runtime
            .call(
                "POST",
                &format!("{uri}/check"),
                AUTHOR,
                Some(old_check.clone())
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(runtime.get(&uri).await, before_late);
    assert_eq!(runtime.get(&workspace_uri).await, before_late_workspace);
    let saved = runtime
        .candidate(&format!("{root}/drafts/checkpoints"), output.clone())
        .await;
    let continued = runtime.get(&workspace_uri).await;
    assert_eq!(
        continued["chapters"][0]["candidate"]["body"],
        output["payload"]["body"]
    );
    assert_eq!(continued["chapters"][0]["accepted"], Value::Null);
    let grant = runtime
        .get(&format!(
            "{root}/tasks/{}/provider-budget",
            writer["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(grant["deadlineAt"], paused["deadlineAt"]);
    assert_eq!(grant["maxRequests"], 57);
    assert_eq!(grant["maxTokens"], 999_951);
    runtime
        .candidate(
            &format!("{root}/tasks/{}/complete", writer["id"].as_str().unwrap()),
            json!({"threadId":"continued-writer","summary":"作者改稿后的新意图已交付"}),
        )
        .await;
    let charged = runtime.settle(&uri, &writer, &grant).await;
    assert_eq!(charged["usedTaskAttempts"], 8);
    assert_eq!(charged["providerBudget"]["usedRequests"], 8);
    assert_eq!(charged["providerBudget"]["usedTokens"], 56);
    let advanced = runtime
        .post(
            &format!("{uri}/advance-task"),
            json!({"expectedVersion":charged["version"],"taskId":writer["id"]}),
        )
        .await;
    assert_eq!(advanced["stage"], "extract");
    let pending = runtime.post(&format!("{uri}/next"), json!({})).await;
    let mut cancelled_claim = pending["action"].clone();
    let batch = runtime
        .post(
            &format!("{root}/modeling-batches"),
            json!({
                "objectId":format!("draft-{}", chapter.as_str().unwrap()),"sourceRevisionId":saved["revisionId"],
                "expectedRevision":continued["revision"],"extractorVersion":"cancelled-test"
            }),
        )
        .await;
    cancelled_claim["target"]["batchId"] = batch["id"].clone();
    let charged = runtime
        .post(
            &format!("{uri}/control"),
            json!({"expectedVersion":advanced["version"],"command":"cancel"}),
        )
        .await;
    let cancelled_workspace = runtime.get(&workspace_uri).await;
    assert_eq!(charged["status"], "cancelled");
    assert_eq!(charged["usedTaskAttempts"], 8);
    assert_eq!(charged["providerBudget"]["usedRequests"], 8);
    assert_eq!(charged["providerBudget"]["usedTokens"], 56);
    runtime.restart().await;
    assert_eq!(runtime.get(&uri).await, charged);
    assert_eq!(
        runtime.post(&format!("{uri}/next"), json!({})).await,
        json!({"run":charged,"action":null})
    );
    assert!(runtime.call("POST", &format!("{root}/tasks"), AUTHOR, Some(json!({
        "operation":cancelled_claim["operation"],"target":cancelled_claim["target"],"inputRevision":continued["revision"],
        "threadId":"cancelled-next","authorIntent":cancelled_claim["authorIntent"],
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":charged["deadlineAt"]
    }))).await.0.is_client_error());
    for (endpoint, payload) in [
        (
            "control",
            json!({"expectedVersion":charged["version"],"command":"resume"}),
        ),
        (
            "replan",
            json!({"expectedVersion":charged["version"],"expectedProjectRevision":continued["revision"],
            "expectedSourceDigest":continued["sourceDigest"],"restartFrom":"currentChapter","instruction":"继续"}),
        ),
        ("check", old_check),
    ] {
        assert!(
            runtime
                .call("POST", &format!("{uri}/{endpoint}"), AUTHOR, Some(payload))
                .await
                .0
                .is_client_error()
        );
    }
    output["expectedCandidateRevisionId"] = saved["revisionId"].clone();
    output["idempotencyKey"] = json!("cancelled-output");
    assert!(
        runtime
            .call(
                "POST",
                &format!("{root}/drafts/checkpoints"),
                CANDIDATE,
                Some(output)
            )
            .await
            .0
            .is_client_error()
    );
    assert_eq!(runtime.get(&workspace_uri).await, cancelled_workspace);
    assert_eq!(runtime.get(&uri).await, charged);
    let final_work = runtime.get(&work_uri).await;
    let old_steps = old_work["steps"].as_array().unwrap();
    assert_eq!(
        &final_work["steps"].as_array().unwrap()[..old_steps.len()],
        old_steps
    );
    assert_eq!(final_work["status"], "cancelled");
    assert_eq!(
        final_work["steps"].as_array().unwrap().len(),
        old_steps.len() + 1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_active_attempt_can_settle_spent_usage_without_reopening_run() {
    let mut runtime = Runtime::new().await;
    let (root, uri) = runtime.start().await;
    let next = runtime.advance_until(&root, &uri, "draft", BODY).await;
    let writer = runtime
        .claim(&root, &next["action"], "cancelled-writer")
        .await;
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
    assert_eq!(charged["providerBudget"]["usedRequests"], 4);
    assert_eq!(charged["providerBudget"]["usedTokens"], 28);
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
        json!({"run":charged,"action":null})
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replan_after_author_edit_delivers_only_the_new_draft_check() {
    let mut runtime = Runtime::new().await;
    let (root, uri) = runtime.start().await;
    let old = runtime.advance_until(&root, &uri, "check", BODY).await;
    let paused = runtime
        .post(
            &format!("{uri}/control"),
            json!({"expectedVersion":old["run"]["version"],"command":"pause"}),
        )
        .await;
    let workspace_uri = format!("{root}/creative-workspace");
    let before = runtime.get(&workspace_uri).await;
    let body = "林夕看见灯光，藏好密函。";
    runtime.post(&format!("{root}/drafts/checkpoints"), json!({
        "payload":draft(paused["currentChapterId"].clone(), body),
        "expectedRevision":before["revision"],"expectedCandidateRevisionId":before["chapters"][0]["candidate"]["id"],
        "idempotencyKey":"edited-before-final-check"
    })).await;
    let preview = runtime.get(&format!("{uri}/replan")).await;
    let replanned = runtime.post(&format!("{uri}/replan"), json!({
        "expectedVersion":preview["run"]["version"],"expectedProjectRevision":preview["projectRevision"],
        "expectedSourceDigest":preview["sourceDigest"],"restartFrom":"currentChapter","instruction":"保留藏信，核验灯光事件"
    })).await;
    assert_eq!(allowance(&replanned), allowance(&paused));
    runtime.restart().await;
    let current = runtime.advance_until(&root, &uri, "check", body).await;
    let workspace = runtime.get(&workspace_uri).await;
    assert_eq!(workspace["chapters"][0]["candidate"]["body"], body);
    assert_ne!(
        workspace["chapters"][0]["candidate"]["id"],
        before["chapters"][0]["candidate"]["id"]
    );
    let checked = runtime
        .post(
            &format!("{uri}/check"),
            json!({"expectedVersion":current["run"]["version"]}),
        )
        .await;
    assert_eq!(checked["stage"], "finished");
    assert_eq!(checked["status"], "completed");
    assert_eq!(checked["completedChapters"], 1);
    assert_eq!(checked["intentGeneration"], 1);
    assert_eq!(checked["deadlineAt"], paused["deadlineAt"]);
    assert_eq!(checked["usedTaskAttempts"], 11);
    assert_eq!(
        checked["chapterDeliveries"][0]["draftRevisionId"],
        workspace["chapters"][0]["candidate"]["id"]
    );
    assert_eq!(checked["providerBudget"]["usedRequests"], 11);
    assert_eq!(checked["providerBudget"]["usedTokens"], 77);
    runtime.restart().await;
    assert_eq!(runtime.get(&uri).await, checked);
    assert_eq!(
        runtime.get(&workspace_uri).await["chapters"][0]["accepted"],
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
