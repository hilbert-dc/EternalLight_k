//! The legacy workspace reader still serves the production desktop startup flow.
use axum::{body::Body,http::{Request,StatusCode}};
use http_body_util::BodyExt;
use novel_codex_story_runtime::{router,RuntimeState};
use serde_json::{Value,json};
use tower::ServiceExt;
async fn call(app:axum::Router,method:&str,path:&str,body:Value)->(StatusCode,Value){
 let response=app.oneshot(Request::builder().method(method).uri(path).header("authorization","Bearer state-contract-author-12345678901234").header("content-type","application/json").body(Body::from(body.to_string())).unwrap()).await.unwrap();
 let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();(status,serde_json::from_slice(&bytes).unwrap())
}
#[tokio::test]
async fn fresh_project_state_preserves_native_timestamps_and_empty_sections(){
 let directory=tempfile::tempdir().unwrap();let state=RuntimeState::new(directory.path().join("state.db"),"state-contract-author-12345678901234".into(),"state-contract-writer-12345678901234".into()).unwrap();let app=router(state);
 let (status,created)=call(app.clone(),"POST","/api/projects",json!({"title":"本地作品","logline":"","medium":"novel","idempotencyKey":"create"})).await;assert_eq!(status,StatusCode::CREATED,"{created}");
 let project=created["project"]["id"].as_str().unwrap();let (status,value)=call(app.clone(),"GET",&format!("/api/projects/{project}/state"),Value::Null).await;
 assert_eq!(status,StatusCode::OK,"{value}");assert!(value["project"]["updated_at"].is_i64());assert!(value["novelEdition"]["chapters"].as_array().unwrap().is_empty());
 let (status,saved)=call(app.clone(),"POST",&format!("/api/projects/{project}/drafts/checkpoints"),json!({"payload":{"chapterId":"chapter-state","chapterNumber":1,"title":"正文","body":"本地正文","sceneGroups":[{"id":"state-scene","body":"本地正文"}]},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"state-draft"})).await;
 assert_eq!(status,StatusCode::CREATED,"{saved}");
 let (status,value)=call(app,"GET",&format!("/api/projects/{project}/state"),Value::Null).await;
 assert_eq!(status,StatusCode::OK,"{value}");let chapter=&value["novelEdition"]["chapters"][0];
 assert_eq!(chapter["id"],"chapter-state");assert_eq!(chapter["passages"][0]["body"],"本地正文");
 assert!(chapter.get("summary").is_none());assert!(chapter.get("updated_at").is_none());assert!(chapter["passages"][0].get("scene_id").is_none());
}
