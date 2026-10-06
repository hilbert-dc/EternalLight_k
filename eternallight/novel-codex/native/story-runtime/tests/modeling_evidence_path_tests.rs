use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;

const AUTHOR: &str = "evidence-path-author-12345678901234567890";
const CANDIDATE: &str = "evidence-path-candidate-123456789012345";
const CAPABILITY: &str = "evidence-path-capability-12345678901234";

#[derive(Clone, Copy)]
enum Actor {
    Author,
    Candidate,
}

#[tokio::test]
async fn failed_evidence_identifies_original_field_without_partial_publication_then_can_be_corrected()
 {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("evidence-path.sqlite3");
    let (_, created) = call(
        &path,
        "POST",
        "/api/projects",
        Some(json!({"title":"信"})),
        Actor::Author,
    )
    .await;
    let root = format!(
        "/api/projects/{}",
        created["project"]["id"].as_str().unwrap()
    );
    let (status, draft) = call(&path, "POST", &format!("{root}/drafts/checkpoints"), Some(json!({
        "payload":{"chapterNumber":1,"title":"信","body":"林夕烧掉信件。张衡是林夕的父亲。林夕知道密函已送出。二十日后，监护关系解除。"},
        "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"evidence-draft"
    })), Actor::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let (status, batch) = call(&path, "POST", &format!("{root}/modeling-batches"), Some(json!({
        "objectId":format!("draft-{}",draft["chapterId"].as_str().unwrap()),
        "sourceRevisionId":draft["revisionId"],"expectedRevision":0,"extractorVersion":"evidence-path"
    })), Actor::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let (status, task) = call(&path, "POST", &format!("{root}/tasks"), Some(json!({
        "operation":"modelKnowledge","inputRevision":0,"threadId":"evidence-thread","target":{"batchId":batch["id"]},
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires,
        "authorIntent":"分析这版正文的已证实事实"
    })), Actor::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let valid = json!({
        "taskId":task["id"],"threadId":"evidence-thread","expectedRevision":0,
        "entities":[
            {"id":"linxi","kind":"character","displayName":"林夕","aliases":[],"evidence":{"quote":"林夕","start":0,"end":2}},
            {"id":"zhangheng","kind":"character","displayName":"张衡","aliases":[],"evidence":{"quote":"张衡"}},
            {"id":"letter-sent","kind":"proposition","displayName":"密函已送出","aliases":[],"evidence":{"quote":"密函已送出"}},
            {"id":"father-linxi","kind":"proposition","displayName":"张衡是林夕的父亲","aliases":[],"evidence":{"quote":"张衡是林夕的父亲"}}
        ],
        "assertions":[
            {"kind":"relation","subjectId":"zhangheng","predicate":"parentOf","relationType":"parentOf","targetId":"linxi",
             "propositionId":"father-linxi","value":true,"validFrom":1,"validUntilExclusive":20,
             "evidence":{"quote":"张衡是林夕的父亲"},"validUntilEvidence":{"quote":"监护关系解除"}},
            {"kind":"epistemic","subjectId":"letter-sent","predicate":"deliveryState","propositionId":"letter-sent",
             "epistemicOwnerId":"linxi","epistemicState":"known","epistemicTime":12,"epistemicUntilExclusive":20,"value":true,
             "evidence":{"quote":"知道密函已送出"},"epistemicUntilEvidence":{"quote":"监护关系解除"}}
        ]
    });
    let publish = format!(
        "{root}/modeling-batches/{}/publish",
        batch["id"].as_str().unwrap()
    );
    for (collection, index, field, quote, reason) in [
        (
            "entities",
            1,
            "evidence",
            "林夕",
            "evidence_quote_ambiguous",
        ),
        (
            "assertions",
            0,
            "evidence",
            "不存在的摘录",
            "evidence_quote_not_found",
        ),
        (
            "assertions",
            0,
            "validUntilEvidence",
            "林夕",
            "evidence_quote_ambiguous",
        ),
        (
            "assertions",
            1,
            "epistemicUntilEvidence",
            "不存在的摘录",
            "evidence_quote_not_found",
        ),
    ] {
        let mut invalid = valid.clone();
        invalid[collection][index][field] = json!({"quote":quote});
        let (status, rejected) =
            call(&path, "POST", &publish, Some(invalid), Actor::Candidate).await;
        assert_eq!(
            (status, rejected["error"].clone()),
            (
                StatusCode::BAD_REQUEST,
                json!({
                    "code":"invalid_request","details":{},"recovery":null,
                    "message":format!("InvalidContent(\"{collection}[{index}].{field}: {reason}\")")
                })
            )
        );
        let connection = rusqlite::Connection::open(&path).unwrap();
        let state: (String, String, String, i64, i64, i64) = connection.query_row(
            "SELECT status,entities_json,assertions_json,(SELECT count(*) FROM story_entity_claims_v2),
             (SELECT count(*) FROM story_assertion_claims_v1),(SELECT count(*) FROM story_knowledge_heads_v2)
             FROM story_modeling_batches_v2 WHERE batch_id=?1", [batch["id"].as_str().unwrap()],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)),
        ).unwrap();
        assert_eq!(state, ("queued".into(), "[]".into(), "[]".into(), 0, 0, 0));
    }
    let (status, committed) = call(&path, "POST", &publish, Some(valid), Actor::Candidate).await;
    assert_eq!(
        (
            status,
            committed["publicationState"].clone(),
            committed["status"].clone()
        ),
        (StatusCode::OK, json!("committed"), json!("ready"))
    );
    let connection = rusqlite::Connection::open(&path).unwrap();
    let counts: (i64, i64, i64) = connection.query_row(
        "SELECT (SELECT count(*) FROM story_entity_claims_v2),
         (SELECT count(*) FROM story_assertion_claims_v1),(SELECT count(*) FROM story_knowledge_heads_v2)", [],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).unwrap();
    assert_eq!(counts, (4, 2, 1));
}

async fn call(
    path: &std::path::Path,
    method: &str,
    uri: &str,
    payload: Option<Value>,
    actor: Actor,
) -> (StatusCode, Value) {
    let app = router(task_fixture::state(
        path.to_owned(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let mut request = Request::builder().method(method).uri(uri).header(
        "authorization",
        format!(
            "Bearer {}",
            match actor {
                Actor::Author => AUTHOR,
                Actor::Candidate => CANDIDATE,
            }
        ),
    );
    if let Actor::Candidate = actor {
        request = request.header("x-novel-task-capability", CAPABILITY);
    }
    let body = match payload {
        Some(payload) => {
            request = request.header("content-type", "application/json");
            Body::from(payload.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .author_fixture_request(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
