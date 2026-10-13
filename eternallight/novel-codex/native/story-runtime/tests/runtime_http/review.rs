use super::*;

#[tokio::test]
async fn proposal_set_http_previews_applies_and_undoes_a_group() {
    use novel_codex_story_core::{
        BranchId, CommitId, CreateReviewRequest, IdempotencyKey, ObjectId, RevisionId,
        SaveCandidateRequest,
    };

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("proposal-set.sqlite3");
    let (status, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"多对象提案"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project = ProjectId::parse(created["project"]["id"].as_str().unwrap()).unwrap();
    let branch = BranchId::parse("main").unwrap();
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let mut snapshot = repository
        .branch_head(&project, &branch)
        .unwrap()
        .unwrap()
        .snapshot_id;
    for (object, kind, revision, review) in [
        (
            "artifact-brief",
            "brief",
            "proposal-brief-revision",
            "proposal-brief-review",
        ),
        (
            "artifact-outline",
            "outline",
            "proposal-outline-revision",
            "proposal-outline-review",
        ),
    ] {
        let object_id = ObjectId::parse(object).unwrap();
        let revision_id = RevisionId::parse(revision).unwrap();
        let next_snapshot_id =
            novel_codex_story_core::SnapshotId::parse(format!("snapshot-{review}")).unwrap();
        repository
            .save_candidate(SaveCandidateRequest {
                project_id: project.clone(),
                branch_id: branch.clone(),
                object_id: object_id.clone(),
                object_kind: kind.into(),
                revision_id: revision_id.clone(),
                commit_id: CommitId::parse(format!("candidate-{review}")).unwrap(),
                idempotency_key: IdempotencyKey::parse(format!("save-{review}")).unwrap(),
                request_digest: review.into(),
                expected_project_revision: 0,
                expected_snapshot_id: snapshot,
                next_snapshot_id: next_snapshot_id.clone(),
                expected_candidate_revision_id: None,
                content_digest: "content".into(),
                content_json: "{}".into(),
                chapter_projection: None,
                task_output: None,
                linked_impact_ids: vec![],
            })
            .unwrap();
        repository
            .create_review(CreateReviewRequest {
                review_id: review.into(),
                project_id: project.clone(),
                object_id,
                object_kind: kind.into(),
                revision_id,
                idempotency_key: IdempotencyKey::parse(format!("review-{review}")).unwrap(),
                request_digest: review.into(),
                checks_json: "[]".into(),
                proposals_json: "[]".into(),
                modeling_batch_id: None,
                selections_json: "{}".into(),
            })
            .unwrap();
        snapshot = next_snapshot_id;
    }
    let uri = format!("/api/projects/{}/proposal-sets", project.as_str());
    let request = json!({"baseSnapshotId":snapshot.as_str(), "intent":"先改设定再改大纲",
        "operations":[
            {"operationId":"brief","reviewId":"proposal-brief-review","dependsOn":[]},
            {"operationId":"outline","reviewId":"proposal-outline-review","dependsOn":["brief"]}
        ], "idempotencyKey":"proposal-http-key"});
    let (status, _) = call_as(
        application(&path),
        "POST",
        &uri,
        Some(request.clone()),
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, proposal) = call(application(&path), "POST", &uri, Some(request)).await;
    assert_eq!(status, StatusCode::CREATED, "{proposal}");
    let id = proposal["id"].as_str().unwrap();
    let (status, _) = call_as(application(&path), "GET", &uri, None, CANDIDATE_TOKEN, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, listing) = call(application(&path), "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listing["data"][0]["id"], id);
    assert_eq!(listing["data"][0]["status"], "proposed");
    let (status, invalid_cursor) =
        call(application(&path), "GET", &format!("{uri}?cursor=-1"), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid_cursor}");
    let preview_uri = format!("{uri}/{id}/preview");
    let (status, partial) = call(
        application(&path),
        "POST",
        &preview_uri,
        Some(json!({"selectedOperationIds":["outline"],"selectionsByOperationId":{"outline":{}}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{partial}");
    assert_eq!(partial["selection"]["declaredClosureReady"], false);
    assert_eq!(
        partial["selection"]["missingDependencies"],
        json!([{"operationId":"outline","requires":"brief"}])
    );
    let (status, complete) = call(
        application(&path),
        "POST",
        &preview_uri,
        Some(json!({"selectedOperationIds":["brief","outline"],"selectionsByOperationId":{"brief":{},"outline":{}}})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{complete}");
    assert_eq!(complete["selection"]["declaredClosureReady"], true);
    assert_eq!(complete["selection"]["applyReady"], true);
    assert!(
        repository
            .object_heads(&project)
            .unwrap()
            .iter()
            .all(|head| head.accepted_revision_id.is_none())
    );
    let apply_uri = format!("{uri}/{id}/apply");
    let apply_request = json!({
        "selectedOperationIds": ["brief", "outline"],
        "selectionsByOperationId": {"brief": {}, "outline": {}},
        "idempotencyKey": "proposal-http-apply"
    });
    let (status, _) = call_as(
        application(&path),
        "POST",
        &apply_uri,
        Some(apply_request.clone()),
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, applied) = call(
        application(&path),
        "POST",
        &apply_uri,
        Some(apply_request.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    assert_eq!(applied["resultRevision"], 1);
    let (status, applied_status) =
        call(application(&path), "GET", &format!("{uri}/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(applied_status["status"], "applied");
    assert_eq!(
        applied_status["application"]["id"],
        applied["applicationId"]
    );
    let (status, workspace_after_apply) = call(
        application(&path),
        "GET",
        &format!("/api/projects/{}/creative-workspace", project.as_str()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{workspace_after_apply}");
    assert_eq!(
        workspace_after_apply["headSnapshotId"],
        applied["resultSnapshotId"]
    );
    assert_eq!(
        workspace_after_apply["commits"][0]["proposalApplicationId"],
        applied["applicationId"]
    );
    let (_, applied_listing) = call(application(&path), "GET", &uri, None).await;
    assert_eq!(applied_listing["data"][0]["status"], "applied");
    let (status, replay) = call(application(&path), "POST", &apply_uri, Some(apply_request)).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["applicationId"], applied["applicationId"]);
    assert_eq!(replay["replayed"], true);
    let undo_uri = format!(
        "/api/projects/{}/proposal-applications/{}/undo",
        project.as_str(),
        applied["applicationId"].as_str().unwrap()
    );
    let (status, undone) = call(
        application(&path),
        "POST",
        &undo_uri,
        Some(json!({
            "expectedProjectRevision": 1, "idempotencyKey": "proposal-http-undo"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undone}");
    assert_eq!(undone["resultRevision"], 2);
    let (status, undone_status) =
        call(application(&path), "GET", &format!("{uri}/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(undone_status["status"], "undone");
    let mut cursor = None::<String>;
    let mut group_history = Vec::new();
    for _ in 0..8 {
        let suffix = cursor
            .as_ref()
            .map_or(String::new(), |value| format!("&cursor={value}"));
        let (status, page) = call(
            application(&path), "GET",
            &format!("/api/projects/{}/knowledge/evolution?dimension=creative&projectRevision=2&limit=1{suffix}", project.as_str()),
            None,
        ).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        group_history.extend(
            page["entries"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|entry| entry["proposalApplicationId"] == applied["applicationId"])
                .cloned(),
        );
        cursor = page["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(group_history.len(), 4);
    assert!(cursor.is_none());
    assert!(
        repository
            .object_heads(&project)
            .unwrap()
            .iter()
            .all(|head| head.accepted_revision_id.is_none())
    );
}

#[tokio::test]
async fn hypothesis_routes_are_author_only_and_reject_unpinned_assumptions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hypothesis-api.sqlite3");
    let (status, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"假设边界"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project_id = created["project"]["id"].as_str().unwrap();
    let uri = format!("/api/projects/{project_id}/hypotheses");
    let payload = json!({
        "baseSnapshotId": "missing-snapshot",
        "intent": "如果师父没有离开",
        "assumptions": [{"kind":"assumeClaim", "subjectId":"master", "predicate":"alive", "value":true, "storyTime":1}],
        "idempotencyKey": "hypothesis-api-test",
    });
    let (status, _) = call_as(
        application(&path),
        "POST",
        &uri,
        Some(payload.clone()),
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(application(&path), "POST", &uri, Some(payload)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        application(&path),
        "GET",
        &format!("{uri}/hypothesis-missing"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call_as(
        application(&path),
        "GET",
        &format!("{uri}/hypothesis-missing"),
        None,
        CANDIDATE_TOKEN,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn impact_analysis_is_author_only_and_never_claims_global_completeness() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("impact-analysis.sqlite3");
    let (status, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"影响分析"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let project_id = created["project"]["id"].as_str().unwrap();
    let uri = format!("/api/projects/{project_id}/impact-analysis?entityId=event-a");
    let (status, report) = call(application(&path), "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["findings"], json!([]));
    assert_eq!(report["coverage"]["globalImpactComplete"], false);
    assert_eq!(report["authority"], "readOnlyHypothesis");
    let (status, _) = call(
        application(&path),
        "GET",
        &format!("{uri}&fromRevisionId=only-one"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call_as(application(&path), "GET", &uri, None, CANDIDATE_TOKEN, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn artifact_review_accept_undo_round_trip() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (status, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Round trip", "logline": "Atomic story"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = created["project"]["id"].as_str().unwrap();
    let artifact_uri = format!("/api/projects/{project_id}/artifacts/brief");
    let candidate_request = json!({
        "payload": {"premise": "memory", "stakes": "identity"},
        "expectedRevision": 0,
        "expectedCandidateRevisionId": null,
        "idempotencyKey": "save-brief-1",
        "linkedImpactIds": []
    });
    let (status, candidate) = call(
        application(&path),
        "POST",
        &artifact_uri,
        Some(candidate_request.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, replay) = call(
        application(&path),
        "POST",
        &artifact_uri,
        Some(candidate_request),
    )
    .await;
    assert_eq!(candidate, replay);

    let revision_id = candidate["revisionId"].as_str().unwrap();
    let review_uri = format!("/api/projects/{project_id}/reviews");
    let (status, review) = call(
        application(&path),
        "POST",
        &review_uri,
        Some(json!({
            "artifactKind": "brief",
            "revisionId": revision_id,
            "proposals": [],
            "selections": {},
            "idempotencyKey": "review-brief-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let review_id = review["id"].as_str().unwrap();
    let accept_uri = format!("/api/projects/{project_id}/reviews/{review_id}/accept");
    let (status, accepted) = call(
        application(&path),
        "POST",
        &accept_uri,
        Some(json!({
            "expectedRevision": 0,
            "idempotencyKey": "accept-brief-1",
            "selections": {}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accepted["resultRevision"], 1);

    let commit_id = accepted["commitId"].as_str().unwrap();
    let undo_uri = format!("/api/projects/{project_id}/commits/{commit_id}/undo");
    let (status, undone) = call(
        application(&path),
        "POST",
        &undo_uri,
        Some(json!({
            "expectedRevision": 1,
            "idempotencyKey": "undo-brief-1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(undone["resultRevision"], 2);

    let workspace_uri = format!("/api/projects/{project_id}/creative-workspace");
    let (status, workspace) = call(application(&path), "GET", &workspace_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(workspace["revision"], 2);
    assert!(workspace["artifacts"]["brief"]["accepted"].is_null());
    assert_eq!(
        workspace["artifacts"]["brief"]["candidate"]["id"],
        revision_id
    );
}

#[tokio::test]
async fn planning_review_seat_uses_frozen_context_and_persists_report() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Review"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let outline_uri = format!("/api/projects/{project_id}/artifacts/outline");
    let (_, outline) = call(
        application(&path),
        "POST",
        &outline_uri,
        Some(json!({
            "payload": {"nodes": [{
                "id": "book-1", "kind": "book", "position": 1,
                "title": "The Last Lamp", "purpose": "Test memory", "summary": "A city forgets.",
                "dependencyKeys": [], "certainty": "tentative", "protection": "free"
            }]},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "planning-review-outline-1"
        })),
    )
    .await;
    let start_uri = format!("/api/projects/{project_id}/planning-reviews");
    let (status, run) = call(
        application(&path),
        "POST",
        &start_uri,
        Some(json!({
            "artifactKind": "outline", "revisionId": outline["revisionId"],
            "targetNodeId": null, "authorIntent": "Check causality",
            "roles": ["structure", "character"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let run_id = run["id"].as_str().unwrap();
    let capability = "review-secret-123456789012345678901234567890";
    let capability_hash = format!("{:x}", Sha256::digest(capability.as_bytes()));
    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let seat_uri =
        format!("/api/projects/{project_id}/planning-reviews/{run_id}/seats/structure/tasks");
    let (_, workspace) = call(
        application(&path),
        "GET",
        &format!("/api/projects/{project_id}/creative-workspace"),
        None,
    )
    .await;
    let (status, _) = call(application(&path), "POST", &format!("/api/projects/{project_id}/work-tasks/work-{run_id}/budget-authorization"), Some(json!({
        "expectedVersion":0,"expectedProjectRevision":0,"expectedSourceDigest":workspace["sourceDigest"],
        "maxRequests":3,"maxTokens":1000,"deadlineAt":expires_at
    }))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, task) = call(
        application(&path),
        "POST",
        &seat_uri,
        Some(json!({
            "threadId": "thread-structure", "model": "test",
            "capabilityHash": capability_hash, "capabilityExpiresAt": expires_at
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let task_id = task["id"].as_str().unwrap();
    let query_plan = json!({"question":"规划证据是否充分", "byteBudget":9000, "ranking":{"type":"literal"}, "steps":[
        {"id":"text", "operation":{"type":"text", "query":"港城", "limit":1}},
        {"id":"state", "operation":{"type":"state", "entityId":"linxi", "perspective":"author"}}
    ]});
    let encoded: String = query_plan
        .to_string()
        .as_bytes()
        .iter()
        .map(|byte| format!("%{byte:02X}"))
        .collect();
    let context_uri = format!(
        "/api/projects/{project_id}/context?taskId={task_id}&threadId=thread-structure&projectRevision=0&purpose=planner&budget=24&byteBudget=60000&queryPlan={encoded}"
    );
    let (status, context) = call_as(
        application(&path),
        "GET",
        &context_uri,
        None,
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        context["queryReport"]["policy"]["operation"],
        "reviewPlanningSeat"
    );
    assert_eq!(context["queryReport"]["policy"], context["queryScope"]);
    let planning_review_source = read_frozen_context_source(
        &path, project_id, &task, capability, &context, "/operationContext/planningReview",
    ).await;
    assert_eq!(
        planning_review_source["role"],
        "structure"
    );
    assert_eq!(
        planning_review_source["checks"],
        json!(["structure:book-1"])
    );
    let report_uri = format!("/api/projects/{project_id}/planning-reviews/{run_id}/reports");
    let (status, report) = call_as(
        application(&path),
        "POST",
        &report_uri,
        Some(json!({
            "taskId": task_id, "threadId": "thread-structure", "expectedRevision": 0,
            "report": {
                "summary": "Causality is visible.", "strengths": [], "findings": [],
                "coverage": [{"checkId": "structure:book-1", "status": "checked", "reason": "Reviewed"}]
            }
        })),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["status"], "complete");
    let complete_uri = format!("/api/projects/{project_id}/tasks/{task_id}/complete");
    let (status, _) = call_as(
        application(&path),
        "POST",
        &complete_uri,
        Some(json!({"threadId": "thread-structure", "summary": "done"})),
        CANDIDATE_TOKEN,
        Some(capability),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let finalize_uri = format!("/api/projects/{project_id}/planning-reviews/{run_id}/finalize");
    settlement::settle(&path, project_id, task_id);
    let (status, finalized) =
        call(application(&path), "POST", &finalize_uri, Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(finalized["status"], "partial");
    assert_eq!(finalized["reports"].as_array().unwrap().len(), 1);
}

use novel_codex_story_core::{ProjectRevisionRepository, WorkflowRepository};
