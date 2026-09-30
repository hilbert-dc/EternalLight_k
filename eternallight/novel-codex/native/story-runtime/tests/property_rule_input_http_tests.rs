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

const AUTHOR: &str = "rule-page-author-token-1234567890123456";
const CANDIDATE: &str = "rule-page-candidate-token-1234567890123";
const CAPABILITY: &str = "rule-page-capability-1234567890123456";

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {token}"))
                .header("x-novel-task-capability", CAPABILITY)
                .header("content-type", "application/json")
                .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn escaped_unicode_rule_payload_is_reassembled_without_loss_and_cursors_cannot_cross_tasks() {
    let directory = tempfile::tempdir().unwrap();
    let app = router(task_fixture::state(
        directory.path().join("分页.sqlite3"),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (status, created) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        Some(json!({"title":"完整规则输入"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let base = format!(
        "/api/projects/{}",
        created["project"]["id"].as_str().unwrap()
    );
    let payload = json!({"eventRules":[],"definitions":[{"predicate":"long-rule","subjectKind":"character",
        "schema":{"cardinality":"single","valueType":{"type":"text"}},
        "description":format!("世界🙂{}","\u{0001}".repeat(498)),
        "exceptions":(0..8).map(|index|format!("条件{index}{}","\u{0001}".repeat(240))).collect::<Vec<_>>() }]});
    let wire = payload.to_string();
    assert!(wire.len() > 12_000 && wire.len() <= 16 * 1024);
    let (status, saved) = call(
        &app,
        "POST",
        &format!("{base}/artifacts/propertyRules"),
        AUTHOR,
        Some(json!({"payload":payload,
        "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"rule-base"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let create = json!({"operation":"generatePropertyRules","inputRevision":0,"threadId":"root","target":{"type":"artifact"},
        "authorIntent":"调整规则而非采用","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires});
    let (status, task) = call(
        &app,
        "POST",
        &format!("{base}/tasks"),
        AUTHOR,
        Some(create.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let id = task["id"].as_str().unwrap();
    let input = format!("{base}/tasks/{id}/property-rule-input?threadId=root&expectedRevision=0");
    assert_eq!(
        call(&app, "GET", &input, AUTHOR, /*body*/ None).await.0,
        StatusCode::FORBIDDEN
    );
    let child = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .uri(&input)
                .header("authorization", format!("Bearer {CANDIDATE}"))
                .header("x-novel-task-capability", CAPABILITY)
                .header("x-novel-tool-name", "read_property_rules")
                .header("x-novel-root-thread", "root")
                .header("x-novel-caller-thread", "child")
                .header("x-novel-parent-thread", "root")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(!child.status().is_success());
    let (status,context)=call(&app,"GET",&format!("{base}/context?taskId={id}&threadId=root&projectRevision=0&purpose=planner&budget=24&byteBudget=60000"),CANDIDATE,/*body*/None).await;
    assert_eq!(status, StatusCode::OK, "{context}");
    let descriptor = &context["operationContext"]["propertyRulesBase"];
    assert_eq!(descriptor["id"], saved["revisionId"]);
    assert!(descriptor.get("payload").is_none());
    let mut cursor = None;
    let mut combined = String::new();
    let mut pages = Vec::new();
    loop {
        let uri = cursor.as_ref().map_or_else(
            || input.clone(),
            |cursor: &String| format!("{input}&cursor={cursor}"),
        );
        let (status, page) = call(&app, "GET", &uri, CANDIDATE, /*body*/ None).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(
            call(&app, "GET", &uri, CANDIDATE, /*body*/ None).await,
            (StatusCode::OK, page.clone())
        );
        assert!(page.to_string().len() <= 7_000);
        let part = page["payloadPart"].as_str().unwrap();
        assert!(part.len() <= 2_800);
        assert_eq!(
            page["byteRange"],
            json!({"start":combined.len(),"end":combined.len()+part.len(),"total":wire.len()})
        );
        assert_eq!(page["contentHash"], descriptor["contentHash"]);
        combined.push_str(part);
        cursor = page["nextCursor"].as_str().map(str::to_owned);
        assert_eq!(page["endOfBaselineFromCursor"], json!(cursor.is_none()));
        pages.push(page);
        if cursor.is_none() {
            break;
        }
        assert!(pages.len() < 10);
    }
    assert!(pages.len() > 1);
    assert_eq!(combined, wire);
    assert_eq!(serde_json::from_str::<Value>(&combined).unwrap(), payload);
    let (status, recovery) = call(
        &app,
        "POST",
        &format!("{base}/tasks/recovery-snapshot"),
        AUTHOR,
        Some(json!({"taskIds":[id]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(recovery["operations"], json!([]));
    let (status, other) = call(&app, "POST", &format!("{base}/tasks"), AUTHOR, Some(create)).await;
    assert_eq!(status, StatusCode::CREATED);
    let cursor = pages[0]["nextCursor"].as_str().unwrap();
    let other_uri = format!(
        "{base}/tasks/{}/property-rule-input?threadId=root&expectedRevision=0&cursor={cursor}",
        other["id"].as_str().unwrap()
    );
    assert!(
        !call(&app, "GET", &other_uri, CANDIDATE, /*body*/ None)
            .await
            .0
            .is_success()
    );
    let (fence, _) = cursor.split_once(':').unwrap();
    for offset in [wire.len(), wire.len() + 1, wire.find('世').unwrap() + 1] {
        assert!(
            !call(
                &app,
                "GET",
                &format!("{input}&cursor={fence}:{offset}"),
                CANDIDATE,
                /*body*/ None
            )
            .await
            .0
            .is_success()
        );
    }
    let other_id = other["id"].as_str().unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{base}/tasks/{other_id}/cancel"),
            AUTHOR,
            Some(json!({}))
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            "GET",
            &format!(
                "{base}/tasks/{other_id}/property-rule-input?threadId=root&expectedRevision=0"
            ),
            CANDIDATE,
            /*body*/ None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status,changed)=call(&app,"POST",&format!("{base}/artifacts/propertyRules"),AUTHOR,Some(json!({"payload":{"eventRules":[],"definitions":[]},
        "expectedRevision":0,"expectedCandidateRevisionId":saved["revisionId"],"idempotencyKey":"external-change"}))).await;
    assert_eq!(status, StatusCode::CREATED, "{changed}");
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("{input}&cursor={cursor}"),
            CANDIDATE,
            /*body*/ None
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn initial_rule_task_reads_an_explicit_empty_payload_while_other_operations_are_denied() {
    let directory = tempfile::tempdir().unwrap();
    let app = router(task_fixture::state(
        directory.path().join("empty.sqlite3"),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (_, created) = call(
        &app,
        "POST",
        "/api/projects",
        AUTHOR,
        Some(json!({"title":"空基线"})),
    )
    .await;
    let base = format!(
        "/api/projects/{}",
        created["project"]["id"].as_str().unwrap()
    );
    for operation in ["generatePropertyRules", "generateOutline"] {
        let request = json!({"operation":operation,"inputRevision":0,"threadId":"root","target":{"type":"artifact"},"authorIntent":"生成候选",
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+600});
        let (status, task) = call(
            &app,
            "POST",
            &format!("{base}/tasks"),
            AUTHOR,
            Some(request),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        let (status, page) = call(
            &app,
            "GET",
            &format!(
                "{base}/tasks/{}/property-rule-input?threadId=root&expectedRevision=0",
                task["id"].as_str().unwrap()
            ),
            CANDIDATE,
            /*body*/ None,
        )
        .await;
        if operation == "generateOutline" {
            assert_eq!(status, StatusCode::FORBIDDEN);
        } else {
            assert_eq!(status, StatusCode::OK, "{page}");
            let payload = json!({"eventRules":[],"definitions":[]}).to_string();
            assert_eq!(
                page,
                json!({"revisionId":null,"source":null,"contentHash":format!("{:x}",Sha256::digest(payload.as_bytes())),
                "definitionCount":0,"eventRuleCount":0,"payloadEncoding":"jsonUtf8","byteRange":{"start":0,"end":payload.len(),"total":payload.len()},
                "payloadPart":payload,"nextCursor":null,"endOfBaselineFromCursor":true,"byteLimit":7000})
            );
        }
    }
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
