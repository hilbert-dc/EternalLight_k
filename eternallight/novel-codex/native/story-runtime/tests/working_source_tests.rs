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

const AUTHOR: &str = "working-source-author-token-1234567890";
const CANDIDATE: &str = "working-source-candidate-token-123456";
const CAPABILITY: &str = "working-source-capability-123456789";

#[tokio::test]
async fn selected_historical_draft_is_frozen_read_and_advanced_by_writer_checkpoints() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("working-source.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"港城"})),
        false,
    )
    .await;
    let project = created["project"]["id"].as_str().unwrap();
    let root = format!("/api/projects/{project}");
    let drafts = format!("{root}/drafts/checkpoints");
    let (status, original) = call(
        &path,
        "POST",
        &drafts,
        Some(json!({
            "payload":{"chapterNumber":1,"title":"白塔","body":"林夕藏起密函。"},
            "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"original"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{original}");
    let chapter = original["chapterId"].as_str().unwrap();
    let (status, newer) = call(&path,"POST",&drafts,Some(json!({
        "payload":{"chapterId":chapter,"chapterNumber":1,"title":"白塔","body":"林夕烧掉密函。"},
        "expectedRevision":0,"expectedCandidateRevisionId":original["revisionId"],"idempotencyKey":"newer"
    })),false).await;
    assert_eq!(status, StatusCode::CREATED, "{newer}");
    let baseline_uri = format!("{root}/working-baselines/draft");
    let (status, selected) = call(
        &path,
        "PUT",
        &baseline_uri,
        Some(json!({
            "objectId":chapter,"revisionId":original["revisionId"],"expectedSelectionRevision":0
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{selected}");
    assert_eq!(selected["selectionRevision"], 1);
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, task) = call(&path,"POST",&format!("{root}/tasks"),Some(json!({
        "operation":"generateChapter","inputRevision":0,"threadId":"selected-writer",
        "target":{"type":"chapter","chapterId":chapter,"chapterNumber":1,"readingChapter":1,"sceneOrder":1},
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
        "capabilityExpiresAt":expires,"authorIntent":"从藏信稿继续"
    })),false).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let context_uri = format!(
        "{root}/context?taskId={task_id}&threadId=selected-writer&projectRevision=0&purpose=writer&readingChapter=1&sceneOrder=1&budget=24&byteBudget=60000"
    );
    let (status, context) = call(&path, "GET", &context_uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let relevant_text = read_section(&path, &root, &task, &context, "/relevantText").await;
    assert_eq!(relevant_text[0]["sourceRevisionId"], original["revisionId"]);
    assert_eq!(
        relevant_text[0]["content"]["sceneGroups"][0]["body"],
        "林夕藏起密函。"
    );
    assert_eq!(relevant_text[0]["authority"], "working");
    assert_eq!(relevant_text[0]["workingSelectionRevision"], 1);
    let modeling = read_section(&path, &root, &task, &context, "/operationContext/modeling").await;
    assert_eq!(modeling[0]["sourceRevisionId"], original["revisionId"]);
    assert_eq!(modeling[0]["status"], "model_pending");
    let knowledge = read_section(
        &path,
        &root,
        &task,
        &context,
        "/operationContext/workingKnowledge",
    )
    .await;
    assert_eq!(knowledge["coverage"]["sourceModelingIncomplete"], true);
    let query_uri = format!("{root}/tasks/{task_id}/query");
    let (status, report) = call(
        &path,
        "POST",
        &query_uri,
        Some(json!({
            "threadId":"selected-writer","expectedRevision":0,
            "plan":{
                "question":"林夕对密函做了什么？","byteBudget":9000,
                "ranking":{"type":"literal"},
                "steps":[
                    {"id":"passage","operation":{"type":"passages","cursor":null,"maxChars":160,"limit":2}},
                    {"id":"text","operation":{"type":"text","query":"藏起密函","limit":2}}
                ]
            }
        })),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(
        report["steps"][0]["results"][0]["sourceRevisionId"],
        original["revisionId"]
    );
    assert_eq!(
        report["steps"][1]["results"][0]["sourceRevisionId"],
        original["revisionId"]
    );
    let (status, unmodeled) = call(
        &path,
        "POST",
        &query_uri,
        Some(json!({
            "threadId":"selected-writer","expectedRevision":0,
            "plan":{
                "question":"林夕当前知道什么？","byteBudget":9000,
                "ranking":{"type":"literal"},
                "steps":[{"id":"state","operation":{"type":"state","entityId":"linxi","perspective":"world"}}]
            }
        })),
        true,
    )
    .await;
    assert!(status.is_client_error(), "{unmodeled}");
    let before_digest = context["sourceWatermarks"]["digest"].clone();
    let save = |body: &str, previous: Value, key: &str| {
        json!({
            "payload":{"chapterId":chapter,"chapterNumber":1,"title":"白塔","body":body},
            "expectedRevision":0,"expectedCandidateRevisionId":previous,"idempotencyKey":key,
            "taskId":task_id,"threadId":"selected-writer"
        })
    };
    let (status, first) = call(
        &path,
        "POST",
        &drafts,
        Some(save(
            "林夕把密函藏进灯座。",
            newer["revisionId"].clone(),
            "writer-first",
        )),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let database = rusqlite::Connection::open(&path).unwrap();
    let first_base: String = database.query_row(
        "SELECT working_base_revision_id FROM story_task_outputs_v2 WHERE task_id=?1 AND ordinal=1",
        [task_id],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(first_base, original["revisionId"].as_str().unwrap());
    let refs: String = database
        .query_row(
            "SELECT output_refs_json FROM story_tasks_v2 WHERE task_id=?1",
            [task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&refs).unwrap()[0]["workingBaseRevisionId"],
        original["revisionId"]
    );
    let (status, workspace) = call(
        &path,
        "GET",
        &format!("{root}/creative-workspace"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{workspace}");
    assert_eq!(
        workspace["chapters"][0]["workingSelection"]["revisionId"],
        first["revisionId"]
    );
    assert_eq!(
        workspace["chapters"][0]["workingSelection"]["selectionRevision"],
        2
    );
    let (status, refreshed) = call(&path, "GET", &context_uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{refreshed}");
    let refreshed_text = read_section(&path, &root, &task, &refreshed, "/relevantText").await;
    assert_eq!(
        refreshed["relevantText"]["sourceId"],
        context["relevantText"]["sourceId"]
    );
    assert_eq!(refreshed_text, relevant_text);
    assert_eq!(
        refreshed_text[0]["sourceRevisionId"],
        original["revisionId"]
    );
    let first_source =
        read_saved_checkpoint(&path, &root, &task, &refreshed, &first, 1, true).await;
    assert_eq!(first_source["textRepresentation"], "sceneGroups");
    assert!(first_source["content"].get("body").is_none());
    assert_eq!(first_source["content"]["sceneGroups"][0]["body"], "林夕把密函藏进灯座。");
    assert_ne!(refreshed["sourceWatermarks"]["digest"], before_digest);
    let (status, second) = call(
        &path,
        "POST",
        &drafts,
        Some(save(
            "林夕记下灯座的刻痕。",
            first["revisionId"].clone(),
            "writer-second",
        )),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{second}");
    let second_base: String = database.query_row(
        "SELECT working_base_revision_id FROM story_task_outputs_v2 WHERE task_id=?1 AND ordinal=2",
        [task_id],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(second_base, first["revisionId"].as_str().unwrap());
    let (status, after_second) = call(&path, "GET", &context_uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{after_second}");
    assert_eq!(
        after_second["relevantText"]["sourceId"],
        context["relevantText"]["sourceId"]
    );
    assert_eq!(
        read_section(&path, &root, &task, &after_second, "/relevantText").await,
        relevant_text
    );
    let second_source =
        read_saved_checkpoint(&path, &root, &task, &after_second, &second, 2, true).await;
    assert_eq!(second_source["textRepresentation"], "sceneGroups");
    assert!(second_source["content"].get("body").is_none());
    assert_eq!(second_source["content"]["sceneGroups"][0]["body"], "林夕记下灯座的刻痕。");
    let historical_first =
        read_saved_checkpoint(&path, &root, &task, &after_second, &first, 1, false).await;
    assert_eq!(historical_first, first_source);

    let (status, switch) = call(
        &path,
        "PUT",
        &baseline_uri,
        Some(json!({
            "objectId":chapter,"revisionId":newer["revisionId"],"expectedSelectionRevision":3
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{switch}");
    let (status, stale) = call(&path, "GET", &context_uri, None, true).await;
    assert!(status.is_client_error(), "{stale}");
    let (status, late) = call(
        &path,
        "POST",
        &drafts,
        Some(save(
            "不应写入的迟到正文。",
            second["revisionId"].clone(),
            "writer-late",
        )),
        true,
    )
    .await;
    assert!(status.is_client_error(), "{late}");
    let (_, after) = call(
        &path,
        "GET",
        &format!("{root}/creative-workspace"),
        None,
        false,
    )
    .await;
    assert_eq!(
        after["chapters"][0]["candidate"]["id"],
        second["revisionId"]
    );
    assert_eq!(
        after["chapters"][0]["workingSelection"]["revisionId"],
        newer["revisionId"]
    );
    assert_eq!(
        after["chapters"][0]["workingSelection"]["selectionRevision"],
        4
    );
}

#[tokio::test]
async fn planner_uses_selected_artifact_and_a_changed_selection_invalidates_its_task() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("working-artifact.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"港城"})),
        false,
    )
    .await;
    let root = format!(
        "/api/projects/{}",
        created["project"]["id"].as_str().unwrap()
    );
    let brief_uri = format!("{root}/artifacts/brief");
    let (status, first) = call(
        &path,
        "POST",
        &brief_uri,
        Some(json!({
            "payload":{"premise":"港城遗忘海名"},"expectedRevision":0,
            "expectedCandidateRevisionId":null,"idempotencyKey":"first-brief"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let (status, latest) = call(
        &path,
        "POST",
        &brief_uri,
        Some(json!({
            "payload":{"premise":"港城记住海名"},"expectedRevision":0,
            "expectedCandidateRevisionId":first["revisionId"],"idempotencyKey":"latest-brief"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{latest}");
    let baseline_uri = format!("{root}/working-baselines/brief");
    let (status, _) = call(
        &path,
        "PUT",
        &baseline_uri,
        Some(json!({
            "revisionId":first["revisionId"],"expectedSelectionRevision":0
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let (status, task) = call(&path,"POST",&format!("{root}/tasks"),Some(json!({
        "operation":"advisory","inputRevision":0,"threadId":"selected-planner","target":{"type":"project"},
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),
        "capabilityExpiresAt":expires,"authorIntent":"从选定简报分析故事"
    })),false).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let uri = format!(
        "{root}/context?taskId={}&threadId=selected-planner&projectRevision=0&purpose=planner&budget=24&byteBudget=60000",
        task["id"].as_str().unwrap()
    );
    let (status, context) = call(&path, "GET", &uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let brief = read_section(&path, &root, &task, &context, "/narrativeGoals/brief").await;
    assert_eq!(brief["premise"], "港城遗忘海名");
    assert_eq!(brief["sourceRevisionId"], first["revisionId"]);
    assert_eq!(brief["workingSelectionRevision"], 1);
    let (status, _) = call(
        &path,
        "DELETE",
        &baseline_uri,
        Some(json!({
            "expectedSelectionRevision":1
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, stale) = call(&path, "GET", &uri, None, true).await;
    assert!(status.is_client_error(), "{stale}");
    let (status, author_context) = call(
        &path,
        "GET",
        &format!("{root}/context?projectRevision=0&purpose=planner&budget=24&byteBudget=60000"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{author_context}");
    assert_eq!(
        author_context["narrativeGoals"]["brief"]["premise"],
        "港城记住海名"
    );
    assert_eq!(
        author_context["narrativeGoals"]["brief"]["sourceRevisionId"],
        latest["revisionId"]
    );
}

#[tokio::test]
async fn writer_and_critic_context_reference_long_sources_and_return_full_bounded_pages() {
    for repeats in [110, 500] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("writer-context.sqlite3");
        let (_, created) = call(
            &path,
            "POST",
            "/api/projects",
            Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"迟到的钟"})),
            false,
        )
        .await;
        let root = format!(
            "/api/projects/{}",
            created["project"]["id"].as_str().unwrap()
        );
        let body = "沈遥合上父亲的修理记录，关掉手电，听见检修梯上有人走来。".repeat(repeats);
        let groups = json!([{"id":"frozen-scene","title":"站台","body":body,"locked":false}]);
        let (status, saved) = call(&path, "POST", &format!("{root}/drafts/checkpoints"), Some(json!({
            "payload":{"chapterNumber":1,"title":"最后一分钟","body":body,"sceneGroups":groups},
            "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"long-chapter"
        })), false).await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;
        let (status, task) = call(&path, "POST", &format!("{root}/tasks"), Some(json!({
            "operation":"generateChapter","inputRevision":0,"threadId":"long-writer",
            "target":{"type":"chapter","chapterId":saved["chapterId"],"chapterNumber":1,"readingChapter":1,"sceneOrder":1},
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires,
            "authorIntent":"保留已有正文，只写这一刻"
        })), false).await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        let uri = format!(
            "{root}/context?taskId={}&threadId=long-writer&projectRevision=0&purpose=writer&readingChapter=1&sceneOrder=1&budget=24&byteBudget=60000",
            task["id"].as_str().unwrap()
        );
        let (status, context) = call(&path, "GET", &uri, None, true).await;
        assert_eq!(status, StatusCode::OK, "{context}");
        assert!(!context.to_string().contains(&body));
        assert_model_item_bound(&context);
        let draft = read_workspace_draft(&path, &root, &task, &context, &saved).await;
        assert_eq!(draft["sourceRevisionId"], saved["revisionId"]);
        assert_eq!(draft["content"]["sceneGroups"], groups);
        assert_eq!(draft["textRepresentation"], "sceneGroups");
        assert!(draft["content"].get("body").is_none());
        assert_eq!(groups.as_array().unwrap().iter().map(|g|g["body"].as_str().unwrap()).collect::<Vec<_>>().join("\n\n---\n\n"), body);
        assert_eq!(draft["authority"], "frozenWriterInput");
        let (status, batch) = call(&path,"POST",&format!("{root}/modeling-batches"),Some(json!({
            "objectId":format!("draft-{}",saved["chapterId"].as_str().unwrap()),"sourceRevisionId":saved["revisionId"],
            "expectedRevision":0,"extractorVersion":"current-context-contract"
        })),false).await;
        assert_eq!(status, StatusCode::CREATED, "{batch}");
        let (status, knowledge_task) = call(&path,"POST",&format!("{root}/tasks"),Some(json!({
            "operation":"modelKnowledge","inputRevision":0,"threadId":"long-critic","target":{"batchId":batch["id"]},
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires,
            "authorIntent":"只分析已绑定的完整正文，不修改章节"
        })),false).await;
        assert_eq!(status, StatusCode::CREATED, "{knowledge_task}");
        let critic_uri = format!(
            "{root}/context?taskId={}&threadId=long-critic&projectRevision=0&purpose=critic&budget=24&byteBudget=60000",
            knowledge_task["id"].as_str().unwrap()
        );
        let (status, critic) = call(&path, "GET", &critic_uri, None, true).await;
        assert_eq!(status, StatusCode::OK, "{critic}");
        assert!(!critic.to_string().contains(&body));
        assert_model_item_bound(&critic);
        let source = read_workspace_draft(&path, &root, &knowledge_task, &critic, &saved).await;
        assert_eq!(source["sourceRevisionId"], saved["revisionId"]);
        assert_eq!(source["content"]["sceneGroups"], groups);
        assert_eq!(source["textRepresentation"], "sceneGroups");
        assert!(source["content"].get("body").is_none());
        let modeling_batch = read_section(
            &path,
            &root,
            &knowledge_task,
            &critic,
            "/operationContext/modelingBatch",
        )
        .await;
        assert_eq!(modeling_batch["sourceRevisionId"], saved["revisionId"]);
        let (status, workspace) = call(
            &path,
            "GET",
            &format!("{root}/creative-workspace"),
            None,
            false,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(workspace["chapters"][0]["candidate"]["body"], body);
        assert!(workspace["chapters"][0]["accepted"].is_null());
    }
}

fn assert_model_item_bound(value: &Value) {
    let result = json!({"content":[{"type":"text","text":value.to_string()}]});
    let item = json!({"type":"function_call_output","call_id":"x".repeat(160),"output":result.to_string()});
    let encoded = item.to_string();
    // Match current Engine admission on the actual doubly encoded MCP item.
    // Route token capacity is enforced for the complete request, not per item.
    assert!(encoded.len() <= 256 * 1024 - 1024, "model item lacks envelope headroom");
    eternallight_engine_core::dispatch(
        "tokens.bound",
        json!({"value":encoded,"label":"task source model item"}),
    ).expect("source page must satisfy production item admission");
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

async fn read_workspace_draft(
    path: &std::path::Path,
    root: &str,
    task: &Value,
    context: &Value,
    saved: &Value,
) -> Value {
    let directory_id = context["taskWorkspace"]["sourceId"].as_str().unwrap();
    let uri = format!(
        "{root}/tasks/{}/task-source-content?threadId={}&expectedRevision={}&sourceId={directory_id}&view=workspace&chapterNumber=1&objectKind=draft",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"]
    );
    let (status, directory) = call(path, "GET", &uri, None, true).await;
    assert_eq!(status, StatusCode::OK, "{directory}");
    assert_eq!(directory["representation"], "workspaceDirectory");
    assert_eq!(directory["contentRead"], false);
    assert!(directory.to_string().len() <= 6500);
    assert_model_item_bound(&directory);
    let entries = directory["items"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "{directory}");
    assert_eq!(entries[0]["revisionId"], saved["revisionId"]);
    assert_eq!(
        entries[0]["objectId"],
        format!("draft-{}", saved["chapterId"].as_str().unwrap())
    );
    let source = read_content(
        path,
        root,
        task,
        entries[0]["sourceId"].as_str().unwrap(),
        None,
    )
    .await;
    assert_eq!(source["sourceRevisionId"], entries[0]["revisionId"]);
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
        assert!(page.to_string().len() <= novel_codex_story_core::SOURCE_CONTENT_PAGE_BYTES);
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
    candidate: bool,
) -> (StatusCode, Value) {
    let app: Router = router(task_fixture::state(
        path.to_owned(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let mut request = Request::builder().method(method).uri(uri).header(
        "authorization",
        format!("Bearer {}", if candidate { CANDIDATE } else { AUTHOR }),
    );
    if candidate {
        request = request.header("x-novel-task-capability", CAPABILITY);
    }
    let body = if let Some(payload) = payload {
        request = request.header("content-type", "application/json");
        Body::from(payload.to_string())
    } else {
        Body::empty()
    };
    let response = app
        .author_fixture_request(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("non-JSON response: {}", String::from_utf8_lossy(&bytes)));
    (status, value)
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
