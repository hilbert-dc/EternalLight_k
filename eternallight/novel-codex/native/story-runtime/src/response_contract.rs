//! Validate Renderer-facing author HTTP projections without changing their JSON.
use axum::{body::{Body, to_bytes}, extract::{Request, State}, middleware::Next, response::{Response, IntoResponse}};
use crate::renderer_responses::*;
use std::sync::LazyLock;

struct Inventory { methods: Vec<Contract> }
struct Contract { method: String, kind: String, response: Option<String>, routes: Vec<Route> }
struct Route { verb: String, path: String, query: std::collections::BTreeMap<String,String> }
static INVENTORY: LazyLock<Inventory> = LazyLock::new(|| {
 let mut value: serde_json::Value = serde_json::from_str(include_str!("../../../shared/runtime-http-contracts.json")).expect("checked HTTP contract inventory");
 let media: serde_json::Value=serde_json::from_str(include_str!("../../../shared/multimedia-http-contracts.json")).expect("checked media inventory");
 value["methods"].as_array_mut().unwrap().extend(media["methods"].as_array().unwrap().iter().cloned());
 let methods=value["methods"].as_array().expect("inventory methods").iter().map(|item| Contract {
  method:item["method"].as_str().expect("method").into(),kind:item["kind"].as_str().expect("kind").into(),response:item["response"].as_str().map(str::to_owned),
  routes:item["routes"].as_array().expect("routes").iter().map(|route|Route{verb:route["verb"].as_str().expect("verb").into(),path:route["path"].as_str().expect("path").into(),query:route["query"].as_object().map(|q|q.iter().map(|(k,v)|(k.clone(),v.as_str().expect("query selector").to_owned())).collect()).unwrap_or_default()}).collect()
 }).collect();Inventory{methods}
});

fn path_matches(template: &str, path: &str) -> bool {
 let expected: Vec<_> = template.split('/').collect(); let actual: Vec<_> = path.split('/').collect();
 expected.len()==actual.len() && expected.iter().zip(actual).all(|(a,b)| (a.starts_with('{') && a.ends_with('}') && !b.is_empty()) || *a==b)
}
fn contract_for(method: &str, path: &str, query: &str) -> Option<&'static Contract> {
 let query:std::collections::BTreeMap<String,String>=serde_urlencoded::from_str(query).unwrap_or_default();
 INVENTORY.methods.iter().filter(|c| c.kind=="http").filter_map(|c| c.routes.iter().find(|r|r.verb==method && path_matches(&r.path,path) && r.query.iter().all(|(key,expected)|match expected.as_str(){"present"=>query.contains_key(key),"absent"=>!query.contains_key(key),value=>query.get(key).is_some_and(|actual|actual==value)})).map(|r|(c,r.path.split('/').filter(|s|!s.starts_with('{')).count()+r.query.len()))).max_by_key(|(_,score)|*score).map(|(c,_)|c)
}

