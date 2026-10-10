use crate::{
    multimedia_api::{failure, open},
    server::{Actor, RuntimeState, candidate_task, error},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use novel_codex_story_core::{MediaOperationSeal, MediaTranscriptInput, PrepareMediaOperation};
use serde::Deserialize;
use serde_json::{Value, json};

pub(crate) fn author_routes() -> Router<RuntimeState> {
    Router::new()
        .route(
            "/api/projects/{project_id}/media-operations/{operation_id}",
            get(read_intent),
        )
        .route(
            "/api/projects/{project_id}/media-operations/{operation_id}/outputs",
            post(begin_outputs),
        )
        .route(
            "/api/projects/{project_id}/media-operations/{operation_id}/transcript",
            get(read_transcript).post(transcript),
        )
        .route(
            "/api/projects/{project_id}/media-operations/{operation_id}/seal",
            post(seal),
        )
}
pub(crate) fn writer_routes() -> Router<RuntimeState> {
    Router::new().route(
        "/api/projects/{project_id}/tasks/{task_id}/media/operations",
        post(task_operation),
    )
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    thread_id: String,
    expected_revision: i64,
    action: String,
    request: Option<PrepareMediaOperation>,
    operation_id: Option<String>,
    transcript_offset: Option<usize>,
    transcript_limit: Option<usize>,
}
async fn task_operation(
    State(s): State<RuntimeState>,
    Path((p, t)): Path<(String, String)>,
    Extension(actor): Extension<Actor>,
    headers: HeaderMap,
    Json(r): Json<Request>,
) -> Response {
    if !matches!(r.action.as_str(), "prepare" | "read" | "wait" | "cancel") {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_media_action",
            "媒体操作无效。",
        );
    }
    let intent = {
        let (mut db, project) = match open(&s, p.clone()) {
            Ok(v) => v,
            Err(e) => return e,
        };
        let task = match candidate_task(
            &db,
            actor,
            &headers,
            &project,
            Some(&t),
            Some(&r.thread_id),
            Some(r.expected_revision),
        ) {
            Ok(Some(t)) => t,
            Ok(None) => {
                return error(
                    StatusCode::FORBIDDEN,
                    "media_task_capability_required",
                    "媒体操作需要对应任务能力。",
                );
            }
            Err(e) => return *e,
        };
        let result = if r.action == "prepare" {
            match (r.request, r.operation_id) {
                (Some(request), None) => db.prepare_media_operation(&task, &request, now()),
                _ => {
                    return error(
                        StatusCode::BAD_REQUEST,
                        "invalid_media_request",
                        "准备操作需要生成请求。",
                    );
                }
            }
        } else {
            match (r.request, r.operation_id) {
                (None, Some(op)) => db.media_task_operation(&task, &op, now()),
                _ => {
                    return error(
                        StatusCode::BAD_REQUEST,
                        "invalid_media_request",
                        "核对操作需要作业编号。",
                    );
                }
            }
        };
        match result {
            Ok(v) => v,
            Err(e) => return failure(e),
        }
    };
    let operation = intent["operation"]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    // Drop SQLite transactions before calling Host: collection calls author APIs.
    let execution = call_host(&s, &p, &operation, &r.action).await;
    let (db, project) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.media_operation_intent(&project, &operation, now()) {
        Ok(mut value) => {
            value["execution"] = execution;
            if value["state"] == "ready" && value["operation"]["kind"] == "audio.transcribe" {
                value["transcript"] = match db.media_transcript_page(
                    &project,
                    &operation,
                    r.transcript_offset.unwrap_or(0),
                    r.transcript_limit.unwrap_or(2000),
                ) {
                    Ok(v) => v,
                    Err(e) => return failure(e),
                };
            }
            Json(value).into_response()
        }
        Err(e) => failure(e),
    }
}
async fn call_host(s: &RuntimeState, p: &str, op: &str, action: &str) -> Value {
    let Some((url, token)) = &s.media_host_callback else {
        return json!({"available":false,"errorCode":"MEDIA_HOST_UNAVAILABLE","message":"媒体执行服务尚未连接，操作已保留。"});
    };
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(v) => v,
        Err(_) => return json!({"available":false,"errorCode":"MEDIA_HOST_UNAVAILABLE"}),
    };
    let response = client
        .post(url)
        .bearer_auth(token.as_ref())
        .header("content-type", "application/json")
        .body(json!({"projectId":p,"operationId":op,"action":action}).to_string())
        .send()
        .await;
    match response {
        Ok(response) if response.status().is_success() => {
            if response
                .content_length()
                .is_some_and(|n| n > 2 * 1024 * 1024)
            {
                return json!({"errorCode":"MEDIA_HOST_RESPONSE_TOO_LARGE"});
            }
            match response.bytes().await {
                Ok(bytes) if bytes.len() <= 2 * 1024 * 1024 => serde_json::from_slice(&bytes)
                    .unwrap_or_else(|_| json!({"errorCode":"MEDIA_HOST_RESPONSE_INVALID"})),
                _ => json!({"errorCode":"MEDIA_HOST_RESPONSE_INVALID"}),
            }
        }
        // A timeout is not evidence that no provider request was submitted.
        _ => {
            json!({"available":false,"errorCode":"MEDIA_HOST_RECONCILIATION_REQUIRED","message":"作业状态尚未核对，请使用同一作业编号继续查看。"})
        }
    }
}
async fn read_intent(
    State(s): State<RuntimeState>,
    Path((p, op)): Path<(String, String)>,
) -> Response {
    let (db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.media_operation_intent(&p, &op, now()) {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OutputManifest {
    output_count: usize,
}
async fn begin_outputs(
    State(s): State<RuntimeState>,
    Path((p, op)): Path<(String, String)>,
    Json(r): Json<OutputManifest>,
) -> Response {
    let (mut db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.begin_media_operation_outputs(&p, &op, r.output_count, now()) {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
async fn transcript(
    State(s): State<RuntimeState>,
    Path((p, op)): Path<(String, String)>,
    Json(r): Json<MediaTranscriptInput>,
) -> Response {
    let (mut db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.store_media_transcript(&p, &op, &r, now()) {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
async fn seal(
    State(s): State<RuntimeState>,
    Path((p, op)): Path<(String, String)>,
    Json(r): Json<MediaOperationSeal>,
) -> Response {
    let (mut db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.seal_media_operation(&p, &op, &r, now()) {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscriptQuery {
    offset: Option<usize>,
    limit: Option<usize>,
}
async fn read_transcript(
    State(s): State<RuntimeState>,
    Path((p, op)): Path<(String, String)>,
    Query(q): Query<TranscriptQuery>,
) -> Response {
    let (db, p) = match open(&s, p) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match db.media_transcript_page(&p, &op, q.offset.unwrap_or(0), q.limit.unwrap_or(2000)) {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e),
    }
}
