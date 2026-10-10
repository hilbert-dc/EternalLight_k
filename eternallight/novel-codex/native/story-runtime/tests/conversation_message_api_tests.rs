use axum::{body::Body,http::{Request,StatusCode}};
use http_body_util::BodyExt;
use novel_codex_story_runtime::{router,RuntimeState};
use serde_json::{Value,json};
use tower::ServiceExt;
const AUTHOR:&str="message-author-token-1234567890123456";
const MODEL:&str="message-model-token-12345678901234567";
async fn call(path:&std::path::Path,method:&str,uri:&str,body:Option<Value>,model:bool)->(StatusCode,Value){
 let app=router(RuntimeState::new(path.to_owned(),AUTHOR.into(),MODEL.into()));
 let request=Request::builder().method(method).uri(uri).header("authorization",format!("Bearer {}",if model{MODEL}else{AUTHOR})).header("content-type","application/json").body(body.map(|v|Body::from(v.to_string())).unwrap_or_else(Body::empty)).unwrap();
 let response=app.oneshot(request).await.unwrap();let status=response.status();let body=response.into_body().collect().await.unwrap().to_bytes();(status,serde_json::from_slice(&body).unwrap_or(Value::Null))
}
#[tokio::test]
async fn messages_are_author_only_and_replay_claim_receipts_preserve_identity(){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("messages.sqlite3");
 let (status,created)=call(&path,"POST","/api/projects",Some(json!({"title":"Messages","logline":"","medium":"novel","idempotencyKey":"messages-project"})),false).await;
 assert!(status.is_success(),"{created}");let p=created["project"]["id"].as_str().unwrap();
 let collection=format!("/api/projects/{p}/conversations");
 let(status,record)=call(&path,"POST",&collection,Some(json!({"threadId":"thread-a","title":"Messages","scopeId":null,"forkedFromId":null,"lastTurnId":null})),false).await;assert!(status.is_success(),"{record}");
 for suffix in ["interludes","interludes/return"] {
   let (status,_) = call(&path,"POST",&format!("/api/projects/{p}/work-tasks/work-a/{suffix}"),Some(json!({})),true).await;
   assert_eq!(status,StatusCode::UNAUTHORIZED,"Model must not control interlude authorization");
 }
 let uri=format!("{collection}/thread-a/messages");let body=json!({"messageId":"m1","sessionId":"host-a","input":{"text":"解释设定","focus":null,"connectionId":null},"trigger":{"kind":"immediate"}});
 assert!(!call(&path,"POST",&uri,Some(body.clone()),true).await.0.is_success());
 let(status,received)=call(&path,"POST",&uri,Some(body.clone()),false).await;assert!(status.is_success(),"{received}");assert_eq!(received["status"],"queued");
 assert_eq!(call(&path,"POST",&uri,Some(body),false).await.1,received);
 let item=format!("{uri}/m1");
 let(status,claimed)=call(&path,"POST",&item,Some(json!({"action":"claim","expectedRevision":0,"sessionId":"host-a"})),false).await;assert!(status.is_success(),"{claimed}");assert_eq!(claimed["status"],"claimed");
 let(status,stale)=call(&path,"POST",&item,Some(json!({"action":"cancel","expectedRevision":0})),false).await;assert_eq!(status,StatusCode::CONFLICT);assert_eq!(stale["error"]["code"],"message_stale");
 assert_eq!(call(&path,"GET",&item,None,false).await.1["id"],"m1");
 let mut repo=novel_codex_story_core::SqliteStoryRepository::open(&path).unwrap();
 repo.create_task(novel_codex_story_core::CreateTaskRequest{task_id:"steer-task".into(),project_id:novel_codex_story_core::ProjectId::parse(p).unwrap(),thread_id:"thread-a".into(),operation:"advisory".into(),input_revision:0,model:None,target_json:json!({"type":"project"}).to_string(),input_manifest_json:json!({"authorIntent":"解释设定"}).to_string(),capability_hash:"a".repeat(64),capability_expires_at:4_000_000_000}).unwrap();
 drop(repo);
 let intents=format!("/api/projects/{p}/tasks/steer-task/intents");
 let(status,intent)=call(&path,"POST",&intents,Some(json!({"expectedIntentRevision":0,"authorText":"保留原结局"})),false).await;
 assert!(status.is_success(),"{intent}");
 let receipt=format!("{uri}/{}",intent["authorMessageId"].as_str().unwrap());
 let(status,pending)=call(&path,"GET",&receipt,None,false).await;assert!(status.is_success(),"{pending}");assert_eq!(pending["trigger"]["kind"],"steer");
 assert!(!call(&path,"GET",&receipt,None,true).await.0.is_success());
 assert_eq!(call(&path,"POST",&receipt,Some(json!({"action":"reconcile","expectedRevision":0})),false).await.1,pending);
 let(status,delivered)=call(&path,"POST",&format!("{intents}/1/delivered"),Some(json!({})),false).await;
 assert!(status.is_success(),"{delivered}");
 assert_eq!(call(&path,"GET",&receipt,None,false).await.1["status"],"delivered");

}