macro_rules! responses { ($($name:ident),* $(,)?) => {
 #[cfg(test)]
 pub(crate) const RESPONSE_TYPES: &[&str] = &[$(stringify!($name)),*];
 fn validate(name: &str, body: &[u8]) -> Result<(),String> {match name {$(stringify!($name)=>serde_path_to_error::deserialize::<_, $name>(&mut serde_json::Deserializer::from_slice(body)).map(|_|()).map_err(|e|e.to_string()),)*_=>Err(format!("unregistered response contract {name}"))}}
};}
responses!(
 RendererMediaSourceCatalogResponse,
 RendererMediaListEditionsResponse,
 RendererMediaCreateEditionResponse,
 RendererMediaReadWorkspaceResponse,
 RendererMediaSaveDocumentResponse,
 RendererMediaPatchDocumentResponse,
 RendererMediaReadTextPageResponse,
 RendererMediaReadCanonResponse,
 RendererMediaSaveCanonResponse,
 RendererMediaSourceImpactsResponse,
 RendererListProjectsResponse,
 RendererCreateProjectResponse,
 RendererGetStateResponse,
 RendererGetLocalMetricsResponse,
 RendererRecordStageAbandonmentResponse,
 RendererExportProjectResponse,
 RendererGetChapterDraftHistoryResponse,
 RendererGetCreativeWorkspaceResponse,
 RendererGetQualityNotebookResponse,
 RendererChangeQualityNotebookResponse,
 RendererGetQualityPacketResponse,
 RendererGetQualityReviewResponse,
 RendererListProposalSetsResponse,
 RendererCreateProposalSetResponse,
 RendererGetProposalSetResponse,
 RendererPreviewProposalSetResponse,
 RendererApplyProposalSetResponse,
 RendererUndoProposalSetResponse,
 RendererGetImpactAnalysisResponse,
 RendererGetHistoricalKnowledgeStateResponse,
 RendererCreateHypothesisResponse,
 RendererGetHypothesisResponse,
 RendererGetIdentityCatalogResponse,
 RendererGetCharacterKnowledgeResponse,
 RendererSaveIdentityDecisionsResponse,
 RendererSavePropertyRulesResponse,
 RendererListGenerationRunsResponse,
 RendererPreviewGenerationReplanResponse,
 RendererReplanGenerationResponse,
 RendererGetSceneAssessmentResponse,
 RendererGetCommitmentExtractionResponse,
 RendererEstablishCommitmentResponse,
 RendererGetCommitmentResponse,
 RendererListCommitmentsResponse,
 RendererSaveCommitmentResponse,
 RendererCreateCommitmentReviewResponse,
 RendererListWorkTasksResponse,
 RendererGetSearchAuditResponse,
 RendererStartSearchAuditResponse,
 RendererControlSearchAuditResponse,
 RendererGetEntityAuditResponse,
 RendererStartEntityAuditResponse,
 RendererControlEntityAuditResponse,
 RendererGetRelationAuditResponse,
 RendererStartRelationAuditResponse,
 RendererControlRelationAuditResponse,
 RendererGetKnowledgeAuditResponse,
 RendererStartConditionEvaluationResponse,
 RendererGetConditionEvaluationResponse,
 RendererControlConditionEvaluationResponse,
 RendererGetConditionExplanationsResponse,
 RendererStartKnowledgeAuditResponse,
 RendererControlKnowledgeAuditResponse,
 RendererGetSnapshotKnowledgeAuditResponse,
 RendererStartSnapshotKnowledgeAuditResponse,
 RendererControlSnapshotKnowledgeAuditResponse,
 RendererGetFactAuditResponse,
 RendererStartFactAuditResponse,
 RendererControlFactAuditResponse,
 RendererGetWorkTaskResponse,
 RendererGetWorkIssuesResponse,
 RendererGetTaskFinalReplyResponse,
 RendererGetRevisionOutcomeResponse,
 RendererDecideRevisionOutcomeResponse,
 RendererGetPlanningReviewResponse,
 RendererCancelPlanningReviewResponse,
 RendererSetWorkingBaselineResponse,
 RendererClearWorkingBaselineResponse,
 RendererSaveContinuationIntentResponse,
 RendererGetWriterPreviewResponse,
 RendererListPlanningImpactsResponse,
 RendererSaveArtifactResponse,
 RendererMaterializeChapterPlanResponse,
 RendererGetWorkCatalogResponse,
 RendererDeleteChapterResponse,
 RendererListChapterStructureChangesResponse,
 RendererReadChapterStructureChangeResponse,
 RendererUpdateWorkCatalogResponse,
 RendererMaterializeStandaloneChapterResponse,
 RendererAssociateChapterPlanResponse,
 RendererRebindChapterPlanResponse,
 RendererResolvePlanningImpactResponse,
 RendererSetArtifactLockResponse,
 RendererSaveDraftCheckpointResponse,
 RendererCreateReviewResponse,
 RendererRejectIdentityReviewResponse,
 RendererAcceptReviewResponse,
 RendererUndoCommitResponse,
 RendererGetDiffsResponse,
 RendererSearchResponse
);

