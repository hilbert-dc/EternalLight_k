use super::*;

#[tokio::test]
async fn chinese_search_uses_selected_revision_and_returns_exact_source_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite3");
    let (_, created) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Search"})),
    )
    .await;
    let project_id = created["project"]["id"].as_str().unwrap();
    let draft_uri = format!("/api/projects/{project_id}/drafts/checkpoints");
    let (status, first) = call(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload": {"chapterNumber": 1, "title": "夜里的港口", "body": "林夕把密函藏进灯塔。"},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "search-first"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let search_uri = format!("/api/projects/{project_id}/search");
    let (status, found) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E5%AF%86%E5%87%BD&kind=draft"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["results"].as_array().unwrap().len(), 1, "{found}");
    assert_eq!(found["results"][0]["sourcePath"], "/sceneGroups/0/body");
    assert_eq!(found["results"][0]["sourceRevisionId"], first["revisionId"]);
    assert_eq!(
        found["results"][0]["evidence"],
        json!({
            "quote": "密函", "start": 3, "end": 5
        })
    );
    assert_eq!(found["coverage"]["complete"], true);
    let (_, hidden_metadata) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q={}", first["chapterId"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(hidden_metadata["results"], json!([]));
    let (_, one_character) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E5%87%BD"),
        None,
    )
    .await;
    assert_eq!(one_character["results"][0]["evidence"]["quote"], "函");
    let (_, title_match) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E6%B8%AF%E5%8F%A3"),
        None,
    )
    .await;
    assert_eq!(title_match["results"][0]["sourcePath"], "/title");
    let (status, _) = call(
        application(&path),
        "POST",
        &format!("/api/projects/{project_id}/artifacts/brief"),
        Some(json!({
            "payload": {"premise": "港城正在失去记忆。"},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "search-brief"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, artifact_match) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E5%A4%B1%E5%8E%BB&kind=brief"),
        None,
    )
    .await;
    assert_eq!(artifact_match["results"][0]["kind"], "brief");
    assert!(
        artifact_match["results"][0]["sourcePath"]
            .as_str()
            .unwrap()
            .ends_with("/premise")
    );
    let (status, _) = call(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload": {"chapterNumber": 2, "title": "第二章", "body": "林夕把另一封信藏起。"},
            "expectedRevision": 0, "expectedCandidateRevisionId": null,
            "idempotencyKey": "search-second-chapter"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, first_page) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E6%9E%97%E5%A4%95&limit=1"),
        None,
    )
    .await;
    assert_eq!(first_page["results"].as_array().unwrap().len(), 1);
    let search_cursor = first_page["nextCursor"].as_str().unwrap();
    let (_, next_page) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E6%9E%97%E5%A4%95&limit=1&cursor={search_cursor}"),
        None,
    )
    .await;
    assert_eq!(next_page["results"].as_array().unwrap().len(), 1);
    assert!(next_page["nextCursor"].is_null());
    assert_ne!(
        first_page["results"][0]["id"],
        next_page["results"][0]["id"]
    );
    let (_, other) = call(
        application(&path),
        "POST",
        "/api/projects",
        Some(json!({"logline":"","medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title": "Other"})),
    )
    .await;
    let other_id = other["project"]["id"].as_str().unwrap();
    let (_, isolated) = call(
        application(&path),
        "GET",
        &format!("/api/projects/{other_id}/search?q=%E5%AF%86%E5%87%BD"),
        None,
    )
    .await;
    assert_eq!(isolated["results"], json!([]));
    let (status, revised) = call(
        application(&path),
        "POST",
        &draft_uri,
        Some(json!({
            "payload": {"chapterId": first["chapterId"], "chapterNumber": 1,
                "title": "改稿后的港口", "body": "林夕把信纸藏进灯塔。"},
            "expectedRevision": 0, "expectedCandidateRevisionId": first["revisionId"],
            "idempotencyKey": "search-revised"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{revised}");
    let (_, stale) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E5%AF%86%E5%87%BD"),
        None,
    )
    .await;
    assert_eq!(stale["results"], json!([]));
    let (_, current) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E4%BF%A1%E7%BA%B8"),
        None,
    )
    .await;
    assert_eq!(
        current["results"][0]["sourceRevisionId"],
        revised["revisionId"]
    );
    assert_eq!(current["results"][0]["title"], "改稿后的港口");
    let first_snapshot = found["coverage"]["snapshotId"].as_str().unwrap();
    let historical_first_uri =
        format!("{search_uri}?q=%E5%AF%86%E5%87%BD&kind=draft&snapshotId={first_snapshot}");
    let (status, historical_first) =
        call(application(&path), "GET", &historical_first_uri, None).await;
    assert_eq!(status, StatusCode::OK, "{historical_first}");
    assert_eq!(
        historical_first["results"][0]["sourceRevisionId"],
        first["revisionId"]
    );
    assert_eq!(historical_first["results"][0]["title"], "夜里的港口");
    assert_eq!(historical_first["coverage"]["snapshotId"], first_snapshot);
    let (_, not_yet_written) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E4%BF%A1%E7%BA%B8&snapshotId={first_snapshot}"),
        None,
    )
    .await;
    assert_eq!(not_yet_written["results"], json!([]));
    let paged_snapshot = first_page["coverage"]["snapshotId"].as_str().unwrap();
    let historical_page_uri =
        format!("{search_uri}?q=%E6%9E%97%E5%A4%95&limit=1&snapshotId={paged_snapshot}");
    let (_, historical_page_one) =
        call(application(&path), "GET", &historical_page_uri, None).await;
    let historical_cursor = historical_page_one["nextCursor"].as_str().unwrap();
    let (status, historical_page_two) = call(
        application(&path),
        "GET",
        &format!("{historical_page_uri}&cursor={historical_cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{historical_page_two}");
    assert_eq!(historical_page_two["results"].as_array().unwrap().len(), 1);
    let (status, cross_project) = call(
        application(&path),
        "GET",
        &format!(
            "/api/projects/{other_id}/search?q=%E5%AF%86%E5%87%BD&snapshotId={first_snapshot}"
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{cross_project}");
    let (status, stale_cursor) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E6%9E%97%E5%A4%95&limit=1&cursor={search_cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale_cursor}");
    let (status, changed_query) = call(
        application(&path),
        "GET",
        &format!("{search_uri}?q=%E6%B8%AF%E5%8F%A3&limit=1&cursor={search_cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{changed_query}");
}
