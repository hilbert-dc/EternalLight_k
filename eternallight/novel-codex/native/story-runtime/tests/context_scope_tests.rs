use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const AUTHOR_TOKEN: &str = "scope-author-token-123456789012345678";
const CANDIDATE_TOKEN: &str = "scope-candidate-token-123456789012345";
const CAPABILITY: &str = "scope-capability-123456789012345678901";

#[tokio::test]
async fn writer_relation_tool_is_bound_to_the_active_chapter_task() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("relation-scope.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"港城"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let (status, draft) = call(&path, "POST", &format!("/api/projects/{project_id}/drafts/checkpoints"),
        Some(json!({"payload":{"chapterNumber":1,"title":"白塔","body":"林夕走进港口。"},
            "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"relation-writer-draft"})), false).await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let chapter_id = draft["chapterId"].as_str().unwrap();
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, task) = call(&path, "POST", &format!("/api/projects/{project_id}/tasks"),
        Some(json!({"operation":"generateChapter","inputRevision":0,"threadId":"writer-relation-thread",
            "target":{"chapterId":chapter_id,"chapterNumber":1,"readingChapter":1,"sceneOrder":1,"povCharacterId":"linxi","storyTime":1},
            "capabilityHash":format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt":expires,"authorIntent":"续写"})), false).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let url = format!(
        "/api/projects/{project_id}/tasks/{task_id}/relations?threadId=writer-relation-thread&expectedRevision=0&entityId=linxi&direction=outgoing&worldTime=1&maxHops=2"
    );
    let (status, result) = call(&path, "GET", &url, None, true).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["scope"], "writerCharacterKnowledge");
    assert_eq!(result["visibleThroughChapter"], 0);
    assert_eq!(result["results"], json!([]));
    assert_eq!(result["seedFound"], json!(null));
    let (status, implicit_time) =
        call(&path, "GET", &url.replace("&worldTime=1", ""), None, true).await;
    assert_eq!(status, StatusCode::OK, "{implicit_time}");
    assert_eq!(implicit_time["worldTime"], 1);
    let (status, time_override) = call(
        &path,
        "GET",
        &url.replace("worldTime=1", "worldTime=2"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{time_override}");
    assert_eq!(
        time_override["error"]["code"],
        "pov_time_override_forbidden"
    );
    let override_request = Request::builder()
        .method("GET")
        .uri(format!("{url}&povCharacterId=other"))
        .header("authorization", format!("Bearer {CANDIDATE_TOKEN}"))
        .header("x-novel-task-capability", CAPABILITY)
        .body(Body::empty())
        .unwrap();
    let override_response = application(&path)
        .author_fixture_request(override_request)
        .await
        .unwrap();
    assert_eq!(override_response.status(), StatusCode::BAD_REQUEST);
    let (status, _) = call(&path, "GET", &url, None, false).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(
        &path,
        "GET",
        &format!("{url}&snapshotId=anything"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(
        &path,
        "GET",
        &url.replace("expectedRevision=0", "expectedRevision=1"),
        None,
        true,
    )
    .await;
    assert_ne!(status, StatusCode::OK);
}

#[tokio::test]
async fn author_controls_the_persisted_user_task_across_an_attempt() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("controls.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "任务控制"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, attempt) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(json!({
            "operation": "advisory", "inputRevision": 0, "threadId": "thread-control",
            "target": {"type":"project"}, "authorIntent":"分析人物",
            "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt": expires
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{attempt}");
    let attempt_id = attempt["id"].as_str().unwrap();
    let (status, bound) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks/{attempt_id}/turn"),
        Some(json!({"threadId":"thread-control","turnId":"turn-control"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{bound}");
    assert_eq!(bound["turnId"], "turn-control");
    let work_path = format!("/api/projects/{project_id}/tasks/{attempt_id}/work-task");
    let (status, work) = call(&path, "GET", &work_path, None, false).await;
    assert_eq!(status, StatusCode::OK, "{work}");
    assert_eq!(work["steps"][0]["attempts"][0]["id"], attempt_id);
    assert_eq!(work["steps"][0]["attempts"][0]["turnId"], "turn-control");
    let work_id = work["id"].as_str().unwrap();
    let control_path = format!("/api/projects/{project_id}/work-tasks/{work_id}/control");
    // An executor receives its grant while the attempt is active, before pause.
    let (status, grant) = call(&path, "GET",
        &format!("/api/projects/{project_id}/tasks/{attempt_id}/provider-budget"), None, false).await;
    assert_eq!(status, StatusCode::OK, "{grant}");
    let (status, paused) = call(
        &path,
        "POST",
        &control_path,
        Some(json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":work["threadId"],"command":"pause","expectedControlRevision":work["controlRevision"]})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{paused}");
    assert_eq!(paused["status"], "paused");
    let completion_path = format!("/api/projects/{project_id}/tasks/{attempt_id}/complete");
    let completion = json!({"threadId":"thread-control","summary":"人物分析"});
    let (status, rejected) = call(
        &path,
        "POST",
        &completion_path,
        Some(completion.clone()),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{rejected}");
    assert_eq!(rejected["error"]["code"], "task_capability_invalid");
    let (status, persisted) = call(&path, "GET", &work_path, None, false).await;
    assert_eq!(status, StatusCode::OK, "{persisted}");
    assert_eq!(persisted, paused);
    let attempt_path = format!("/api/projects/{project_id}/tasks/{attempt_id}");
    let (status, unchanged_attempt) = call(&path, "GET", &attempt_path, None, false).await;
    assert_eq!(status, StatusCode::OK, "{unchanged_attempt}");
    let mut paused_attempt = bound.clone();
    paused_attempt["status"] = json!("cancelled");
    paused_attempt["stage"] = json!("cancelled");
    paused_attempt["error"] = json!("author_paused");
    assert_eq!(unchanged_attempt, paused_attempt);
    let (status, stale) = call(
        &path,
        "POST",
        &control_path,
        Some(json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":work["threadId"],"command":"resume","expectedControlRevision":work["controlRevision"]})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale}");
    let (status, forbidden) = call(
        &path,
        "POST",
        &control_path,
        Some(json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":paused["threadId"],"command":"resume","expectedControlRevision":paused["controlRevision"]})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{forbidden}");
    assert_eq!(forbidden["error"]["code"], "unauthorized");
    let (status, persisted) = call(&path, "GET", &work_path, None, false).await;
    assert_eq!(status, StatusCode::OK, "{persisted}");
    assert_eq!(persisted, paused);
    // Pausing does not settle a real executor grant, even with zero requests.
    assert_eq!(paused["resumeBlockReason"], "pendingUsage");
    let (status, unsettled) = call(&path, "POST", &control_path,
        Some(json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":paused["threadId"],"command":"resume","expectedControlRevision":paused["controlRevision"]})), false).await;
    assert_eq!(status, StatusCode::CONFLICT, "{unsettled}");
    let mut engine = eternallight_engine_core::PersistentBudget::new(
        &path.with_extension("engine.sqlite3"), attempt_id,
        json!({"options":{"id":grant["budgetId"],"maxRequests":grant["maxRequests"],"maxTokens":grant["maxTokens"],"startedAt":1000},"now":1000})).unwrap();
    // The Host observes execution termination before settling its receipt.
    let (status, stopped) = call(&path, "PATCH", &attempt_path,
        Some(json!({"threadId":"thread-control","status":"failed","error":"execution_interrupted"})), false).await;
    assert_eq!(status, StatusCode::CONFLICT, "{stopped}");
    engine.complete("terminal").unwrap();
    let (status, settled) = call(&path, "POST",
        &format!("/api/projects/{project_id}/work-tasks/{work_id}/provider-usage"),
        Some(json!({"taskId":attempt_id})), false).await;
    assert_eq!(status, StatusCode::OK, "{settled}");
    let (status, paused) = call(&path, "GET", &work_path, None, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(paused["resumeBlockReason"].is_null(), "{paused}");
    let (status, resumed) = call(
        &path,
        "POST",
        &control_path,
        Some(json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":paused["threadId"],"command":"resume","expectedControlRevision":paused["controlRevision"]})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    assert_eq!(resumed["status"], "running");
    assert_eq!(resumed["steps"][0]["status"], "queued");
    assert!(resumed["steps"][0]["currentAttemptId"].is_null());
    assert_eq!(resumed["steps"][0]["attempts"][0]["id"], attempt_id);
    // Resume queues fresh execution; it must not resurrect the stopped capability.
    let (status, rejected) = call(&path, "POST", &completion_path, Some(completion), true).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{rejected}");
    let (status, persisted_attempt) = call(&path, "GET", &attempt_path, None, false).await;
    assert_eq!(status, StatusCode::OK, "{persisted_attempt}");
    assert_eq!(persisted_attempt, paused_attempt);
    let (status, finished) = call(&path, "GET", &work_path, None, false).await;
    assert_eq!(status, StatusCode::OK, "{finished}");
    assert_eq!(finished, resumed);
    assert_eq!(
        finished["steps"][0]["attempts"].as_array().unwrap().len(),
        1
    );
    let (status, stale) = call(
        &path,
        "POST",
        &control_path,
        Some(json!({"commandId":uuid::Uuid::new_v4().to_string(),"threadId":paused["threadId"],"command":"pause","expectedControlRevision":paused["controlRevision"]})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale}");
    let (status, persisted) = call(&path, "GET", &work_path, None, false).await;
    assert_eq!(status, StatusCode::OK, "{persisted}");
    assert_eq!(persisted, finished);
}

#[tokio::test]
async fn completed_chapter_task_exposes_its_pending_knowledge_step() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("followup.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "港城"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let draft_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let (status, initial) = call(&path, "POST", &draft_uri, Some(json!({
        "payload": {"chapterNumber": 1, "title": "白塔", "body": "林夕看见白塔。"},
        "expectedRevision": 0, "expectedCandidateRevisionId": null, "idempotencyKey": "first-draft"
    })), false).await;
    assert_eq!(status, StatusCode::CREATED, "{initial}");
    let chapter_id = initial["chapterId"].as_str().unwrap();
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, task) = call(&path, "POST", &format!("/api/projects/{project_id}/tasks"), Some(json!({
        "operation": "generateChapter", "inputRevision": 0, "threadId": "writer-thread",
        "target": {"chapterId": chapter_id, "chapterNumber": 1, "readingChapter": 1, "sceneOrder": 1},
        "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
        "capabilityExpiresAt": expires, "authorIntent": "修改白塔章节"
    })), false).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let (status, saved) = call(&path, "POST", &draft_uri, Some(json!({
        "payload": {"chapterId": chapter_id, "chapterNumber": 1, "title": "白塔", "body": "林夕记录白塔亮灯。"},
        "expectedRevision": 0, "expectedCandidateRevisionId": initial["revisionId"],
        "idempotencyKey": "writer-draft", "taskId": task_id, "threadId": "writer-thread",
        "completeness": "chapterComplete"
    })), true).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let (status, completed) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks/{task_id}/complete"),
        Some(json!({"threadId": "writer-thread", "summary": "已修改章节"})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let (status, resumable) = call(
        &path,
        "GET",
        &format!("/api/projects/{project_id}/chapters/{chapter_id}/next-step"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        resumable,
        json!({"action":{"kind":"modelKnowledge","chapterId":chapter_id,
        "sourceRevisionId":saved["revisionId"],"sourceTaskId":task_id}})
    );
}

#[tokio::test]
async fn scene_scoped_writer_reads_and_changes_only_the_visible_prefix() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scene-scope.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "港城"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let (status, draft) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/drafts/checkpoints"),
        Some(json!({
            "payload": {
                "chapterNumber": 1, "title": "白塔",
                "body": "林夕发现密函。\n\n---\n\n张衡就是叛徒。",
                "sceneGroups": [
                    {"id": "scene-first", "sceneId": "scene-one", "body": "林夕发现密函。", "locked": false},
                    {"id": "scene-second", "sceneId": "scene-two", "body": "张衡就是叛徒。", "locked": false}
                ]
            },
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "scene-scope-original"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let chapter_id = draft["chapterId"].as_str().unwrap();
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, task) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(json!({
            "operation": "generateChapter", "inputRevision": 0,
            "threadId": "scene-writer-thread",
            "target": {"type": "chapter", "chapterId": chapter_id,
                "chapterNumber": 1, "readingChapter": 1, "sceneOrder": 1,
                "scope": {"type": "throughScene", "endSceneOrder": 1}, "readScope": {"type": "throughScene", "endSceneOrder": 1}},
            "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt": expires, "authorIntent": "修改第一场景"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let context_uri = format!(
        "/api/projects/{project_id}/context?taskId={task_id}&threadId=scene-writer-thread&projectRevision=0&purpose=writer&readingChapter=1&sceneOrder=1&budget=24&byteBudget=60000"
    );
    let (status, context) = call(&path, "GET", &context_uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let root = format!("/api/projects/{project_id}");
    let frozen_text = read_section(&path, &root, &task, &context, "/relevantText").await;
    assert_eq!(
        frozen_text[0]["content"]["sceneGroups"][0]["body"],
        "林夕发现密函。"
    );
    assert_eq!(frozen_text[0]["sourceRevisionId"], draft["revisionId"]);
    assert_eq!(
        frozen_text[0]["content"]["sceneGroups"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(!frozen_text.to_string().contains("叛徒"));
    assert!(!context.to_string().contains("叛徒"));
    assert_model_item_bound(&context);
    let search_uri = format!(
        "/api/projects/{project_id}/tasks/{task_id}/search?threadId=scene-writer-thread&expectedRevision=0&limit=5"
    );
    let (status, hidden) = call(
        &path,
        "GET",
        &format!("{search_uri}&q=%E5%8F%9B%E5%BE%92"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{hidden}");
    assert_eq!(hidden["results"], json!([]));
    assert_eq!(hidden["coverage"]["scope"]["type"], "throughScene");
    let (status, found) = call(
        &path,
        "GET",
        &format!("{search_uri}&q=%E5%AF%86%E5%87%BD"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["results"][0]["evidence"]["quote"], "密函");
    let (status, rejected) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/drafts/checkpoints"),
        Some(json!({
            "payload": {"chapterId": chapter_id, "chapterNumber": 1, "title": "白塔",
                "body": "林夕打开密函。\n\n---\n\n张衡无罪。",
                "sceneGroups": [
                    {"id": "scene-first", "sceneId": "scene-one", "body": "林夕打开密函。", "locked": false},
                    {"id": "scene-second", "sceneId": "scene-two", "body": "张衡无罪。", "locked": false}]},
            "expectedRevision": 0, "expectedCandidateRevisionId": draft["revisionId"],
            "idempotencyKey": "scene-scope-illegal", "taskId": task_id,
            "threadId": "scene-writer-thread"
        })),
        true,
    )
    .await;
    assert_ne!(status, StatusCode::CREATED, "{rejected}");
    let (status, saved) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/drafts/checkpoints"),
        Some(json!({
            "payload": {"chapterId": chapter_id, "chapterNumber": 1, "title": "白塔",
                "body": "林夕打开密函。",
                "sceneGroups": [{"id": "scene-first", "sceneId": "scene-one", "body": "林夕打开密函。", "locked": false}]},
            "expectedRevision": 0, "expectedCandidateRevisionId": draft["revisionId"],
            "idempotencyKey": "scene-scope-valid", "taskId": task_id,
            "threadId": "scene-writer-thread"
        })),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(saved["body"], "林夕打开密函。");
    assert_eq!(saved["sceneGroups"].as_array().unwrap().len(), 1);
    assert!(!saved.to_string().contains("叛徒"));
    let (status, author_workspace) = call(
        &path,
        "GET",
        &format!("/api/projects/{project_id}/creative-workspace"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{author_workspace}");
    assert_eq!(
        author_workspace["chapters"][0]["candidate"]["body"],
        "林夕打开密函。\n\n---\n\n张衡就是叛徒。"
    );
    let (status, current) = call(&path, "GET", &context_uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert_eq!(
        current["relevantText"]["sourceId"],
        context["relevantText"]["sourceId"]
    );
    let current_input = read_section(&path, &root, &task, &current, "/relevantText").await;
    assert_eq!(current_input, frozen_text);
    let saved_source = read_saved_checkpoint(&path, &root, &task, &current, &saved, 1, true).await;
    assert_eq!(saved_source["textRepresentation"], "sceneGroups");
    assert!(saved_source["content"].get("body").is_none());
    assert_eq!(saved_source["content"]["sceneGroups"][0]["body"], "林夕打开密函。");
    assert_eq!(
        saved_source["content"]["sceneGroups"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(!saved_source.to_string().contains("叛徒"));
    assert!(!current.to_string().contains("叛徒"));
}

#[tokio::test]
async fn writer_search_cannot_read_future_chapters_or_planning_text() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("writer-search-scope.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "港城"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let mut first_chapter_id = String::new();
    let mut second_chapter_id = String::new();
    for (number, body) in [(1, "林夕发现密函。"), (2, "未来章节揭示叛徒。")] {
        let (status, draft) = call(
            &path,
            "POST",
            &format!("/api/projects/{project_id}/drafts/checkpoints"),
            Some(json!({
                "payload": {"chapterNumber": number, "title": if number == 1 { "密函谜团".to_owned() } else { format!("第 {number} 章") }, "body": body},
                "expectedRevision": 0, "expectedCandidateRevisionId": null,
                "idempotencyKey": format!("writer-search-chapter-{number}")
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{draft}");
        if number == 1 {
            first_chapter_id = draft["chapterId"].as_str().unwrap().to_owned();
        } else {
            second_chapter_id = draft["chapterId"].as_str().unwrap().to_owned();
        }
    }
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, task) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(json!({
            "operation": "generateChapter", "inputRevision": 0,
            "threadId": "writer-search-thread",
            "target": {"type": "chapter", "chapterId": first_chapter_id,
                "chapterNumber": 1, "readingChapter": 1, "sceneOrder": 1},
            "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt": expires, "authorIntent": "续写第一章"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let writer_search = format!(
        "/api/projects/{project_id}/tasks/{task_id}/search?threadId=writer-search-thread&expectedRevision=0&limit=5"
    );
    let (status, own_evidence) = call(
        &path,
        "GET",
        &format!("{writer_search}&q=%E5%AF%86%E5%87%BD"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{own_evidence}");
    assert_eq!(own_evidence["results"][0]["evidence"]["quote"], "密函");
    let (status, future) = call(
        &path,
        "GET",
        &format!("{writer_search}&q=%E5%8F%9B%E5%BE%92"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{future}");
    assert_eq!(future["results"], json!([]));
    assert_eq!(future["coverage"]["scannedCandidates"], 0);
    assert_eq!(future["coverage"]["scope"]["type"], "throughChapter");
    assert_eq!(future["coverage"]["scope"]["chapterNumber"], 1);
    let (status, author) = call(
        &path,
        "GET",
        &format!("/api/projects/{project_id}/search?q=%E5%8F%9B%E5%BE%92&limit=5"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{author}");
    assert_eq!(author["results"][0]["evidence"]["quote"], "叛徒");
    let (status, author_page) = call(
        &path,
        "GET",
        &format!("/api/projects/{project_id}/search?q=%E5%AF%86%E5%87%BD&limit=1"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{author_page}");
    let cursor = author_page["nextCursor"].as_str().unwrap();
    let (status, _) = call(
        &path,
        "GET",
        &format!("{writer_search}&q=%E5%AF%86%E5%87%BD&cursor={cursor}"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, pov_task) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(json!({
            "operation": "generateChapter", "inputRevision": 0,
            "threadId": "writer-pov-search-thread",
            "target": {"type": "chapter", "chapterId": second_chapter_id,
                "chapterNumber": 2, "readingChapter": 2, "sceneOrder": 1,
                "storyTime": 10, "povCharacterId": "linxi"},
            "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt": expires, "authorIntent": "从林夕视角续写"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{pov_task}");
    let pov_task_id = pov_task["id"].as_str().unwrap();
    let pov_search = format!(
        "/api/projects/{project_id}/tasks/{pov_task_id}/search?threadId=writer-pov-search-thread&expectedRevision=0&limit=5"
    );
    let (status, withheld) = call(
        &path,
        "GET",
        &format!("{pov_search}&q=%E5%AF%86%E5%87%BD"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{withheld}");
    assert_eq!(withheld["results"], json!([]));
    assert_eq!(withheld["coverage"]["complete"], false);
    assert_eq!(withheld["coverage"]["povTextUnverified"], true);
    let (status, pov_context) = call(
        &path,
        "GET",
        &format!(
            "/api/projects/{project_id}/context?taskId={pov_task_id}&threadId=writer-pov-search-thread&projectRevision=0&purpose=writer&readingChapter=2&sceneOrder=1&storyTime=10&povCharacterId=linxi&budget=24&byteBudget=60000"
        ),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pov_context}");
    let root = format!("/api/projects/{project_id}");
    let pov_text = read_section(&path, &root, &pov_task, &pov_context, "/relevantText").await;
    assert_eq!(pov_text.as_array().unwrap().len(), 1);
    assert_eq!(pov_text[0]["content"]["chapterNumber"], 2);
    assert_eq!(
        projected_scene_body(&pov_text[0]["content"]),
        "未来章节揭示叛徒。"
    );
    assert_eq!(
        pov_context["coverage"]["scannedScope"]["chaptersInScope"],
        1
    );
    assert_eq!(
        pov_context["coverage"]["scannedScope"]["modelingSourcesInScope"],
        2
    );
    let knowledge = read_section(
        &path,
        &root,
        &pov_task,
        &pov_context,
        "/operationContext/workingKnowledge",
    )
    .await;
    assert_eq!(knowledge["perspective"], "character");
    assert_eq!(pov_context["stateFacts"], json!([]));
    assert_eq!(pov_context["coverage"]["complete"], false);
    assert!(
        pov_context["coverage"]["missingScope"]
            .as_array()
            .unwrap()
            .contains(&json!("povPriorText"))
    );
    let (status, _) = call(
        &path,
        "GET",
        &format!("{pov_search}&q=%E5%AF%86%E5%87%BD&cursor={cursor}"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn story_search_is_bound_to_the_live_task_and_returns_source_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("search.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "港城"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let (status, draft) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/drafts/checkpoints"),
        Some(json!({"payload": {"chapterNumber": 1, "title": "灯塔", "body": "林夕在白塔下发现密函。"},
            "expectedRevision": 0, "expectedCandidateRevisionId": null, "idempotencyKey": "search-test-draft"})),
        false,
    ).await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, task) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(
            json!({"operation": "advisory", "inputRevision": 0, "threadId": "story-search-thread",
            "target": {"type": "project"},
            "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt": expires, "authorIntent": "分析密函"}),
        ),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let search_uri = format!(
        "/api/projects/{project_id}/tasks/{task_id}/search?threadId=story-search-thread&expectedRevision=0&q=%E5%AF%86%E5%87%BD&limit=5"
    );
    let (status, result) = call(&path, "GET", &search_uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["results"][0]["evidence"]["quote"], "密函");
    assert_eq!(
        result["results"][0]["sourceRevisionId"],
        draft["revisionId"]
    );
    let (status, _) = call(
        &path,
        "GET",
        &format!("{search_uri}&snapshotId=snapshot-before-task"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call(
        &path,
        "GET",
        &search_uri.replace("story-search-thread", "wrong-thread"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let actions_uri = format!("/api/projects/{project_id}/tasks/{task_id}/actions");
    let acceptance = chapter_acceptance(&task);
    let action_request = json!({
        "taskId": task_id, "threadId": "story-search-thread", "expectedRevision": 0, "expectedIntentRevision": 0,
        "idempotencyKey": "revise-chapter-one", "operation": "generateChapter",
        "target": {"chapterId": draft["chapterId"], "chapterNumber": 1,
            "povCharacterId": "linxi", "storyTime": 10, "chapterAcceptance":acceptance},
        "instruction": "让林夕主动打开密函"
    });
    let (status, action) = call(
        &path,
        "POST",
        &actions_uri,
        Some(action_request.clone()),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    assert_eq!(action["status"], "proposed");
    assert_eq!(action["target"]["povCharacterId"], "linxi");
    assert_eq!(action["target"]["storyTime"], 10);
    let (status, replay) = call(&path, "POST", &actions_uri, Some(action_request), true).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(replay, action);
    let (status, listed) = call(&path, "GET", &actions_uri, None, false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["actions"][0], action);
    let (status, unrelated) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(
            json!({"operation": "advisory", "inputRevision": 0, "threadId": "other-thread",
            "target": {"type": "project"},
            "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt": expires, "authorIntent": "无关讨论"}),
        ),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{unrelated}");
    let unrelated_id = unrelated["id"].as_str().unwrap();
    let (status, _) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks/{unrelated_id}/complete"),
        Some(json!({"threadId": "other-thread", "summary": "已完成"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        &path,
        "POST",
        &format!("{actions_uri}/{}/finish", action["id"].as_str().unwrap()),
        Some(json!({"resultTaskId": unrelated_id})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, unchanged) = call(&path, "GET", &search_uri, None, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        unchanged["results"][0]["sourceRevisionId"],
        draft["revisionId"]
    );
    let (status, _) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks/{task_id}/cancel"),
        Some(json!({})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(&path, "GET", &search_uri, None, true).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn conversation_writer_actions_freeze_bound_plan_position_before_child_claim() {
    for selected_scene in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("conversation-writer.sqlite3");
        let (_, created) = call(
            &path,
            "POST",
            "/api/projects",
            Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"港城"})),
            false,
        )
        .await;
        let project_id = created["project"]["id"].as_str().unwrap();
        let root = format!("/api/projects/{project_id}");
        let drafts_uri = format!("{root}/drafts/checkpoints");
        let (status, prior) = call(
            &path,
            "POST",
            &drafts_uri,
            Some(json!({
            "payload":{"chapterNumber":1,"title":"白塔","body":"林夕不知道密函中的名字。"},
            "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"prior"})),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{prior}");
        let (status, current) = call(&path, "POST", &drafts_uri, Some(json!({
            "payload":{"chapterNumber":2,"title":"藏信","body":"林夕把密函藏入衣袖。",
                "sceneGroups":[{"id":"letter","sceneId":null,"body":"林夕把密函藏入衣袖。","locked":false}]},
            "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"current"})), false).await;
        assert_eq!(status, StatusCode::CREATED, "{current}");
        let chapter_id = current["chapterId"].as_str().unwrap();
        let plan = json!({"payload":{"chapters":[{"chapterPlanId":"planned-two","chapterNumber":2,
            "publicTitle":"藏信","povCharacterId":"linxi","storyTime":10}]}});
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute(
            "INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) VALUES ('conversation-plan',?1,'artifact-plan','plan','fixture',?2)",
            rusqlite::params![project_id,plan.to_string()],
        ).unwrap();
        connection.execute(
            "INSERT INTO story_chapter_plan_bindings_v2(project_id,plan_revision_id,chapter_plan_id,chapter_id,created_at) VALUES (?1,'conversation-plan','planned-two',?2,1)",
            rusqlite::params![project_id,chapter_id],
        ).unwrap();
        let tasks_uri = format!("{root}/tasks");
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 600;
        let common = json!({"inputRevision":0,"threadId":"conversation-writer","authorIntent":"只改密函一节",
            "capabilityHash":format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires});
        let mut source_request = common.clone();
        source_request["operation"] = json!("advisory");
        source_request["target"] = json!({"type":"project"});
        let (status, source) = call(&path, "POST", &tasks_uri, Some(source_request), false).await;
        assert_eq!(status, StatusCode::CREATED, "{source}");
        let source_id = source["id"].as_str().unwrap();
        let actions_uri = format!("{tasks_uri}/{source_id}/actions");
        let scope = if selected_scene {
            json!({"type":"selectedScenes","sourceRevisionId":current["revisionId"],"groupIds":["letter"]})
        } else {
            json!({"type":"wholeChapter"})
        };
        let acceptance = chapter_acceptance(&source);
        let request = json!({"taskId":source_id,"threadId":"conversation-writer","expectedRevision":0,"expectedIntentRevision":0,
            "idempotencyKey":"modify-chapter","operation":"generateChapter",
            "target":{"chapterId":chapter_id,"chapterNumber":2,"scope":scope,"chapterAcceptance":acceptance},"instruction":"让林夕藏起密函"});
        let (status, action) = call(&path, "POST", &actions_uri, Some(request.clone()), true).await;
        assert_eq!(status, StatusCode::CREATED, "{action}");
        assert_eq!(
            action["target"],
            json!({"type":"chapter","chapterId":chapter_id,"chapterNumber":2,
            "readingChapter":2,"sceneOrder":1,"scope":scope,"povCharacterId":"linxi","storyTime":10,"chapterAcceptance":acceptance})
        );
        assert_eq!(
            call(&path, "POST", &actions_uri, Some(request.clone()), true).await,
            (StatusCode::CREATED, action.clone())
        );
        let changed_plan = json!({"payload":{"chapters":[{"chapterPlanId":"planned-two",
            "chapterNumber":2,"publicTitle":"另一种计划","povCharacterId":"other","storyTime":11}]}});
        connection.execute(
            "INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) VALUES ('conversation-plan-changed',?1,'artifact-plan','plan','fixture-changed',?2)",
            rusqlite::params![project_id,changed_plan.to_string()],
        ).unwrap();
        connection.execute(
            "UPDATE story_chapter_plan_bindings_v2 SET plan_revision_id='conversation-plan-changed' WHERE project_id=?1 AND chapter_id=?2",
            rusqlite::params![project_id,chapter_id],
        ).unwrap();
        assert_eq!(
            call(&path, "POST", &actions_uri, Some(request.clone()), true).await,
            (StatusCode::CREATED, action.clone())
        );
        let mut changed_request = request.clone();
        changed_request["instruction"] = json!("改成另一个动作");
        let (status, changed) =
            call(&path, "POST", &actions_uri, Some(changed_request), true).await;
        assert!(status.is_client_error(), "{changed}");
        connection.execute(
            "UPDATE story_chapter_plan_bindings_v2 SET plan_revision_id='conversation-plan' WHERE project_id=?1 AND chapter_id=?2",
            rusqlite::params![project_id,chapter_id],
        ).unwrap();
        let (_, before) = call(
            &path,
            "GET",
            &format!("{tasks_uri}/{source_id}/work-task"),
            None,
            false,
        )
        .await;
        for (field, value) in [("povCharacterId", json!("other")), ("storyTime", json!(11))] {
            let mut invalid = request.clone();
            invalid["idempotencyKey"] = json!(format!("invalid-{field}"));
            invalid["target"][field] = value;
            let (status, rejected) = call(&path, "POST", &actions_uri, Some(invalid), true).await;
            assert!(status.is_client_error(), "{rejected}");
            assert_eq!(
                call(&path, "GET", &actions_uri, None, false).await.1["actions"],
                json!([action])
            );
            assert_eq!(
                call(
                    &path,
                    "GET",
                    &format!("{tasks_uri}/{source_id}/work-task"),
                    None,
                    false
                )
                .await
                .1,
                before
            );
        }
        let (status, completed) = call(
            &path,
            "POST",
            &format!("{tasks_uri}/{source_id}/complete"),
            Some(json!({"threadId":"conversation-writer","summary":"已登记改稿任务"})),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{completed}");
        settle_fixture::settle(&path, project_id, source_id);
        let (status, approved) = call(
            &path,
            "POST",
            &format!("{actions_uri}/{}/approve", action["id"].as_str().unwrap()),
            Some(json!({"expectedIntentRevision":0})),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{approved}");
        let mut child = common;
        child["operation"] = json!("generateChapter");
        child["target"] = action["target"].clone();
        child["target"]["requestedByTaskId"] = json!(source_id);
        child["target"]["requestedActionId"] = action["id"].clone();
        for field in ["povCharacterId", "storyTime"] {
            let mut stripped = child.clone();
            stripped["target"].as_object_mut().unwrap().remove(field);
            let (status, rejected) = call(&path, "POST", &tasks_uri, Some(stripped), false).await;
            assert!(status.is_client_error(), "{rejected}");
        }
        let (status, writer) = call(&path, "POST", &tasks_uri, Some(child), false).await;
        assert_eq!(status, StatusCode::CREATED, "{writer}");
        let context_uri = format!(
            "{root}/context?taskId={}&threadId=conversation-writer&projectRevision=0&purpose=writer&readingChapter=2&sceneOrder=1&povCharacterId=linxi&storyTime=10&budget=24&byteBudget=60000",
            writer["id"].as_str().unwrap()
        );
        let (status, context) = call(&path, "GET", &context_uri, None, true).await;
        assert_eq!(status, StatusCode::OK, "{context}");
        assert_eq!(
            context["target"]["position"],
            json!({"readingChapter":2,"sceneOrder":1,"povCharacterId":"linxi","storyTime":10})
        );
        let planned = read_section(&path, &root, &writer, &context, "/plannedEffects").await;
        assert_eq!(planned[0]["value"]["planRevisionId"], "conversation-plan");
        assert_eq!(planned[0]["value"]["chapters"], plan["payload"]["chapters"]);
        let text = read_section(&path, &root, &writer, &context, "/relevantText").await;
        assert_eq!(text.as_array().unwrap().len(), 1);
        assert_eq!(text[0]["content"]["chapterNumber"], 2);
        assert_eq!(text[0]["sourceRevisionId"], current["revisionId"]);
        assert_eq!(
            projected_scene_body(&text[0]["content"]),
            "林夕把密函藏入衣袖。"
        );
        assert!(
            context["coverage"]["missingScope"]
                .as_array()
                .unwrap()
                .contains(&json!("povPriorText"))
        );
        let (status, implicit) = call(
            &path,
            "GET",
            &context_uri.replace("&povCharacterId=linxi", ""),
            None,
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{implicit}");
        assert_eq!(
            implicit["target"]["position"],
            context["target"]["position"]
        );
        assert_eq!(implicit["queryScope"], context["queryScope"]);
        assert_eq!(implicit["relevantText"], context["relevantText"]);
        assert_eq!(
            read_section(&path, &root, &writer, &implicit, "/relevantText").await,
            text
        );
        for denied_uri in [
            context_uri.replace("povCharacterId=linxi", "povCharacterId=other"),
            context_uri.replace("storyTime=10", "storyTime=11"),
        ] {
            let (status, denied) = call(&path, "GET", &denied_uri, None, true).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{denied}");
            assert_eq!(denied["error"]["code"], "writer_scope_mismatch");
        }
    }
}

#[tokio::test]
async fn author_can_decline_a_model_proposal_without_creating_candidate_work() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("declined-action.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"拒绝建议"})),
        false,
    )
    .await;
    let project = created["project"]["id"].as_str().unwrap();
    let tasks_uri = format!("/api/projects/{project}/tasks");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let source_request = json!({"operation":"advisory","inputRevision":0,"threadId":"decline-thread",
        "target":{"type":"project"},"authorIntent":"只讨论，不修改",
        "capabilityHash":format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires});
    let (status, source) = call(&path, "POST", &tasks_uri, Some(source_request), false).await;
    assert_eq!(status, StatusCode::CREATED, "{source}");
    let source_id = source["id"].as_str().unwrap();
    let actions_uri = format!("{tasks_uri}/{source_id}/actions");
    let (status, action) = call(&path, "POST", &actions_uri, Some(json!({
        "taskId":source_id,"threadId":"decline-thread","expectedRevision":0,"expectedIntentRevision":0,
        "idempotencyKey":"decline-request","operation":"generateBrief","target":{},"instruction":"生成简报"
    })), true).await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    let action_id = action["id"].as_str().unwrap();
    let (status, completed) = call(
        &path,
        "POST",
        &format!("{tasks_uri}/{source_id}/complete"),
        Some(json!({"threadId":"decline-thread","summary":"只讨论完成"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let decline_uri = format!("{actions_uri}/{action_id}/decline");
    let decision = json!({"expectedIntentRevision":0});
    let (status, _) = call(&path, "POST", &decline_uri, Some(decision.clone()), true).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, declined) = call(&path, "POST", &decline_uri, Some(decision.clone()), false).await;
    assert_eq!(status, StatusCode::OK, "{declined}");
    assert_eq!(declined["status"], "declined");
    assert_eq!(
        call(&path, "POST", &decline_uri, Some(decision.clone()), false)
            .await
            .1,
        declined
    );
    let (status, proposals) = call(
        &path,
        "GET",
        &format!("/api/projects/{project}/task-actions/proposed"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{proposals}");
    assert_eq!(proposals["actions"], json!([]));
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{actions_uri}/{action_id}/approve"),
            Some(decision),
            false
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (_, work) = call(
        &path,
        "GET",
        &format!("{tasks_uri}/{source_id}/work-task"),
        None,
        false,
    )
    .await;
    assert_eq!(work["steps"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn candidate_action_claim_and_completion_use_the_persisted_domain_binding() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("action-binding.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"任务绑定"})),
        false,
    )
    .await;
    let project = created["project"]["id"].as_str().unwrap();
    let tasks_uri = format!("/api/projects/{project}/tasks");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let common = json!({"inputRevision":0, "threadId":"binding-thread", "authorIntent":"生成简报",
        "capabilityHash":format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())), "capabilityExpiresAt":expires});
    let mut source_request = common.clone();
    source_request["operation"] = json!("advisory");
    source_request["target"] = json!({"type":"project"});
    let (status, source) = call(&path, "POST", &tasks_uri, Some(source_request), false).await;
    assert_eq!(status, StatusCode::CREATED, "{source}");
    let source_id = source["id"].as_str().unwrap();
    let actions_uri = format!("{tasks_uri}/{source_id}/actions");
    let (status, action) = call(&path, "POST", &actions_uri, Some(json!({
        "taskId":source_id, "threadId":"binding-thread", "expectedRevision":0, "expectedIntentRevision":0,
        "idempotencyKey":"binding-request", "operation":"generateBrief", "target":{}, "instruction":"生成简报"
    })), true).await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    let action_id = action["id"].as_str().unwrap();
    let (status, completed) = call(
        &path,
        "POST",
        &format!("{tasks_uri}/{source_id}/complete"),
        Some(json!({"threadId":"binding-thread", "summary":"已规划"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    settle_fixture::settle(&path, project, source_id);
    let proposed_uri = format!("/api/projects/{project}/task-actions/proposed");
    let (status, proposals) = call(&path, "GET", &proposed_uri, None, false).await;
    assert_eq!(status, StatusCode::OK, "{proposals}");
    assert_eq!(proposals["actions"][0]["id"], action_id);
    assert_eq!(
        call(&path, "GET", &proposed_uri, None, true).await.0,
        StatusCode::UNAUTHORIZED
    );
    let approve_uri = format!("{actions_uri}/{action_id}/approve");
    let (status, denied) = call(
        &path,
        "POST",
        &approve_uri,
        Some(json!({"expectedIntentRevision":0})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{denied}");
    let (status, approved) = call(
        &path,
        "POST",
        &approve_uri,
        Some(json!({"expectedIntentRevision":0})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    assert_eq!(
        call(&path, "GET", &proposed_uri, None, false).await.1["actions"],
        json!([])
    );
    let work_uri = format!("{tasks_uri}/{source_id}/work-task");
    let (_, before) = call(&path, "GET", &work_uri, None, false).await;
    let mut child_request = common;
    child_request["operation"] = json!("generateBrief");
    child_request["target"] = action["target"].clone();
    child_request["target"]["requestedByTaskId"] = json!(source_id);
    child_request["target"]["requestedActionId"] = json!(action_id);
    let mut forged = child_request.clone();
    forged["target"]["povCharacterId"] = json!("unexpected");
    let (status, rejected) = call(&path, "POST", &tasks_uri, Some(forged), false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    let (_, after) = call(&path, "GET", &work_uri, None, false).await;
    assert_eq!(after, before);
    let (status, child) = call(&path, "POST", &tasks_uri, Some(child_request), false).await;
    assert_eq!(status, StatusCode::CREATED, "{child}");
    let child_id = child["id"].as_str().unwrap();
    let finish_uri = format!("{actions_uri}/{action_id}/finish");
    let (status, rejected) = call(
        &path,
        "POST",
        &finish_uri,
        Some(json!({"resultTaskId":child_id})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{rejected}");
    let (status, output) = call(&path, "POST", &format!("/api/projects/{project}/artifacts/brief"), Some(json!({
        "payload":{"premise":"港城正在失去记忆"}, "expectedRevision":0, "expectedCandidateRevisionId":null,
        "idempotencyKey":"binding-save", "taskId":child_id, "threadId":"binding-thread"
    })), true).await;
    assert_eq!(status, StatusCode::CREATED, "{output}");
    let (status, completed) = call(
        &path,
        "POST",
        &format!("{tasks_uri}/{child_id}/complete"),
        Some(json!({"threadId":"binding-thread", "summary":"已保存候选"})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let (status, receipt) = call(
        &path,
        "POST",
        &finish_uri,
        Some(json!({"resultTaskId":child_id})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["status"], "completed");
    let (status, repeated) = call(
        &path,
        "POST",
        &finish_uri,
        Some(json!({"resultTaskId":child_id})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{repeated}");
    assert_eq!(repeated, receipt);
    let (status, _) = call(
        &path,
        "POST",
        &finish_uri,
        Some(json!({"error":"迟到失败"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (_, listed) = call(&path, "GET", &actions_uri, None, false).await;
    assert_eq!(listed["actions"], json!([receipt]));
}

#[tokio::test]
async fn creative_methods_are_author_selected_frozen_and_included_in_context_budget() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("methods-http.sqlite");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"方法测试"})),
        false,
    )
    .await;
    let project = created["project"]["id"].as_str().unwrap();
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let base = format!("/api/projects/{project}");
    let payload = json!({"operation":"advisory","inputRevision":0,"threadId":"method-thread",
        "target":{"type":"project"},"capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
        "capabilityExpiresAt":expires,"authorIntent":"分析故事","experimentalMethods":["story-tests"]});
    let (status, task) = call(
        &path,
        "POST",
        &format!("{base}/tasks"),
        Some(payload.clone()),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let binding = &task["inputManifest"]["creativeCapabilityBinding"];
    assert_eq!(binding["selectedMethods"].as_array().unwrap().len(), 2);
    assert_eq!(binding["selectedMethods"][1]["id"], "story-tests");
    assert_eq!(binding["selectedMethods"][1]["status"], "candidate");
    let task_id = task["id"].as_str().unwrap();
    let uri = format!(
        "{base}/context?taskId={task_id}&threadId=method-thread&projectRevision=0&purpose=planner&budget=24&byteBudget=60000"
    );
    let (status, context) = call(&path, "GET", &uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let mut metadata = binding.clone();
    for method in metadata["selectedMethods"].as_array_mut().unwrap() {
        method.as_object_mut().unwrap().remove("text");
    }
    metadata["selectedMethodsSource"] = context["creativeCapabilityBinding"]["selectedMethodsSource"].clone();
    assert_eq!(context["creativeCapabilityBinding"], metadata);
    assert!(metadata["selectedMethodsSource"]["sourceId"].is_string());
    assert!(context.to_string().len() <= 60000);
    let (status, _) = call(
        &path,
        "GET",
        &uri.replace("byteBudget=60000", "byteBudget=4000"),
        None,
        true,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = call(
        &path,
        "POST",
        &format!("{base}/tasks"),
        Some(payload.clone()),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let mut invalid = payload;
    invalid["experimentalMethods"] = json!(["narrative-ir"]);
    let (status, _) = call(
        &path,
        "POST",
        &format!("{base}/tasks"),
        Some(invalid),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let repo = novel_codex_story_core::SqliteStoryRepository::open(&path).unwrap();
    let project_id = novel_codex_story_core::ProjectId::parse(project).unwrap();
    assert_eq!(repo.tasks(&project_id).unwrap().len(), 1);
}

fn application(path: &std::path::Path) -> Router {
    router(task_fixture::state(
        path.to_owned(),
        AUTHOR_TOKEN.into(),
        CANDIDATE_TOKEN.into(),
    ))
}

#[tokio::test]
async fn author_steer_revisions_fence_candidate_actions_and_completion() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("author-intents.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"作者意图"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, task) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(json!({
            "operation":"advisory", "inputRevision":0, "threadId":"intent-thread",
            "target":{"type":"project"}, "authorIntent":"先讨论白塔",
            "capabilityHash":format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt":expires
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let base = format!("/api/projects/{project_id}/tasks/{task_id}");
    let context_uri = format!(
        "/api/projects/{project_id}/context?taskId={task_id}&threadId=intent-thread&projectRevision=0&purpose=planner&budget=24&byteBudget=60000"
    );
    let action = |key: &str, revision: i64| {
        json!({
            "taskId":task_id, "threadId":"intent-thread", "expectedRevision":0,
            "expectedIntentRevision":revision, "idempotencyKey":key,
            "operation":"generateBrief", "target":{"type":"artifact"}, "instruction":"拟定简报"
        })
    };
    let (status, initial_context) = call(&path, "GET", &context_uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{initial_context}");
    assert_eq!(
        initial_context["authorIntentState"],
        json!({"expectedIntentRevision":0,"canRequestActions":true,"status":"current"})
    );
    let (status, unissued) = call(
        &path,
        "POST",
        &format!("{base}/actions"),
        Some(action("unissued", 1)),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{unissued}");
    assert!(
        unissued["error"]["message"]
            .as_str()
            .unwrap()
            .contains("has not been issued")
    );
    let (status, original) = call(
        &path,
        "POST",
        &format!("{base}/actions"),
        Some(action("first", 0)),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{original}");
    assert_eq!(original["intentRevision"], 0);
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{base}/intents"),
            Some(json!({"expectedIntentRevision":0,"authorText":"不要生成简报，先调查林夕"})),
            true
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, pending) = call(
        &path,
        "POST",
        &format!("{base}/intents"),
        Some(json!({"expectedIntentRevision":0,"authorText":"改为生成大纲"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    assert_eq!(pending["intentRevision"], 1);
    assert_eq!(pending["status"], "pending");
    let (status, pending_context) = call(&path, "GET", &context_uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{pending_context}");
    assert_eq!(
        pending_context["authorIntentState"],
        json!({"expectedIntentRevision":1,"canRequestActions":false,"status":"pendingDelivery"})
    );
    let (status, earlier_actions) =
        call(&path, "GET", &format!("{base}/actions"), None, false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(earlier_actions["actions"][0]["status"], "superseded");
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{base}/actions"),
            Some(action("pending", 0)),
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{base}/complete"),
            Some(json!({"threadId":"intent-thread","summary":"结束"})),
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (status, delivered) = call(
        &path,
        "POST",
        &format!("{base}/intents/1/delivered"),
        Some(json!({})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{delivered}");
    assert_eq!(delivered["status"], "delivered");
    let (status, updated_context) = call(&path, "GET", &context_uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{updated_context}");
    assert_eq!(
        updated_context["authorIntentState"],
        json!({"expectedIntentRevision":1,"canRequestActions":true,"status":"current"})
    );
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{base}/actions"),
            Some(action("stale", 0)),
            true
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{base}/actions"),
            Some(action("future", 2)),
            true
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (status, current) = call(
        &path,
        "POST",
        &format!("{base}/actions"),
        Some(action("current", 1)),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{current}");
    assert_eq!(current["intentRevision"], 1);
    let (status, replay) = call(
        &path,
        "POST",
        &format!("{base}/actions"),
        Some(action("first", 0)),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(replay["id"], original["id"]);
    assert_eq!(replay["status"], "superseded");
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{base}/complete"),
            Some(json!({"threadId":"intent-thread","summary":"结束"})),
            true
        )
        .await
        .0,
        StatusCode::OK
    );
    settle_fixture::settle(&path, project_id, task_id);
    let (status, approved) = call(
        &path,
        "POST",
        &format!("{base}/actions/{}/approve", current["id"].as_str().unwrap()),
        Some(json!({"expectedIntentRevision":1})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    let child_request = |action_id: &Value| {
        json!({
            "operation":"generateBrief", "inputRevision":0, "threadId":"child-thread",
            "target":{"type":"artifact","requestedByTaskId":task_id,"requestedActionId":action_id},
            "authorIntent":"按作者最新要求拟定简报",
            "capabilityHash":format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt":expires
        })
    };
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("/api/projects/{project_id}/tasks"),
            Some(child_request(&original["id"])),
            false
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (status, child) = call(
        &path,
        "POST",
        &format!("/api/projects/{project_id}/tasks"),
        Some(child_request(&current["id"])),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{child}");
    assert_eq!(
        json!({
            "authorIntent": child["inputManifest"]["authorIntent"],
            "candidateInstruction": child["inputManifest"]["candidateInstruction"],
            "authorIntentBinding": child["inputManifest"]["authorIntentBinding"]
        }),
        json!({
            "authorIntent": "先讨论白塔\n\n作者追加要求：\n改为生成大纲",
            "candidateInstruction": "拟定简报",
            "authorIntentBinding": {
                "sourceTaskId": task_id, "intentRevision": 1, "actionId": current["id"]
            }
        })
    );
    assert_eq!(
        call(
            &path,
            "POST",
            &format!("{base}/intents"),
            Some(json!({"expectedIntentRevision":1,"authorText":"继续"})),
            false
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

fn chapter_acceptance(task: &Value) -> Value {
    let author = task["inputManifest"]["authorIntent"].as_str().unwrap();
    assert!(!author.is_empty());
    json!({"requirements":[{"id":"author-goal","text":author}],
        "length":{"requirement":"作者原话未指定篇幅；保留原写作范围","minCharacters":null,"maxCharacters":null}})
}

fn assert_model_item_bound(value: &Value) {
    let result = json!({"content":[{"type":"text","text":value.to_string()}]});
    let item = json!({"type":"function_call_output","call_id":"x".repeat(160),"output":result.to_string()});
    let bytes = item.to_string().len();
    assert!(bytes <= 256 * 1024 - 1024, "actual escaped model item has {bytes} bytes");
}

// The current relevantText projection contains the complete admitted scene
// groups, without the duplicate body. Read every group already delivered by
// the task source; never borrow an unprojected revision or alternate format.
fn projected_scene_body(content: &Value) -> String {
    assert!(content.get("body").is_none(), "{content}");
    let groups = content["sceneGroups"].as_array().unwrap();
    assert_eq!(
        groups.len(),
        1,
        "these fixtures each saved one complete scene"
    );
    groups
        .iter()
        .map(|group| group["body"].as_str().unwrap().trim())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

async fn read_section(
    path: &std::path::Path,
    root: &str,
    task: &Value,
    context: &Value,
    section: &str,
) -> Value {
    let entry = context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["sectionPath"] == section)
        .unwrap_or_else(|| panic!("missing current source {section}: {context}"));
    assert_eq!(
        context.pointer(section).unwrap()["sourceId"],
        entry["sourceId"]
    );
    read_content(
        path,
        root,
        task,
        entry["sourceId"].as_str().unwrap(),
        entry["contentHash"].as_str(),
    )
    .await
}

async fn read_workspace_entries(
    path: &std::path::Path,
    root: &str,
    task: &Value,
    context: &Value,
    filter: &str,
) -> Vec<Value> {
    let (status, current_task) = call(
        path,
        "GET",
        &format!("{root}/tasks/{}", task["id"].as_str().unwrap()),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{current_task}");
    assert_eq!(current_task["id"], task["id"]);
    assert_eq!(current_task["threadId"], task["threadId"]);
    assert_eq!(current_task["inputRevision"], task["inputRevision"]);
    assert!(
        matches!(
            current_task["status"].as_str(),
            Some("queued" | "running" | "waiting")
        ),
        "{current_task}"
    );
    let (status, current_work) = call(
        path,
        "GET",
        &format!("{root}/tasks/{}/work-task", task["id"].as_str().unwrap()),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{current_work}");
    assert_eq!(current_work["status"], "running", "{current_work}");
    let source = context["taskWorkspace"]["sourceId"].as_str().unwrap();
    let endpoint = format!(
        "{root}/tasks/{}/task-source-content?threadId={}&expectedRevision={}&sourceId={source}&view=workspace{filter}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"]
    );
    let mut cursor = String::new();
    let mut entries = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut cursors = std::collections::BTreeSet::new();
    loop {
        let (status, page) = call(path, "GET", &format!("{endpoint}{cursor}"), None, true).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["sourceId"], source);
        assert_eq!(page["representation"], "workspaceDirectory");
        assert_eq!(page["contentRead"], false);
        assert!(page.to_string().len() <= 6500);
        assert_model_item_bound(&page);
        for entry in page["items"].as_array().unwrap() {
            assert_eq!(entry["contentRead"], false);
            assert_eq!(entry["contentHash"], Value::Null);
            assert!(seen.insert(entry["sourceId"].as_str().unwrap().to_owned()));
            entries.push(entry.clone());
        }
        let Some(next) = page["nextCursor"].as_str() else {
            break;
        };
        assert!(cursors.insert(next.to_owned()));
        assert!(cursors.len() < 40);
        cursor = format!("&cursor={next}");
    }
    entries
}

async fn read_saved_checkpoint(
    path: &std::path::Path,
    root: &str,
    task: &Value,
    context: &Value,
    saved: &Value,
    ordinal: u64,
    current: bool,
) -> Value {
    let entries = read_workspace_entries(
        path,
        root,
        task,
        context,
        "&chapterNumber=1&objectKind=draft",
    )
    .await;
    let entry = entries
        .iter()
        .find(|item| {
            item["revisionId"] == saved["revisionId"]
                && item["producerTaskId"] == task["id"]
                && item["outputOrdinal"] == ordinal
        })
        .unwrap_or_else(|| panic!("missing genuine saved checkpoint: {entries:?}"));
    assert_eq!(entry["purpose"], "savedCheckpoint");
    assert_eq!(entry["authority"], "nativeSavedCheckpoint");
    assert_eq!(entry["outputKind"], "draft");
    assert_eq!(
        entry["version"],
        json!({"revisionId":saved["revisionId"],"outputOrdinal":ordinal})
    );
    assert_eq!(entry["isCurrentAtListing"], current);
    assert_eq!(entry["currentMeaning"], "equalsCandidateHeadAtListing");
    assert_eq!(
        entry["evidenceUse"],
        "immutableSavedVersionForReadingAndComparison;notAcceptanceOrDelivery"
    );
    let source = read_content(path, root, task, entry["sourceId"].as_str().unwrap(), None).await;
    assert_eq!(source["sourceRevisionId"], saved["revisionId"]);
    assert_eq!(
        source["objectId"],
        format!("draft-{}", saved["chapterId"].as_str().unwrap())
    );
    assert_eq!(source["producerTaskId"], task["id"]);
    assert_eq!(source["outputOrdinal"], ordinal);
    assert_eq!(source["purpose"], "savedCheckpoint");
    assert_eq!(source["authority"], "nativeSavedCheckpoint");
    assert_eq!(source["evidenceUse"], entry["evidenceUse"]);
    assert!(source.get("isCurrentAtListing").is_none());
    assert!(source.get("producerStatusAtListing").is_none());
    source
}

// Rebuild only the actually returned parsed fields; no database/author read can
// stand in for the task's complete source transport and character coverage.
async fn read_content(
    path: &std::path::Path,
    root: &str,
    task: &Value,
    source: &str,
    expected_hash: Option<&str>,
) -> Value {
    let endpoint = format!(
        "{root}/tasks/{}/task-source-content?threadId={}&expectedRevision={}&sourceId={source}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"]
    );
    let mut cursor = String::new();
    let mut content_hash = expected_hash.map(str::to_owned);
    let mut returned = Value::Null;
    let mut chunks = std::collections::BTreeMap::<String, (usize, usize, String)>::new();
    let mut from = 0usize;
    let mut total = None;
    loop {
        let (status, page) = call(path, "GET", &format!("{endpoint}{cursor}"), None, true).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["sourceId"], source);
        assert_eq!(page["representation"], "parsedFields");
        assert_eq!(page["contentRead"], true);
        assert!(page.to_string().len() <= 63 * 1024);
        assert_model_item_bound(&page);
        let hash = page["contentHash"].as_str().unwrap();
        assert_eq!(
            content_hash.get_or_insert_with(|| hash.to_owned()).as_str(),
            hash
        );
        assert_eq!(page["coverage"]["fromEntry"], from);
        let entries = page["entries"].as_array().unwrap();
        assert!(!entries.is_empty());
        from += entries.len();
        assert_eq!(page["coverage"]["toEntry"], from);
        let count = page["coverage"]["totalEntries"].as_u64().unwrap() as usize;
        assert_eq!(*total.get_or_insert(count), count);
        assert!(from <= count);
        for entry in entries {
            let pointer = entry["path"].as_str().unwrap();
            if let Some(text) = entry["text"].as_str() {
                let range = &entry["characterRange"];
                let start = range["start"].as_u64().unwrap() as usize;
                let end = range["end"].as_u64().unwrap() as usize;
                let full = range["total"].as_u64().unwrap() as usize;
                let chunk = chunks
                    .entry(pointer.to_owned())
                    .or_insert((0, full, String::new()));
                assert_eq!((start, full), (chunk.0, chunk.1));
                assert_eq!(end, start + text.chars().count());
                assert!(end <= full);
                chunk.0 = end;
                chunk.2.push_str(text);
            } else {
                assert!(entry.get("value").is_some(), "{entry}");
                set_field(&mut returned, pointer, entry["value"].clone());
            }
        }
        if let Some(next) = page["nextCursor"].as_str() {
            assert!(from < count);
            let next = format!("&cursor={next}");
            assert_ne!(next, cursor);
            cursor = next;
        } else {
            assert_eq!(from, count);
            break;
        }
    }
    for (pointer, (end, total, text)) in chunks {
        assert_eq!(end, total);
        set_field(&mut returned, &pointer, json!(text));
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(returned.to_string().as_bytes())),
        content_hash.unwrap()
    );
    returned
}

fn set_field(value: &mut Value, pointer: &str, field: Value) {
    let parts: Vec<String> = pointer.strip_prefix('/').map_or_else(Vec::new, |pointer| {
        pointer
            .split('/')
            .map(|part| part.replace("~1", "/").replace("~0", "~"))
            .collect()
    });
    fn put(value: &mut Value, parts: &[String], field: Value) {
        let Some((part, rest)) = parts.split_first() else {
            *value = field;
            return;
        };
        if value.is_null() {
            *value = if part.parse::<usize>().is_ok() {
                json!([])
            } else {
                json!({})
            };
        }
        match value {
            Value::Array(items) => {
                let index = part.parse::<usize>().unwrap();
                if items.len() <= index {
                    items.resize(index + 1, Value::Null);
                }
                put(&mut items[index], rest, field);
            }
            Value::Object(object) => put(
                object.entry(part.clone()).or_insert(Value::Null),
                rest,
                field,
            ),
            _ => panic!("non-container returned source at {parts:?}"),
        }
    }
    put(value, &parts, field);
}

async fn call(
    path: &std::path::Path,
    method: &str,
    uri: &str,
    payload: Option<Value>,
    writer: bool,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri).header(
        "authorization",
        format!(
            "Bearer {}",
            if writer {
                CANDIDATE_TOKEN
            } else {
                AUTHOR_TOKEN
            }
        ),
    );
    if writer {
        request = request.header("x-novel-task-capability", CAPABILITY);
        if method == "GET"
            && (uri.contains("/task-source?") || uri.contains("/task-source-content?"))
        {
            let thread = uri
                .split("threadId=")
                .nth(1)
                .unwrap()
                .split('&')
                .next()
                .unwrap();
            request = request
                .header(
                    "x-novel-tool-name",
                    if uri.contains("/task-source-content?") {
                        "read_source_content"
                    } else {
                        "read_source_content"
                    },
                )
                .header("x-novel-root-thread", thread)
                .header("x-novel-caller-thread", thread)
                .header("x-novel-turn", "context-scope-read")
                .header("x-novel-tool-call", "context-scope-source");
        }
    }
    let body = if let Some(payload) = payload {
        request = request.header("content-type", "application/json");
        Body::from(payload.to_string())
    } else {
        Body::empty()
    };
    let response = application(path)
        .author_fixture_request(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        task_fixture::work_control_response(uri, status, serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            panic!(
                "non-JSON response {status}: {}",
                String::from_utf8_lossy(&bytes)
            )
        })),
    )
}

#[tokio::test]
async fn writer_context_keeps_target_chapter_after_eighty_prior_chapters() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scope.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Long novel"})),
        false,
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let drafts_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let mut target_chapter_id = String::new();
    let mut latest_chapter_id = String::new();
    for chapter in 1..=101 {
        let (status, draft) = call(
            &path,
            "POST",
            &drafts_uri,
            Some(json!({
                "payload": {"chapterNumber": chapter, "title": format!("第{chapter}章"),
                    "body": format!("第{chapter}章正文。")},
                "expectedRevision": 0, "expectedCandidateRevisionId": null,
                "idempotencyKey": format!("scope-draft-{chapter}")
            })),
            false,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "chapter {chapter}: {draft}");
        if chapter == 81 {
            target_chapter_id = draft["chapterId"].as_str().unwrap().to_owned();
        }
        if chapter == 101 {
            latest_chapter_id = draft["chapterId"].as_str().unwrap().to_owned();
        }
    }
    let task_uri = format!("/api/projects/{project_id}/tasks");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, task) = call(
        &path,
        "POST",
        &task_uri,
        Some(json!({
            "operation": "generateChapter", "inputRevision": 0, "threadId": "scope-writer",
            "target": {"chapterId": target_chapter_id, "chapterNumber": 81,
                "readingChapter": 81, "sceneOrder": 1},
            "capabilityHash": format!("{:x}", Sha256::digest(CAPABILITY.as_bytes())),
            "capabilityExpiresAt": expires, "authorIntent": "Continue chapter eighty-one"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let context_uri = format!(
        "/api/projects/{project_id}/context?taskId={}&threadId=scope-writer&projectRevision=0&purpose=writer&readingChapter=81&sceneOrder=1&budget=24&byteBudget=60000",
        task["id"].as_str().unwrap()
    );
    let (status, context) = call(&path, "GET", &context_uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let root = format!("/api/projects/{project_id}");
    let relevant = read_section(&path, &root, &task, &context, "/relevantText").await;
    let drafts = relevant.as_array().unwrap();
    assert_eq!(drafts.len(), 80);
    assert_eq!(drafts.first().unwrap()["content"]["chapterNumber"], 2);
    assert_eq!(drafts.last().unwrap()["content"]["chapterNumber"], 81);
    assert!(drafts.last().unwrap()["sourceRevisionId"].is_string());
    for entry in drafts {
        let number = entry["content"]["chapterNumber"].as_i64().unwrap();
        assert!((2..=81).contains(&number));
        assert_eq!(
            projected_scene_body(&entry["content"]),
            format!("第{number}章正文。")
        );
    }
    assert_model_item_bound(&context);
    let directory =
        read_workspace_entries(&path, &root, &task, &context, "&objectKind=draft").await;
    assert_eq!(directory.len(), 81);
    let chapter_numbers: std::collections::BTreeSet<i64> = directory
        .iter()
        .map(|entry| entry["chapterNumber"].as_i64().unwrap())
        .collect();
    assert_eq!(chapter_numbers, (1..=81).collect());
    let current = directory
        .iter()
        .find(|entry| entry["chapterNumber"] == 81)
        .unwrap();
    let current_text = read_content(
        &path,
        &root,
        &task,
        current["sourceId"].as_str().unwrap(),
        None,
    )
    .await;
    assert_eq!(
        current_text["sourceRevisionId"],
        drafts.last().unwrap()["sourceRevisionId"]
    );
    assert_eq!(current_text["textRepresentation"], "sceneGroups");
    assert!(current_text["content"].get("body").is_none());
    assert_eq!(current_text["content"]["sceneGroups"][0]["body"], "第81章正文。");
    assert_eq!(context["coverage"]["scannedScope"]["chaptersInScope"], 81);
    assert_eq!(
        context["coverage"]["missingScope"],
        json!([
            "chapterText",
            "knowledgeModeling",
            "workingKnowledge",
            "taskSourceContents"
        ])
    );
    assert_eq!(context["coverage"]["complete"], false);
    assert_eq!(context["taskId"], task["id"]);
    assert_eq!(
        context["snapshotId"],
        context["sourceWatermarks"]["snapshotId"]
    );
    assert_eq!(context["sourceWatermarks"]["objectCount"], 81);
    let (author_status, author_context) = call(
        &path,
        "GET",
        &format!(
            "/api/projects/{project_id}/context?projectRevision=0&purpose=planner&budget=24&byteBudget=60000"
        ),
        None,
        false,
    )
    .await;
    assert_eq!(author_status, StatusCode::OK, "{author_context}");
    assert_eq!(author_context["sourceWatermarks"]["objectCount"], 101);
    assert_eq!(
        author_context["coverage"]["scannedScope"]["modelingSourcesInScope"],
        101
    );
    assert_eq!(
        author_context["coverage"]["scannedScope"]["modelingSourcesIncluded"],
        100
    );
    assert_eq!(author_context["coverage"]["complete"], false);
    assert!(
        author_context["coverage"]["missingScope"]
            .as_array()
            .unwrap()
            .contains(&json!("knowledgeModeling"))
    );
    let (long_project_status, long_project) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"确定超出模型条目额度的真实正文"})),
        false,
    )
    .await;
    assert_eq!(long_project_status, StatusCode::CREATED, "{long_project}");
    let long_project_id = long_project["project"]["id"].as_str().unwrap();
    let long_body = "霾".repeat(16000);
    assert_eq!(long_body.len(), 48000);
    let body_tokens = eternallight_engine_core::dispatch("tokens.count", json!(long_body))
        .unwrap()
        .as_u64()
        .unwrap();
    assert!(
        body_tokens > 9000,
        "required target prose alone has {body_tokens} tokens"
    );
    let (long_draft_status, long_draft) = call(
        &path,
        "POST",
        &format!("/api/projects/{long_project_id}/drafts/checkpoints"),
        Some(json!({"payload":{"chapterNumber":1,"title":"必需目标正文","body":long_body},
            "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"deterministic-model-item-overflow"})),
        false,
    )
    .await;
    assert_eq!(long_draft_status, StatusCode::CREATED, "{long_draft}");
    assert_eq!(long_draft["body"], long_body);
    let projected_target_bytes = json!({"objectId":format!("draft-{}",long_draft["chapterId"].as_str().unwrap()),
        "sourceRevisionId":long_draft["revisionId"],"authority":"candidate","workingSelectionRevision":null,
        "content":{"chapterId":long_draft["chapterId"],"chapterNumber":1,"title":"必需目标正文",
            "completeness":"chapterComplete","sceneGroups":long_draft["sceneGroups"]}}).to_string().len();
    assert!(
        projected_target_bytes < 58000,
        "target bytes={projected_target_bytes}"
    );
    let (oversized_status, oversized_context) = call(
        &path,
        "GET",
        &format!(
            "/api/projects/{long_project_id}/context?projectRevision=0&purpose=writer&readingChapter=1&sceneOrder=1&budget=24&byteBudget=60000"
        ),
        None,
        false,
    )
    .await;
    assert_eq!(oversized_status, StatusCode::OK, "{oversized_context}");
    assert!(oversized_context.to_string().contains(&long_body), "full high-token target must remain readable");
    let (latest_status, latest_context) = call(
        &path,
        "GET",
        &format!(
            "/api/projects/{project_id}/context?projectRevision=0&purpose=writer&readingChapter=101&sceneOrder=1&budget=8&byteBudget=20000"
        ),
        None,
        false,
    )
    .await;
    assert_eq!(latest_status, StatusCode::OK, "{latest_context}");
    assert_model_item_bound(&latest_context);
    assert_eq!(latest_context["taskId"], Value::Null);
    assert_eq!(latest_context["purpose"], "writer");
    assert_eq!(latest_context["target"]["position"]["readingChapter"], 101);
    assert_eq!(latest_context["target"]["position"]["sceneOrder"], 1);
    assert_eq!(
        latest_context["coverage"]["requestedScope"]["entryLimit"],
        8
    );
    assert_eq!(latest_context["budgetsUsed"]["byteLimit"], 20000);
    assert_eq!(
        latest_context["budgetsUsed"]["byteSize"],
        latest_context.to_string().len()
    );
    assert!(latest_context.to_string().len() <= 20000);
    assert_eq!(latest_context["snapshotId"], author_context["snapshotId"]);
    assert_eq!(
        latest_context["sourceWatermarks"],
        author_context["sourceWatermarks"]
    );
    let mut latest_hashed_content = latest_context.clone();
    latest_hashed_content.as_object_mut().unwrap().remove("id");
    latest_hashed_content
        .as_object_mut()
        .unwrap()
        .remove("contentHash");
    let latest_hash = format!(
        "{:x}",
        Sha256::digest(latest_hashed_content.to_string().as_bytes())
    );
    assert_eq!(latest_context["contentHash"], latest_hash);
    assert_eq!(latest_context["id"], format!("context-{latest_hash}"));
    let included_sources = latest_context["operationContext"]["modeling"]
        .as_array()
        .unwrap()
        .len();
    assert!(included_sources > 0 && included_sources <= 100);
    assert_eq!(
        latest_context["coverage"]["scannedScope"]["modelingSourcesIncluded"],
        included_sources
    );
    assert_eq!(
        latest_context["coverage"]["scannedScope"]["chaptersInScope"],
        101
    );
    assert!(
        latest_context["coverage"]["truncationReasons"]
            .as_array()
            .unwrap()
            .contains(&json!("contextSourceBudget"))
    );
    assert!(
        latest_context["operationContext"]["modeling"]
            .as_array()
            .unwrap()
            .iter()
            .any(|source| source["objectId"] == format!("draft-{latest_chapter_id}"))
    );
    assert_eq!(
        latest_context["coverage"]["scannedScope"]["modelingSourcesInScope"],
        101
    );
    assert_eq!(latest_context["coverage"]["complete"], false);
    assert_ne!(
        context["sourceWatermarks"]["digest"],
        author_context["sourceWatermarks"]["digest"]
    );
    assert_eq!(context["sourceWatermarks"]["knowledgeBatchCount"], 0);
    assert_eq!(
        context["sourceWatermarks"]["digest"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(context["policyDigest"].as_str().unwrap().len(), 64);
    assert_eq!(
        context["budgetsUsed"]["byteSize"],
        context.to_string().len()
    );
    let mut hashed_content = context.clone();
    hashed_content.as_object_mut().unwrap().remove("id");
    hashed_content
        .as_object_mut()
        .unwrap()
        .remove("contentHash");
    let expected_hash = format!(
        "{:x}",
        Sha256::digest(hashed_content.to_string().as_bytes())
    );
    assert_eq!(context["contentHash"], expected_hash);
    assert_eq!(context["id"], format!("context-{expected_hash}"));
    assert!(context.get("drafts").is_none());
    assert!(context.get("inputSnapshot").is_none());
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
#[path = "support/settle_task_fixture.rs"]
mod settle_fixture;

#[tokio::test]
async fn exact_draft_patch_reuses_candidate_cas_scope_and_idempotent_save() {
    let directory=tempfile::tempdir().unwrap();let path=directory.path().join("patch.sqlite3");
    let (_,created)=call(&path,"POST","/api/projects",Some(json!({"logline":"","medium":"novel","idempotencyKey":"project","title":"精确修改"})),false).await;
    let project=created["project"]["id"].as_str().unwrap();let base=format!("/api/projects/{project}");let saves=format!("{base}/drafts/checkpoints");
    let original=json!({"chapterId":"chapter","chapterNumber":1,"title":"月下","body":"风🌙风月\n\n---\n\n保留原文","completeness":"chapterComplete","sceneGroups":[{"id":"selected","body":"风🌙风月","locked":false,"sceneId":null},{"id":"other","body":"保留原文","locked":false,"sceneId":null}]});
    let (status,saved)=call(&path,"POST",&saves,Some(json!({"payload":original,"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"baseline"})),false).await;
    assert_eq!(status,StatusCode::CREATED,"{saved}");
    for (revision, candidate, reason, field) in [
        (1, saved["revisionId"].clone(), "project_revision_stale", "/expectedRevision"),
        (0, Value::Null, "candidate_revision_stale", "/expectedCandidateRevisionId"),
    ] {
        let (status, rejected) = call(&path, "POST", &saves, Some(json!({"payload":original,"expectedRevision":revision,"expectedCandidateRevisionId":candidate,"idempotencyKey":reason})), false).await;
        assert_eq!(status, StatusCode::CONFLICT, "{rejected}");
        assert_eq!(rejected["error"]["details"]["reason"], reason);
        assert_eq!(rejected["error"]["details"]["instancePath"], field);
        assert_eq!(rejected["error"]["details"]["origin"], "storyRuntime");
    }
    let expires=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+600;
    let (status,task)=call(&path,"POST",&format!("{base}/tasks"),Some(json!({"operation":"generateChapter","inputRevision":0,"threadId":"patch-root","target":{"chapterId":"chapter","chapterNumber":1,"readingChapter":1,"sceneOrder":1,"scope":{"type":"selectedScenes","sourceRevisionId":saved["revisionId"],"groupIds":["selected"],"bodyRanges":[{"groupId":"selected","start":2,"end":3}]}},"authorIntent":"只把第二个风改为雨，其余保持","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires})),false).await;
    assert_eq!(status,StatusCode::CREATED,"{task}");
    let context_uri=format!("{base}/context?taskId={}&threadId=patch-root&projectRevision=0&purpose=writer&readingChapter=1&sceneOrder=1&budget=24&byteBudget=60000",task["id"].as_str().unwrap());
    let (status,context)=call(&path,"GET",&context_uri,None,true).await;
    assert_eq!(status,StatusCode::OK,"{context}");
    let source=context["taskSources"]["entries"].as_array().unwrap().iter().find(|v|v["sectionPath"]=="/relevantText").unwrap();
    let locate=format!("{base}/tasks/{}/task-source-content?threadId=patch-root&expectedRevision=0&sourceId={}&view=locate&quote=%E9%A3%8E&groupId=selected",task["id"].as_str().unwrap(),source["sourceId"].as_str().unwrap());
    let (status,matches)=call(&path,"GET",&locate,None,true).await;
    assert_eq!(status,StatusCode::OK,"{matches}");assert_eq!(matches["matchCount"],2);
    assert_eq!(matches["coverage"]["completeSource"],false);
    assert_eq!(matches["entries"][0]["editAnchor"]["start"],0);
    assert_eq!(matches["entries"][1]["editAnchor"]["start"],2);
    let (_,after_locate)=call(&path,"GET",&context_uri,None,true).await;
    assert_eq!(after_locate["taskSources"],context["taskSources"],"locating must not certify full reading");
    let bad_cursor=format!("{locate}&cursor={}:0","0".repeat(64));
    assert!(call(&path,"GET",&bad_cursor,None,true).await.0.is_client_error());
    let foreign=locate.replace(source["sourceId"].as_str().unwrap(),&format!("source-{}","f".repeat(64)));
    assert!(call(&path,"GET",&foreign,None,true).await.0.is_client_error());
    let request=json!({"taskId":task["id"],"threadId":"patch-root","expectedRevision":0,"expectedCandidateRevisionId":saved["revisionId"],"idempotencyKey":"local-edit","patch":{"chapterId":"chapter","sourceRevisionId":saved["revisionId"],"edits":[{"groupId":"selected","start":matches["entries"][1]["editAnchor"]["start"],"end":matches["entries"][1]["editAnchor"]["end"],"oldText":matches["entries"][1]["text"],"newText":"雨"}]}});
    for (field,value) in [("sourceRevisionId",json!("foreign")),("chapterId",json!("foreign"))] {
        let mut invalid=request.clone();invalid["patch"][field]=value;
        let (status,_) = call(&path,"POST",&saves,Some(invalid),true).await;assert!(status.is_client_error());
    }
    let mut wrong=request.clone();wrong["patch"]["edits"][0]["oldText"]="月".into();
    let (status,rejected)=call(&path,"POST",&saves,Some(wrong),true).await;
    assert_eq!(status,StatusCode::BAD_REQUEST,"{rejected}");assert_eq!(rejected["error"]["details"]["reason"],"draft_patch_invalid");
    assert_eq!(rejected["error"]["details"]["constraint"],"old_text_mismatch");
    assert_eq!(rejected["error"]["details"]["instancePath"],"/patch/edits/0/oldText");
    let mut outside=request.clone();outside["patch"]["edits"][0]=json!({"groupId":"other","start":0,"end":1,"oldText":"保","newText":"改"});
    let (status,rejected)=call(&path,"POST",&saves,Some(outside),true).await;assert!(status.is_client_error(),"{rejected}");
    let (status,result)=call(&path,"POST",&saves,Some(request.clone()),true).await;
    assert_eq!(status,StatusCode::CREATED,"{result}");
    assert_eq!(result["body"],"风🌙雨月\n\n---\n\n保留原文");
    assert_eq!(result["sceneGroups"][1],original["sceneGroups"][1]);
    assert_eq!(call(&path,"POST",&saves,Some(request.clone()),true).await,(StatusCode::CREATED,result.clone()));
    let mut changed=request.clone();changed["patch"]["edits"][0]["newText"]="雪".into();
    assert!(call(&path,"POST",&saves,Some(changed.clone()),true).await.0.is_client_error());
    changed["idempotencyKey"]="new-key".into();assert!(call(&path,"POST",&saves,Some(changed),true).await.0.is_client_error());
    let (_,workspace)=call(&path,"GET",&format!("{base}/creative-workspace"),None,false).await;
    assert_eq!(workspace["chapters"][0]["candidate"]["id"],result["revisionId"]);
}

#[tokio::test]
async fn tool_binding_comes_only_from_unique_live_capability_and_root() {
    use tower::ServiceExt;
    let directory=tempfile::tempdir().unwrap();let path=directory.path().join("binding.sqlite3");
    let (_,created)=call(&path,"POST","/api/projects",Some(json!({"logline":"","medium":"novel","idempotencyKey":"project","title":"绑定"})),false).await;
    let project=created["project"]["id"].as_str().unwrap();let base=format!("/api/projects/{project}");
    let expires=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+600;
    let request=json!({"operation":"advisory","inputRevision":0,"threadId":"binding-root","target":{"type":"project"},"authorIntent":"讨论","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires});
    let (status,task)=call(&path,"POST",&format!("{base}/tasks"),Some(request.clone()),false).await;assert_eq!(status,StatusCode::CREATED,"{task}");
    for (token,cap,root,expected) in [(CANDIDATE_TOKEN,CAPABILITY,"binding-root",StatusCode::OK),(CANDIDATE_TOKEN,CAPABILITY,"foreign",StatusCode::UNAUTHORIZED),(CANDIDATE_TOKEN,"wrong-capability-12345678901234","binding-root",StatusCode::UNAUTHORIZED),(AUTHOR_TOKEN,CAPABILITY,"binding-root",StatusCode::UNAUTHORIZED)] {
        let response=application(&path).oneshot(Request::builder().uri("/api/tool-binding").header("authorization",format!("Bearer {token}")).header("x-novel-task-capability",cap).header("x-novel-root-thread",root).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(),expected);
        if expected==StatusCode::OK {let value:Value=serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();assert_eq!(value,json!({"projectId":project,"taskId":task["id"],"threadId":"binding-root","operation":"advisory","inputRevision":0,"output":null}));}
    }
    let connection=rusqlite::Connection::open(&path).unwrap();connection.execute("UPDATE story_tasks_v2 SET capability_revoked_at=1 WHERE task_id=?1",[task["id"].as_str().unwrap()]).unwrap();
    let response=application(&path).oneshot(Request::builder().uri("/api/tool-binding").header("authorization",format!("Bearer {CANDIDATE_TOKEN}")).header("x-novel-task-capability",CAPABILITY).header("x-novel-root-thread","binding-root").body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tool_binding_candidate_tracks_own_output_never_external_head() {
    use tower::ServiceExt;
    let directory=tempfile::tempdir().unwrap();let path=directory.path().join("binding-cas.sqlite3");
    let (_,created)=call(&path,"POST","/api/projects",Some(json!({"logline":"","medium":"novel","idempotencyKey":"project","title":"候选绑定"})),false).await;
    let project=created["project"]["id"].as_str().unwrap();let base=format!("/api/projects/{project}");
    let saves=format!("{base}/drafts/checkpoints");
    let (status,seed)=call(&path,"POST",&saves,Some(json!({"payload":{"chapterNumber":1,"title":"信","body":"林夕藏信。"},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"seed"})),false).await;
    assert_eq!(status,StatusCode::CREATED,"{seed}");
    let chapter=seed["chapterId"].as_str().unwrap();
    let expires=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+600;
    let (status,task)=call(&path,"POST",&format!("{base}/tasks"),Some(json!({"operation":"generateChapter","inputRevision":0,"threadId":"cas-root","target":{"chapterId":chapter,"chapterNumber":1,"scope":{"type":"wholeChapter"}},"authorIntent":"改写本章候选","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires})),false).await;
    assert_eq!(status,StatusCode::CREATED,"{task}");
    async fn binding(path:&std::path::Path)->Value {
        let response=application(path).oneshot(Request::builder().uri("/api/tool-binding").header("authorization",format!("Bearer {CANDIDATE_TOKEN}")).header("x-novel-task-capability",CAPABILITY).header("x-novel-root-thread","cas-root").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::OK);
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
    }
    let first=binding(&path).await;
    assert_eq!(first["output"],json!({"objectId":format!("draft-{chapter}"),"expectedCandidateRevisionId":seed["revisionId"]}));
    let mut save=json!({"taskId":task["id"],"threadId":"cas-root","expectedRevision":0,"expectedCandidateRevisionId":first["output"]["expectedCandidateRevisionId"],"idempotencyKey":"own","payload":{"chapterId":chapter,"chapterNumber":1,"title":"信","body":"林夕把信藏进衣袋。","completeness":"chapterComplete"}});
    let mut partial = save.clone();
    partial["payload"]["completeness"] = json!("chapterPartial");
    partial["idempotencyKey"] = json!("partial");
    let (status,checkpoint)=call(&path,"POST",&saves,Some(partial.clone()),true).await;
    assert_eq!(status,StatusCode::CREATED,"{checkpoint}");
    assert_eq!(checkpoint["completeness"],"chapterPartial");
    let (status,incomplete)=call(&path,"POST",&format!("{base}/tasks/{}/complete",task["id"].as_str().unwrap()),Some(json!({"threadId":"cas-root","summary":"done"})),true).await;
    assert_eq!(status,StatusCode::CONFLICT,"{incomplete}");
    assert_eq!(incomplete["error"]["code"],"task_output_incomplete");
    assert_eq!(incomplete["error"]["details"]["savedOutputPreserved"],true);
    save["expectedCandidateRevisionId"] = checkpoint["revisionId"].clone();
    let (status,own)=call(&path,"POST",&saves,Some(save.clone()),true).await;
    assert_eq!(status,StatusCode::CREATED,"{own}");
    assert_eq!(own["completeness"],"chapterComplete");
    let (status,replay)=call(&path,"POST",&saves,Some(partial),true).await;
    assert_eq!(status,StatusCode::CREATED,"{replay}");
    assert_eq!(replay["revisionId"],checkpoint["revisionId"]);
    assert_eq!(replay["completeness"],"chapterPartial");
    assert_eq!(binding(&path).await["output"]["expectedCandidateRevisionId"],own["revisionId"]);
    let (status,external)=call(&path,"POST",&saves,Some(json!({"expectedRevision":0,"expectedCandidateRevisionId":own["revisionId"],"idempotencyKey":"external","payload":{"chapterId":chapter,"chapterNumber":1,"title":"信","body":"作者自行修改这一句。"}})),false).await;
    assert_eq!(status,StatusCode::CREATED,"{external}");
    let after=binding(&path).await;
    assert_eq!(after["output"]["expectedCandidateRevisionId"],own["revisionId"]);
    assert_ne!(after["output"]["expectedCandidateRevisionId"],external["revisionId"]);
    save["expectedCandidateRevisionId"]=after["output"]["expectedCandidateRevisionId"].clone();save["idempotencyKey"]=json!("must-conflict");
    let (status,rejected)=call(&path,"POST",&saves,Some(save),true).await;
    assert!(status.is_client_error(),"{status}: {rejected}");
    let (_,workspace)=call(&path,"GET",&format!("{base}/creative-workspace"),None,false).await;
    assert_eq!(workspace["chapters"][0]["candidate"]["id"],external["revisionId"]);
}

use novel_codex_story_core::{TasksRepository};