// Validation interprets only the wire shape. It never reruns or reverses domain work.
fn mismatch(contract: &Contract, status: axum::http::StatusCode, bytes: &[u8], issue: &str, request_path: &str) -> Response {
 // Bound diagnostics: a failed large read must not duplicate megabytes of prose.
 // Small receipts are retained verbatim. Large results retain request route context only.
 let original = if bytes.len() <= 65_536 {serde_json::from_slice::<serde_json::Value>(bytes).ok()} else {None};
 let original = if bytes.len() <= 65_536 {original.unwrap_or_else(||serde_json::json!({"malformedBodyBytes":bytes.len()}))} else {
  let identity = original.as_ref().and_then(serde_json::Value::as_object).map(|object|object.iter().filter(|(key,value)|
   (key.ends_with("Id") || key.ends_with("Revision") || matches!(key.as_str(),"id"|"version"|"replayed"|"status")) && !value.is_array() && !value.is_object() && value.to_string().len()<=1024
  ).take(32).map(|(k,v)|(k.clone(),v.clone())).collect::<serde_json::Map<String,serde_json::Value>>()).unwrap_or_default();
  serde_json::json!({"identity":identity,"omittedBodyBytes":bytes.len(),"reason":"diagnosticSizeLimit"})
 };
 (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(crate::contracts::RuntimeErrorEnvelope { error: crate::contracts::RuntimeError {
  code: "response_contract_mismatch".into(),
  message: "The operation returned success but its receipt failed wire validation. It may already be committed; reconcile using the original request identity before retrying.".into(),
  details: serde_json::json!({"method":contract.method,"requestPath":request_path,"reconcileRoutes":contract.routes.iter().map(|route|route.path.as_str()).collect::<Vec<_>>(),"contract":contract.response,"validationIssue":issue,"originalStatus":status.as_u16(),"originalResponse":original}),
  recovery: None,
 }})).into_response()
}
pub(crate) async fn enforce(State(state): State<crate::server::RuntimeState>, request: Request, next: Next) -> Response {
 let contract=contract_for(request.method().as_str(),request.uri().path(),request.uri().query().unwrap_or_default());
 
 let request_path=request.uri().path().to_owned();
 let response=next.run(request).await;
 let Some(contract)=contract else{return response};
 if !response.status().is_success(){return response;}
 let (parts,body)=response.into_parts();
 let bytes=match to_bytes(body,usize::MAX).await{Ok(value)=>value,Err(e)=>return crate::server::internal_error(e)};
 let input=bytes.clone();let name=contract.response.as_deref().expect("HTTP inventory response type");
 let status=parts.status;let validation_path=request_path.clone();
 let checked=crate::repository_executor::execute(state,move ||validate(name,&input).map_err(|issue|mismatch(contract,status,&input,&issue,&validation_path))).await;
 match checked {
  Ok(Ok(()))=>Response::from_parts(parts,Body::from(bytes)),
  Ok(Err(response))=>response,
  Err(_)=>mismatch(contract,parts.status,&bytes,"Response validation executor did not return a result",&request_path),
 }
}

