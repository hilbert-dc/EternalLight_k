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

const AUTHOR: &str = "rules-author-token-1234567890123456789";
const CANDIDATE: &str = "rules-candidate-token-123456789012345";
const CAPABILITY: &str = "rules-task-capability-123456789012345";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("x-novel-task-capability", CAPABILITY)
        .header("content-type", "application/json");
    if token == CANDIDATE && method == "POST" && uri.ends_with("/artifacts/propertyRules") {
        let request = body.as_ref().unwrap();
        let thread = request["threadId"].as_str().unwrap();
        for (name, value) in [
            ("x-novel-tool-name", "save_property_rules"),
            (
                "x-novel-tool-call",
                request["idempotencyKey"].as_str().unwrap(),
            ),
            ("x-novel-root-thread", thread),
            ("x-novel-caller-thread", thread),
            ("x-novel-turn", "rule-turn"),
        ] {
            builder = builder.header(name, value);
        }
        builder = builder.header(
            "x-novel-request-hash",
            format!(
                "{:x}",
                Sha256::digest(
                    json!({"tool":"save_property_rules","arguments":request})
                        .to_string()
                        .as_bytes()
                )
            ),
        );
    }
    let response = app
        .clone()
        .author_fixture_request(
            builder
                .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn dialogue_rules_keep_frozen_baseline_candidate_origin_and_author_approval_separate() {
    let directory = tempfile::tempdir().unwrap();
    let app = router(task_fixture::state(
        directory.path().join("对话规则.sqlite3"),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (status, created) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"灯城规则"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project}");
    let rules = format!("{base}/artifacts/propertyRules");
    let state = format!("{base}/knowledge/state?perspective=world");
    let event_rule = json!({"eventType":"departure","description":"行动者须在世","exceptions":[],
        "condition":{"type":"all","conditions":[{"type":"not","condition":{"type":"state","role":"actor",
            "subjectKind":"character","predicate":"alive","quantifier":"all","operator":"eq","operand":{"type":"boolean","value":false}}}]}});
    let payload = json!({"eventRules":[event_rule],"definitions":[{"predicate":"alive","subjectKind":"character",
        "schema":{"cardinality":"single","valueType":{"type":"boolean"}},"description":"人物生存状态","exceptions":[]}]});
    let (status, prior) = call(
        &app,
        "POST",
        &rules,
        AUTHOR,
        Some(json!({"payload":payload,
        "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"author-base"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{prior}");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let mut create = json!({"operation":"advisory","inputRevision":0,"threadId":"discussion","target":{"type":"project"},
        "authorIntent":"生成生存规则提案，不自动采用","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires});
    let (status, discussion) = call(
        &app,
        "POST",
        &format!("{base}/tasks"),
        AUTHOR,
        Some(create.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{discussion}");
    let root = discussion["id"].as_str().unwrap();
    let (status, denied) = call(&app, "POST", &rules, CANDIDATE, Some(json!({"taskId":root,"threadId":"discussion",
        "payload":payload,"expectedRevision":0,"expectedCandidateRevisionId":prior["revisionId"],"idempotencyKey":"wrong-task"}))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{denied}");
    let action_request = json!({"taskId":root,"threadId":"discussion","expectedRevision":0,"expectedIntentRevision":0,"operation":"generatePropertyRules",
        "target":{"type":"artifact"},"instruction":"保存规则候选","idempotencyKey":"rules-action"});
    let action_uri = format!("{base}/tasks/{root}/actions");
    let (status, action) = call(
        &app,
        "POST",
        &action_uri,
        CANDIDATE,
        Some(action_request.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    assert_eq!(
        call(&app, "POST", &action_uri, CANDIDATE, Some(action_request)).await,
        (StatusCode::CREATED, action.clone())
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{base}/tasks/{root}/complete"),
            AUTHOR,
            Some(json!({"threadId":"discussion","summary":"已登记"}))
        )
        .await
        .0,
        StatusCode::OK
    );
    settle_fixture::settle(&directory.path().join("对话规则.sqlite3"), project, root);
    let (status, approved) = call(
        &app,
        "POST",
        &format!("{action_uri}/{}/approve", action["id"].as_str().unwrap()),
        AUTHOR,
        Some(json!({"expectedIntentRevision":0})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    create["operation"] = json!("generatePropertyRules");
    create["threadId"] = json!("generator");
    create["target"] = action["target"].clone();
    create["target"]["requestedByTaskId"] = json!(root);
    create["target"]["requestedActionId"] = action["id"].clone();
    let (status, task) = call(&app, "POST", &format!("{base}/tasks"), AUTHOR, Some(create)).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let id = task["id"].as_str().unwrap();
    assert_eq!(task["requiredOutputKind"], "propertyRules");
    assert_eq!(
        task["inputManifest"]["expectedCandidateRevisionId"],
        prior["revisionId"]
    );
    let context_uri = format!(
        "{base}/context?taskId={id}&threadId=generator&projectRevision=0&purpose=planner&budget=24&byteBudget=60000"
    );
    let (status, context) = call(&app, "GET", &context_uri, CANDIDATE, /*body*/ None).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let source = context["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["sectionPath"] == "/operationContext/propertyRulesBase")
        .unwrap();
    let page_uri = format!(
        "{base}/tasks/{id}/task-source?threadId=generator&expectedRevision=0&sourceId={}",
        source["sourceId"].as_str().unwrap()
    );
    let (status, page) = call(&app, "GET", &page_uri, CANDIDATE, /*body*/ None).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let original: Value = serde_json::from_str(page["payloadPart"].as_str().unwrap()).unwrap();
    assert_eq!(original["payload"], payload);
    assert_eq!(original["id"], prior["revisionId"]);
    assert_eq!(page["contentHash"], source["contentHash"]);
    assert_eq!(page["nextCursor"], Value::Null);
    let complete_uri = format!("{base}/tasks/{id}/complete");
    let complete = json!({"threadId":"generator","expectedRevision":0,"summary":"候选已保存"});
    assert_eq!(
        call(
            &app,
            "POST",
            &complete_uri,
            CANDIDATE,
            Some(complete.clone())
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let mut next_payload = payload.clone();
    next_payload["definitions"][0]["description"] = json!("明确记录角色的生存状态，不由规划推断");
    let mut invalid_payload = next_payload.clone();
    invalid_payload["eventRules"][0]["condition"]["conditions"][0]["condition"]["operator"] =
        json!("gt");
    let (status, rejected) = call(&app, "POST", &rules, CANDIDATE, Some(json!({"taskId":id,"threadId":"generator",
        "expectedRevision":0,"payload":invalid_payload,"expectedCandidateRevisionId":prior["revisionId"],"idempotencyKey":"invalid-expression"}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    let submit = json!({"taskId":id,"threadId":"generator","expectedRevision":0,"payload":next_payload,
        "expectedCandidateRevisionId":prior["revisionId"],"idempotencyKey":"generated-rules"});
    let (status, saved) = call(&app, "POST", &rules, CANDIDATE, Some(submit.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    assert_eq!(saved["payload"]["eventRules"], json!([event_rule]));
    assert_eq!(
        call(&app, "POST", &rules, CANDIDATE, Some(submit)).await,
        (StatusCode::CREATED, saved.clone())
    );
    let (status, recovery) = call(
        &app,
        "POST",
        &format!("{base}/tasks/recovery-snapshot"),
        AUTHOR,
        Some(json!({"taskIds":[id]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{recovery}");
    let operations = recovery["operations"].as_array().unwrap();
    let committed = operations
        .iter()
        .find(|entry| entry["tool_call_id"] == "generated-rules")
        .unwrap();
    assert_eq!(
        (&committed["tool_name"], &committed["state"]),
        (&json!("save_property_rules"), &json!("committed"))
    );
    let rejected = operations
        .iter()
        .find(|entry| entry["tool_call_id"] == "invalid-expression")
        .unwrap();
    assert_eq!(
        (&rejected["state"], &rejected["receipt_ref"]),
        (&json!("accepted"), &Value::Null)
    );
    assert_eq!(
        call(&app, "GET", &context_uri, CANDIDATE, /*body*/ None)
            .await
            .1["operationContext"]["propertyRulesBase"],
        context["operationContext"]["propertyRulesBase"]
    );
    assert_eq!(
        call(&app, "GET", &page_uri, CANDIDATE, /*body*/ None).await,
        (StatusCode::OK, page)
    );
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        AUTHOR,
        /*body*/ None,
    )
    .await;
    assert_eq!(
        workspace["artifacts"]["propertyRules"]["candidate"]["source"],
        "agent"
    );
    assert_eq!(
        workspace["artifacts"]["propertyRules"]["accepted"],
        Value::Null
    );
    assert_eq!(
        call(&app, "GET", &state, AUTHOR, /*body*/ None).await.1["propertyRulesRevisionId"],
        Value::Null
    );
    let review = json!({"artifactKind":"propertyRules","revisionId":saved["revisionId"],"proposals":[],"selections":{},"idempotencyKey":"rule-review"});
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{base}/reviews"),
            CANDIDATE,
            Some(review.clone())
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, completed) = call(&app, "POST", &complete_uri, CANDIDATE, Some(complete)).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["deliveryStatus"], "ready_for_review");
    let (status, review) = call(
        &app,
        "POST",
        &format!("{base}/reviews"),
        AUTHOR,
        Some(review),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let (status, receipt) = call(
        &app,
        "POST",
        &format!("{base}/reviews/{}/accept", review["id"].as_str().unwrap()),
        AUTHOR,
        Some(json!({"expectedRevision":0,"idempotencyKey":"rule-accept","selections":{}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let (_, adopted) = call(&app, "GET", &state, AUTHOR, /*body*/ None).await;
    assert_eq!(adopted["eventRuleCount"], 1);
    assert_eq!(adopted["eventConditionsPolicy"], "authorRulesSourceClaims");
    let (_, snapshot) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        AUTHOR,
        /*body*/ None,
    )
    .await;
    let frozen = snapshot["headSnapshotId"].as_str().unwrap();
    let (_, limited) = call(
        &app,
        "GET",
        &format!("{base}/knowledge/state?perspective=reader&readingChapter=1&sceneOrder=1"),
        AUTHOR,
        /*body*/ None,
    )
    .await;
    assert!(limited.get("eventRuleCount").is_none());
    assert!(limited.get("eventConditionsPolicy").is_none());
    assert_eq!(
        limited["propertyRulesPolicy"],
        "withheldForLimitedPerspective"
    );
    assert_eq!(
        call(&app, "GET", &state, AUTHOR, /*body*/ None).await.1["propertyRulesRevisionId"],
        saved["revisionId"]
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!(
                "{base}/commits/{}/undo",
                receipt["commitId"].as_str().unwrap()
            ),
            AUTHOR,
            Some(json!({"expectedRevision":1,"idempotencyKey":"rule-undo"}))
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "GET", &state, AUTHOR, /*body*/ None).await.1["propertyRulesRevisionId"],
        Value::Null
    );
    assert_eq!(
        call(&app, "GET", &state, AUTHOR, /*body*/ None).await.1["eventRuleCount"],
        0
    );
    let (status, historical) = call(
        &app,
        "GET",
        &format!("{state}&snapshotId={frozen}"),
        AUTHOR,
        /*body*/ None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical}");
    assert_eq!(historical["eventRuleCount"], 1);
    assert_eq!(historical["propertyRulesRevisionId"], saved["revisionId"]);
    let stale_request = json!({"operation":"generatePropertyRules","inputRevision":2,"threadId":"stale-generator","target":{"type":"artifact"},
        "authorIntent":"修改规则提案","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires});
    let (status, stale_task) = call(
        &app,
        "POST",
        &format!("{base}/tasks"),
        AUTHOR,
        Some(stale_request),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{stale_task}");
    let (status, author_edit) = call(&app, "POST", &rules, AUTHOR, Some(json!({"payload":payload,"expectedRevision":2,
        "expectedCandidateRevisionId":saved["revisionId"],"idempotencyKey":"concurrent-author-edit"}))).await;
    assert_eq!(status, StatusCode::CREATED, "{author_edit}");
    let late = json!({"taskId":stale_task["id"],"threadId":"stale-generator","expectedRevision":2,"payload":next_payload,
        "expectedCandidateRevisionId":saved["revisionId"],"idempotencyKey":"late-model"});
    assert_eq!(
        call(&app, "POST", &rules, CANDIDATE, Some(late)).await.0,
        StatusCode::CONFLICT
    );
    let (_, workspace) = call(
        &app,
        "GET",
        &format!("{base}/creative-workspace"),
        AUTHOR,
        /*body*/ None,
    )
    .await;
    assert_eq!(
        workspace["artifacts"]["propertyRules"]["candidate"]["id"],
        author_edit["revisionId"]
    );
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
#[path = "support/settle_task_fixture.rs"]
mod settle_fixture;
