use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{ProjectId, SqliteStoryRepository};
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
#[path = "support/settle_task_fixture.rs"]
mod settle_fixture;

const AUTHOR: &str = "plan-first-author-token-123456789012";
const CANDIDATE: &str = "plan-first-candidate-token-123456789";
const CAPABILITY: &str = "plan-first-capability-123456789012";

enum Actor {
    Author,
    Candidate,
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    actor: Actor,
    body: Value,
) -> (StatusCode, Value) {
    let token = match actor {
        Actor::Author => AUTHOR,
        Actor::Candidate => CANDIDATE,
    };
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {token}"))
                .header("x-novel-task-capability", CAPABILITY)
                .header("content-type", "application/json")
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn authority(app: &Router, base: &str) -> Value {
    let (status, workspace) = call(
        app,
        "GET",
        &format!("{base}/creative-workspace"),
        Actor::Author,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{workspace}");
    json!({"revision":workspace["revision"],"snapshot":workspace["headSnapshotId"],
        "outline":workspace["artifacts"]["outline"],"plan":workspace["artifacts"]["plan"],
        "chapters":workspace["chapters"],"commits":workspace["commits"]})
}

#[tokio::test]
async fn conversation_first_plan_saves_against_unadopted_outline_with_exact_null_cas_and_same_work()
{
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("first-plan.sqlite3");
    let app = router(task_fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let (status, created) = call(
        &app,
        "POST",
        "/api/projects",
        Actor::Author,
        json!({"title":"潮汐信使","logline":"来信改变港城"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = created["project"]["id"].as_str().unwrap();
    let base = format!("/api/projects/{project}");
    let outline_payload = json!({"nodes":[{"id":"book","kind":"book","position":0,"title":"全书",
        "purpose":"理解预言代价","summary":"","dependencyKeys":[],"certainty":"tentative",
        "protection":"free","writerGuidance":"保留人物选择"}]});
    let (status, outline) = call(
        &app,
        "POST",
        &format!("{base}/artifacts/outline"),
        Actor::Author,
        json!({"payload":outline_payload,"expectedRevision":0,"expectedCandidateRevisionId":null,
            "idempotencyKey":"outline-source"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{outline}");
    let initial = authority(&app, &base).await;
    assert_eq!(
        json!({"outlineId":initial["outline"]["candidate"]["id"],
        "accepted":initial["outline"]["accepted"],"plan":initial["plan"]["candidate"],
        "chapters":initial["chapters"],"revision":initial["revision"]}),
        json!({"outlineId":outline["revisionId"],"accepted":null,"plan":null,"chapters":[],"revision":0})
    );
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let task_request = json!({"operation":"advisory","inputRevision":0,"threadId":"discussion",
        "target":{"type":"artifact","kind":"plan"},"authorIntent":"按已保存大纲规划前三章，保存建议，不采用或写正文",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":expires});
    let (status, parent) = call(
        &app,
        "POST",
        &format!("{base}/tasks"),
        Actor::Author,
        task_request.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{parent}");
    let parent_id = parent["id"].as_str().unwrap();
    let actions = format!("{base}/tasks/{parent_id}/actions");
    let (status, action) = call(&app, "POST", &actions, Actor::Candidate,
        json!({"taskId":parent_id,"threadId":"discussion","expectedRevision":0,"expectedIntentRevision":0,
            "idempotencyKey":"plan-action","operation":"generatePlan","target":{"type":"artifact"},
            "instruction":"只生成近章规划候选，保留大纲"})).await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    let action_id = action["id"].as_str().unwrap();
    let (status, complete) = call(
        &app,
        "POST",
        &format!("{base}/tasks/{parent_id}/complete"),
        Actor::Author,
        json!({"threadId":"discussion","summary":"已安排近章规划，尚未保存"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{complete}");
    settle_fixture::settle(&path, project, parent_id);
    let (status, approved) = call(
        &app,
        "POST",
        &format!("{actions}/{action_id}/approve"),
        Actor::Author,
        json!({"expectedIntentRevision":0}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    let mut target = action["target"].clone();
    target["requestedByTaskId"] = json!(parent_id);
    target["requestedActionId"] = json!(action_id);
    let mut request = task_request;
    request["operation"] = json!("generatePlan");
    request["threadId"] = json!("plan-worker");
    request["target"] = target;
    let (status, child) = call(
        &app,
        "POST",
        &format!("{base}/tasks"),
        Actor::Author,
        request,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{child}");
    let child_id = child["id"].as_str().unwrap();
    let manifest = &child["inputManifest"];
    assert_eq!(
        json!({"object":manifest["outputObjectId"],"candidate":manifest["expectedCandidateRevisionId"]}),
        json!({"object":"artifact-plan","candidate":null})
    );
    let repository = SqliteStoryRepository::open(&path).unwrap();
    let project_id = ProjectId::parse(project).unwrap();
    let work = repository
        .work_task_for_attempt(&project_id, parent_id)
        .unwrap()
        .id;
    assert_eq!(
        repository
            .work_task_for_attempt(&project_id, child_id)
            .unwrap()
            .id,
        work
    );
    drop(repository);
    let payload = json!({"sourceOutlineRevisionId":outline["revisionId"],"chapters":[{
        "chapterPlanId":"first-plan","chapterNumber":1,"primaryNodeId":"book","title":"落潮来信",
        "goal":"核实预言","scenes":[{"id":"receive","goal":"收到来信","beats":[],
            "expectedEffects":[{"id":"discover","dimension":"knowledge","description":"人物发现来信预言",
                "commitmentRefs":[]}]}]}]});
    let save_uri = format!("{base}/artifacts/plan");
    let save = json!({"taskId":child_id,"threadId":"plan-worker","expectedRevision":0,
        "expectedCandidateRevisionId":null,"idempotencyKey":"first-plan-save","payload":payload});
    for cas in ["null", "fictional-revision"] {
        let mut invalid = save.clone();
        invalid["expectedCandidateRevisionId"] = json!(cas);
        let (status, error) = call(&app, "POST", &save_uri, Actor::Candidate, invalid).await;
        assert_eq!(
            (status, error["error"]["code"].clone()),
            (StatusCode::CONFLICT, json!("candidate_revision_conflict"))
        );
        assert_eq!(authority(&app, &base).await, initial);
    }
    for payload in [json!({}), json!({"plans":[]}), json!({"chapterPlans":[]})] {
        let mut invalid = save.clone();
        invalid["payload"] = payload;
        let (status, error) = call(&app, "POST", &save_uri, Actor::Candidate, invalid).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
        assert_eq!(authority(&app, &base).await, initial);
    }
    let repository = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        repository
            .task(&project_id, child_id)
            .unwrap()
            .unwrap()
            .output_refs_json,
        "[]"
    );
    drop(repository);
    let (status, first) = call(&app, "POST", &save_uri, Actor::Candidate, save.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    assert_eq!(
        (first["version"].clone(), first["payload"].clone()),
        (json!(1), save["payload"].clone())
    );
    assert_eq!(
        call(&app, "POST", &save_uri, Actor::Candidate, save.clone()).await,
        (StatusCode::CREATED, first.clone())
    );
    let saved_authority = authority(&app, &base).await;
    let mut second = save.clone();
    second["idempotencyKey"] = json!("plan-second-save");
    second["payload"]["chapters"][0]["goal"] = json!("核实预言并决定回信");
    let (status, stale) = call(&app, "POST", &save_uri, Actor::Candidate, second.clone()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale}");
    assert_eq!(authority(&app, &base).await, saved_authority);
    second["expectedCandidateRevisionId"] = first["revisionId"].clone();
    let (status, revised) = call(&app, "POST", &save_uri, Actor::Candidate, second.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{revised}");
    assert_eq!(
        (revised["version"].clone(), revised["payload"].clone()),
        (json!(2), second["payload"].clone())
    );
    assert_eq!(
        call(&app, "POST", &save_uri, Actor::Candidate, second).await,
        (StatusCode::CREATED, revised.clone())
    );
    assert_eq!(
        call(&app, "POST", &save_uri, Actor::Candidate, save).await,
        (StatusCode::CREATED, first)
    );
    let (status, completed) = call(
        &app,
        "POST",
        &format!("{base}/tasks/{child_id}/complete"),
        Actor::Candidate,
        json!({"threadId":"plan-worker","summary":"近章规划候选已保存，未采用或写正文"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    settle_fixture::settle(&path, project, child_id);
    let (status, finished) = call(
        &app,
        "POST",
        &format!("{actions}/{action_id}/finish"),
        Actor::Author,
        json!({"resultTaskId":child_id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{finished}");
    let final_state = authority(&app, &base).await;
    assert_eq!(
        json!({"revision":final_state["revision"],"outline":final_state["outline"],
        "planId":final_state["plan"]["candidate"]["id"],"planAccepted":final_state["plan"]["accepted"],
        "chapters":final_state["chapters"]}),
        json!({"revision":0,"outline":initial["outline"],
        "planId":revised["revisionId"],"planAccepted":null,"chapters":[]})
    );
    let database = rusqlite::Connection::open(&path).unwrap();
    let counts: (i64, i64, i64) = database
        .query_row(
            "SELECT (SELECT COUNT(*) FROM story_task_outputs_v2 WHERE task_id=?1),
            (SELECT COUNT(*) FROM story_revisions_v2 WHERE project_id=?2 AND object_kind='plan'),
            (SELECT COUNT(*) FROM story_chapters_v2 WHERE project_id=?2)",
            [child_id, project],
            |row| {
                Ok((
                    row.get(/*idx*/ 0)?,
                    row.get(/*idx*/ 1)?,
                    row.get(/*idx*/ 2)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(counts, (2, 2, 0));
}
