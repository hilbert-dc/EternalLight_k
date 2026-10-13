use super::*;

#[tokio::test]
async fn next_dialogue_queries_remodeled_source_without_promoting_candidate_facts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("remodeled-query.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repository, 1);
    let app = router(task_fixture::state(path, AUTHOR.into(), CANDIDATE.into()));
    let original = task(&app, &project, "advisory", json!({"type":"project"})).await;
    let query = json!({"threadId":"query-thread","expectedRevision":0,"plan":{
        "question":"林夕如何处理密函？","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[{"id":"state","operation":{"type":"state","entityId":"person-0","perspective":"world"}}]
    }});
    let original_uri = format!("/api/projects/{project}/tasks/{original}/query");
    for budget in [3000, 12000, 20000] {
        let mut invalid = query.clone();
        invalid["plan"]["byteBudget"] = json!(budget);
        let (status, rejected) = call(&app, "POST", &original_uri, invalid, CANDIDATE).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
        assert!(
            rejected["error"]["message"]
                .as_str()
                .unwrap()
                .contains("between 4000 and 9000 bytes inclusive"),
            "{rejected}"
        );
    }
    let (status, before) = call(&app, "POST", &original_uri, query.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{before}");
    assert!(!before["steps"][0]["results"].as_array().unwrap().is_empty());

    let (status, draft) = call(&app, "POST", &format!("/api/projects/{project}/drafts/checkpoints"), json!({
        "payload":{"chapterId":"chapter-1","chapterNumber":1,"title":"密函","body":"林夕藏起密函。"},
        "expectedRevision":0,"expectedCandidateRevisionId":"revision-1","idempotencyKey":"replace-query-source"
    }), AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{draft}");
    let (status, stale) = call(&app, "POST", &original_uri, query.clone(), CANDIDATE).await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale}");

    let pending = task(&app, &project, "advisory", json!({"type":"project"})).await;
    let (status, missing) = call(
        &app,
        "POST",
        &format!("/api/projects/{project}/tasks/{pending}/query"),
        query.clone(),
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{missing}");
    assert_eq!(missing["steps"][0]["results"], json!([]));
    assert_eq!(missing["coverage"]["complete"], false);

    let (status, batch) = call(
        &app,
        "POST",
        &format!("/api/projects/{project}/modeling-batches"),
        json!({
            "objectId":"draft-chapter-1","sourceRevisionId":draft["revisionId"],
            "expectedRevision":0,"extractorVersion":"remodeled-query-test"
        }),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    let model = task(
        &app,
        &project,
        "modelKnowledge",
        json!({"batchId":batch["id"]}),
    )
    .await;
    let (status, published) = call(&app, "POST", &format!("/api/projects/{project}/modeling-batches/{}/publish", batch["id"].as_str().unwrap()), json!({
        "taskId":model,"threadId":"query-thread","expectedRevision":0,
        "entities":[{"id":"person-0","kind":"character","displayName":"林夕","aliases":[],"evidence":{"quote":"林夕","start":0,"end":2}}],
        "assertions":[{"kind":"assertion","subjectId":"person-0","predicate":"hidLetter","value":true,
            "evidence":{"quote":"藏起密函","start":2,"end":6}}]
    }), CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{published}");

    let next = task(&app, &project, "advisory", json!({"type":"project"})).await;
    let (status, report) = call(
        &app,
        "POST",
        &format!("/api/projects/{project}/tasks/{next}/query"),
        query,
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    let results = report["steps"][0]["results"].as_array().unwrap();
    assert_eq!(results.len(), 1, "{report}");
    let row = &results[0];
    assert_eq!(
        json!({"source":row["sourceRevisionId"],"confirmation":row["confirmationState"],
        "predicate":row["assertion"]["predicate"],"value":row["assertion"]["value"]}),
        json!({"source":draft["revisionId"],"confirmation":"candidate","predicate":"hidLetter","value":true})
    );
    let evidence_ref = row["assertion"]["evidenceRef"].as_str().unwrap();
    assert_eq!(report["evidence"][evidence_ref]["quote"], "藏起密函");
    assert_eq!(
        report["evidence"][evidence_ref]["sourceRevisionId"],
        draft["revisionId"]
    );
    assert_eq!(report["coverage"]["semanticCompleteness"], "unknown");

    let literal_query = json!({"threadId":"query-thread","expectedRevision":0,"plan":{
        "question":"核对新稿中林夕和密函的原文","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[
            {"id":"spaced","operation":{"type":"text","query":"林夕 密函","limit":5}},
            {"id":"letter","operation":{"type":"text","query":"密函","limit":5}}
        ]
    }});
    let (status, literal) = call(
        &app,
        "POST",
        &format!("/api/projects/{project}/tasks/{next}/query"),
        literal_query,
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{literal}");
    assert_eq!(
        literal["steps"][0]["coverage"]["matchMode"],
        "contiguousSubstring"
    );
    assert_eq!(literal["steps"][0]["results"], json!([]));
    assert!(
        !literal["steps"][1]["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        literal["steps"][1]["coverage"]["matchMode"],
        "contiguousSubstring"
    );
    assert!(
        literal["steps"][1]["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["sourceRevisionId"] == draft["revisionId"])
    );
}