#[cfg(test)]
mod tests {
 use super::*;
 use axum::{Router, routing::get, middleware, http::{Request as HttpRequest, StatusCode}};
 use tower::ServiceExt;
 #[test]
 fn every_http_method_has_a_real_registered_type() {
  for contract in &INVENTORY.methods { if contract.kind=="http" {assert!(RESPONSE_TYPES.contains(&contract.response.as_deref().unwrap()),"{}",contract.method);assert!(!contract.routes.is_empty());} }
 }
 #[test]
 fn specific_paths_and_query_variants_select_the_correct_contract() {
  assert_eq!(contract_for("POST","/api/projects/project/artifacts/identity","").unwrap().method,"saveIdentityDecisions");
  assert!(contract_for("GET","/api/projects/project/not-an-api","").is_none());
  assert!(!path_matches("/api/projects/{projectId}/state","/api/projects//state"));
 }
 #[test]
 fn wire_shape_rejects_missing_fields_wrong_scalars_and_invalid_enums() {
  let valid=serde_json::json!({"projects":[{"id":"p","title":"t","logline":"","created_at":1,"updated_at":2}]});
  assert!(validate("RendererListProjectsResponse",&serde_json::to_vec(&valid).unwrap()).is_ok());
  let mut broken=valid.clone();broken["projects"][0]["created_at"]=serde_json::json!("1");
  assert!(validate("RendererListProjectsResponse",&serde_json::to_vec(&broken).unwrap()).is_err());
  let mut broken=valid;broken["projects"][0].as_object_mut().unwrap().remove("id");
  assert!(validate("RendererListProjectsResponse",&serde_json::to_vec(&broken).unwrap()).is_err());
  assert!(serde_json::from_value::<HttpKnowledgeFindingKind>(serde_json::json!("invented")).is_err());
  assert!(serde_json::from_value::<HttpDraftRevision>(serde_json::json!({})).is_err());
  let receipt=serde_json::json!({"commitId":"c","artifactKind":"brief","artifactRevisionId":"r","previousAcceptedRevisionId":null,"baseRevision":0,"resultRevision":1});
  assert!(serde_json::from_value::<HttpAcceptanceReceipt>(receipt.clone()).is_ok());
  let mut missing=receipt;missing.as_object_mut().unwrap().remove("previousAcceptedRevisionId");
  assert!(serde_json::from_value::<HttpAcceptanceReceipt>(missing).is_err());
 }
 #[tokio::test]
 async fn mismatch_preserves_small_committed_receipt_and_large_diagnostics_are_bounded() {
  let contract=contract_for("POST","/api/projects","").unwrap();
  let receipt=serde_json::json!({"projectId":"committed-project","idempotencyKey":"original-key","replayed":false});
  let response=mismatch(contract,StatusCode::CREATED,&serde_json::to_vec(&receipt).unwrap(),"missing project","/api/projects");
  assert_eq!(response.status(),StatusCode::INTERNAL_SERVER_ERROR);
  let body=to_bytes(response.into_body(),100_000).await.unwrap();let value:serde_json::Value=serde_json::from_slice(&body).unwrap();
  assert_eq!(value["error"]["details"]["originalResponse"],receipt);
  assert!(value["error"]["message"].as_str().unwrap().contains("already be committed"));
  let large=serde_json::json!({"id":"committed","body":"x".repeat(100_000)});
  let response=mismatch(contract,StatusCode::OK,&serde_json::to_vec(&large).unwrap(),"invalid body","/api/projects/project/state");
  let body=to_bytes(response.into_body(),4096).await.unwrap();let value:serde_json::Value=serde_json::from_slice(&body).unwrap();
  assert!(value["error"]["details"]["originalResponse"]["omittedBodyBytes"].as_u64().unwrap()>100_000);
 }
 #[tokio::test]
 async fn successful_json_is_not_rewritten_and_existing_errors_are_not_reclassified() {
  let dir=tempfile::tempdir().unwrap();
  let state=crate::server::RuntimeState::new(dir.path().join("story.db"),"a".repeat(32),"w".repeat(32)).unwrap();
  let original="{\"projects\":[],\"extension\":{\"identity\":\"preserve\"}}";
  let router=Router::new().route("/api/projects",get(move||async move{(StatusCode::OK,original)})).route_layer(middleware::from_fn_with_state(state.clone(),enforce));
  let response=router.oneshot(HttpRequest::builder().uri("/api/projects").body(Body::empty()).unwrap()).await.unwrap();
  assert_eq!(response.status(),StatusCode::OK);assert_eq!(to_bytes(response.into_body(),4096).await.unwrap(),original);
  let router=Router::new().route("/api/projects",get(||async{(StatusCode::CONFLICT,"original conflict")})).route_layer(middleware::from_fn_with_state(state,enforce));
  let response=router.oneshot(HttpRequest::builder().uri("/api/projects").body(Body::empty()).unwrap()).await.unwrap();
  assert_eq!(response.status(),StatusCode::CONFLICT);assert_eq!(to_bytes(response.into_body(),4096).await.unwrap(),"original conflict");
 }
}
