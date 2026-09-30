use std::path::Path;

use novel_codex_story_core::{
    ConditionEvaluationControl as Control, ConditionEvaluationStatus as Status,
    ConditionEvaluationTarget, ProjectId, SqliteStoryRepository,
};
use pretty_assertions::assert_eq;
use rusqlite::params;
use serde_json::{Value, json};

// Runs against real published sources and an author-adopted rule from the HTTP
// fixture. Copies are exact historical sources, not fabricated successful jobs.
pub(super) async fn verify(
    path: &Path,
    project: &ProjectId,
    target: &ConditionEvaluationTarget,
    positive: &str,
    negative: &str,
) {
    let db = rusqlite::Connection::open(path).unwrap();
    let snapshot = "condition-long-execution";
    db.execute("INSERT INTO story_snapshots_v2(snapshot_id,project_id,branch_id,parent_snapshot_id) VALUES(?1,?2,'main',?3)",params![snapshot,project.as_str(),target.snapshot_id]).unwrap();
    db.execute("INSERT INTO story_snapshot_members_v2(snapshot_id,object_id,revision_id) SELECT ?1,object_id,source_revision_id FROM story_modeling_batches_v2 WHERE project_id=?2 AND batch_id=?3",params![snapshot,project.as_str(),target.source_batch_id]).unwrap();
    for index in 0..202 {
        let id = if index == 201 {
            "zz-final-counter".to_owned()
        } else {
            format!("condition-page-{index:03}")
        };
        let original = if index == 201 { negative } else { positive };
        let object = format!("draft-{id}");
        let revision = format!("revision-{id}");
        db.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) SELECT ?1,r.project_id,?2,r.object_kind,r.content_digest,r.content_json FROM story_revisions_v2 r JOIN story_modeling_batches_v2 b ON b.project_id=r.project_id AND b.object_id=r.object_id AND b.source_revision_id=r.revision_id WHERE b.project_id=?3 AND b.batch_id=?4",params![revision,object,project.as_str(),original]).unwrap();
        db.execute("INSERT INTO story_modeling_batches_v2(batch_id,project_id,object_id,source_revision_id,input_project_revision,extractor_version,status,assertions_json,entities_json,created_at,published_at) SELECT ?1,project_id,?2,?3,input_project_revision,extractor_version,'stale',assertions_json,entities_json,created_at,published_at FROM story_modeling_batches_v2 WHERE project_id=?4 AND batch_id=?5",params![id,object,revision,project.as_str(),original]).unwrap();
        db.execute("INSERT INTO story_snapshot_members_v2(snapshot_id,object_id,revision_id) VALUES(?1,?2,?3)",params![snapshot,object,revision]).unwrap();
        db.execute("UPDATE story_snapshot_knowledge_v2 SET batch_id=?1 WHERE snapshot_id=?2 AND object_id=?3",params![id,snapshot,object]).unwrap();
    }
    let mut repository = SqliteStoryRepository::open(path).unwrap();
    repository.rebuild_entity_index(project).unwrap();
    repository.rebuild_assertion_index(project).unwrap();
    let audit = repository.start_knowledge_audit(project).unwrap();
    loop {
        let job = repository
            .advance_knowledge_audit(project, &audit.id)
            .unwrap();
        if job.status != "running" {
            assert_eq!(job.verified_consistent, Some(true));
            break;
        }
    }
    let short_target = target.clone();
    let target = ConditionEvaluationTarget {
        snapshot_id: Some(snapshot.into()),
        ..target.clone()
    };
    // The event's explicit prerequisite must also exist in the frozen scope;
    // equivalent copied claims are not authority to resolve a different source.
    assert!(
        repository
            .start_condition_evaluation(project, target.clone())
            .is_err()
    );
    db.execute("INSERT INTO story_snapshot_members_v2(snapshot_id,object_id,revision_id) SELECT ?1,object_id,source_revision_id FROM story_modeling_batches_v2 WHERE project_id=?2 AND batch_id=?3",params![snapshot,project.as_str(),positive]).unwrap();
    let projection_uri = format!(
        "/api/projects/{}/knowledge/state?perspective=world&predicate=transfer&snapshotId={snapshot}",
        project.as_str()
    );
    let before_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM story_condition_evaluations_v1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let (status, before_projection) = super::call(
        path,
        "GET",
        &projection_uri,
        Value::Null,
        super::Role::Author,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{before_projection}");
    assert_eq!(
        before_projection["entries"][0]["eventConditionAssessment"]["result"],
        "unknown"
    );
    assert!(before_projection["entries"][0]["eventConditionAssessment"]["evaluationJob"].is_null());
    let after_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM story_condition_evaluations_v1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(before_count, after_count);
    let job = repository
        .start_condition_evaluation(project, target.clone())
        .unwrap();
    let before: (String, Option<String>) = db
        .query_row(
            "SELECT progress_json,last_batch_id FROM story_condition_evaluations_v1 WHERE id=?1",
            [&job.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    db.execute_batch("CREATE TRIGGER condition_test_abort BEFORE UPDATE OF progress_json ON story_condition_evaluations_v1 BEGIN SELECT RAISE(ABORT,'injected checkpoint commit failure'); END;").unwrap();
    assert!(
        repository
            .advance_condition_evaluation(project, &job.id, job.version)
            .is_err()
    );
    assert_eq!(
        repository.condition_evaluation(project, &job.id).unwrap(),
        job
    );
    let after: (String, Option<String>) = db
        .query_row(
            "SELECT progress_json,last_batch_id FROM story_condition_evaluations_v1 WHERE id=?1",
            [&job.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(after, before);
    // Background execution records a failure, not a successful scan or an
    // endlessly retried page. The injected commit error leaves progress intact.
    assert!(novel_codex_story_runtime::condition_evaluation_tick(path).unwrap());
    let failed = repository.condition_evaluation(project, &job.id).unwrap();
    assert_eq!(
        (failed.status, failed.scanned_sources, failed.result),
        (Status::Failed, 0, None)
    );
    assert!(!novel_codex_story_runtime::condition_evaluation_tick(path).unwrap());
    let after_failure: (String, Option<String>) = db
        .query_row(
            "SELECT progress_json,last_batch_id FROM story_condition_evaluations_v1 WHERE id=?1",
            [&job.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(after_failure, before);
    db.execute_batch("DROP TRIGGER condition_test_abort;")
        .unwrap();
    let job = repository
        .start_condition_evaluation(project, target.clone())
        .unwrap();
    let first = repository
        .advance_condition_evaluation(project, &job.id, job.version)
        .unwrap();
    assert_eq!(
        (first.status, first.scanned_sources, first.result.clone()),
        (Status::Running, 1, None)
    );
    let paused = repository
        .control_condition_evaluation(project, &job.id, first.version, Control::Pause)
        .unwrap();
    let (status, paused_projection) = super::call(
        path,
        "GET",
        &projection_uri,
        Value::Null,
        super::Role::Author,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{paused_projection}");
    assert_eq!(
        paused_projection["entries"][0]["eventConditionAssessment"]["evaluationJob"],
        json!({"id":job.id,"status":"paused","version":paused.version,"scannedSources":1})
    );
    assert_eq!(
        paused_projection["entries"][0]["eventConditionAssessment"]["result"],
        "unknown"
    );
    assert!(
        repository
            .advance_condition_evaluation(project, &job.id, paused.version)
            .is_err()
    );
    assert!(!novel_codex_story_runtime::condition_evaluation_tick(path).unwrap());
    drop(repository);
    let mut repository = SqliteStoryRepository::open(path).unwrap();
    assert_eq!(
        repository.condition_evaluation(project, &job.id).unwrap(),
        paused
    );
    let mut current = repository
        .control_condition_evaluation(project, &job.id, paused.version, Control::Resume)
        .unwrap();
    assert!(
        repository
            .advance_condition_evaluation(project, &job.id, first.version)
            .is_err()
    );
    // A newly queued request joins behind the older runnable request, then
    // receives a turn instead of waiting for all 203 sources of the first job.
    let peer = repository
        .start_condition_evaluation(project, short_target)
        .unwrap();
    assert!(novel_codex_story_runtime::condition_evaluation_tick(path).unwrap());
    let next = repository.condition_evaluation(project, &job.id).unwrap();
    assert_eq!(next.scanned_sources, current.scanned_sources + 1);
    current = next;
    assert_eq!(
        repository.condition_evaluation(project, &peer.id).unwrap(),
        peer
    );
    assert!(novel_codex_story_runtime::condition_evaluation_tick(path).unwrap());
    let peer = repository.condition_evaluation(project, &peer.id).unwrap();
    assert_eq!((peer.status, peer.scanned_sources), (Status::Running, 1));
    repository
        .control_condition_evaluation(project, &peer.id, peer.version, Control::Cancel)
        .unwrap();
    for _ in 0..203 {
        assert!(novel_codex_story_runtime::condition_evaluation_tick(path).unwrap());
        let next = repository.condition_evaluation(project, &job.id).unwrap();
        assert_eq!(next.scanned_sources, current.scanned_sources + 1);
        current = next;
        if current.status == Status::Completed {
            break;
        }
        assert!(current.result.is_none());
    }
    assert_eq!(
        (current.status, current.scanned_sources),
        (Status::Completed, 203)
    );
    assert!(!novel_codex_story_runtime::condition_evaluation_tick(path).unwrap());
    let report = current.result.as_ref().unwrap();
    let (status, projection) = super::call(
        path,
        "GET",
        &projection_uri,
        Value::Null,
        super::Role::Author,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{projection}");
    assert_eq!(
        &projection["entries"][0]["eventConditionAssessment"],
        report
    );
    let (status, reader) = super::call(
        path,
        "GET",
        &projection_uri.replace("perspective=world", "perspective=reader&readingChapter=47"),
        Value::Null,
        super::Role::Author,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{reader}");
    assert!(
        reader["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["eventConditionAssessment"].is_null())
    );
    assert_eq!(
        (
            &report["result"],
            &report["coverage"]["complete"],
            &report["coverage"]["sources"]["scannedSources"]
        ),
        (&json!("both"), &json!(true), &json!(203))
    );
    assert_eq!(
        report["sourceClaims"],
        json!([{"sourceBatchId":std::cmp::min(positive,"condition-page-000"),"assertionIndex":0},{"sourceBatchId":"zz-final-counter","assertionIndex":0}])
    );
    assert_eq!(
        repository
            .start_condition_evaluation(project, target)
            .unwrap(),
        current
    );
    assert!(
        repository
            .advance_condition_evaluation(project, &job.id, current.version)
            .is_err()
    );
    assert_eq!(
        SqliteStoryRepository::open(path)
            .unwrap()
            .condition_evaluation(project, &job.id)
            .unwrap(),
        current
    );
    let uri = format!(
        "/api/projects/{}/condition-evaluations/{}",
        project.as_str(),
        job.id
    );
    let (status, read) = super::call(path, "GET", &uri, Value::Null, super::Role::Author).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(read, serde_json::to_value(&current).unwrap());
    let explanations = format!("{uri}/explanations");
    let (status, _) = super::call(
        path,
        "GET",
        &explanations,
        Value::Null,
        super::Role::Candidate,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
    let (status, first_page) = super::call(
        path,
        "GET",
        &format!("{explanations}?limit=1"),
        Value::Null,
        super::Role::Author,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{first_page}");
    assert_eq!(first_page["data"].as_array().unwrap().len(), 1);
    let cursor = first_page["nextCursor"].as_str().unwrap();
    let escaped = cursor
        .bytes()
        .map(|byte| format!("%{byte:02X}"))
        .collect::<String>();
    let second_uri = format!("{explanations}?limit=1&cursor={escaped}");
    let (status, second_page) =
        super::call(path, "GET", &second_uri, Value::Null, super::Role::Author).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{second_page}");
    assert!(second_page["nextCursor"].is_null());
    let (status, all) =
        super::call(path, "GET", &explanations, Value::Null, super::Role::Author).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(
        all["data"],
        json!([first_page["data"][0], second_page["data"][0]])
    );
    assert_eq!(all["data"][0]["assessment"]["result"], "both");
    for row in all["data"].as_array().unwrap() {
        for side in ["support", "refute"] {
            let excerpt = &row["excerpts"][side];
            let reference = &row["assessment"][side];
            let (object,revision,body):(String,String,String)=db.query_row(
                "SELECT b.object_id,b.source_revision_id,json_extract(r.content_json,'$.body') FROM story_modeling_batches_v2 b JOIN story_revisions_v2 r ON r.project_id=b.project_id AND r.object_id=b.object_id AND r.revision_id=b.source_revision_id WHERE b.project_id=?1 AND b.batch_id=?2",
                rusqlite::params![project.as_str(),reference["sourceBatchId"].as_str().unwrap()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
            ).unwrap();
            let start = excerpt["start"].as_u64().unwrap() as usize;
            let end = excerpt["end"].as_u64().unwrap() as usize;
            assert!(end > start && end - start <= 160);
            assert_eq!(
                *excerpt,
                json!({"sourceBatchId":reference["sourceBatchId"],"assertionIndex":reference["assertionIndex"],"sourceObjectId":object,"sourceRevisionId":revision,"quote":body.chars().skip(start).take(end-start).collect::<String>(),"start":start,"end":end,"truncated":false})
            );
        }
    }
    assert!(all.to_string().len() <= 9000);
    let mut wrong: Value = serde_json::from_str(cursor).unwrap();
    wrong["job"] = json!("another-job");
    assert!(
        repository
            .condition_explanations(project, &job.id, Some(&wrong.to_string()), 1)
            .is_err()
    );
    assert!(
        repository
            .condition_explanations(project, &job.id, None, 17)
            .is_err()
    );
    let checkpoint: String = db
        .query_row(
            "SELECT progress_json FROM story_condition_evaluations_v1 WHERE id=?1",
            [&job.id],
            |row| row.get(0),
        )
        .unwrap();
    db.execute(
        "UPDATE story_condition_evaluations_v1 SET progress_json='{}' WHERE id=?1",
        [&job.id],
    )
    .unwrap();
    assert!(
        repository
            .condition_explanations(project, &job.id, None, 1)
            .is_err()
    );
    assert!(repository.condition_evaluation(project, &job.id).is_err());
    db.execute(
        "UPDATE story_condition_evaluations_v1 SET progress_json=?1 WHERE id=?2",
        params![checkpoint, job.id],
    )
    .unwrap();
    db.execute("DELETE FROM story_snapshot_members_v2 WHERE snapshot_id=?1 AND object_id='draft-condition-page-000'",[snapshot]).unwrap();
    let stale = repository.condition_evaluation(project, &job.id).unwrap();
    assert_eq!(
        (stale.status, stale.result.clone()),
        (Status::Stale, None::<Value>)
    );
    let (status, read) = super::call(path, "GET", &uri, Value::Null, super::Role::Author).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(read, serde_json::to_value(stale).unwrap());
    let (status, _) = super::call(path, "GET", &second_uri, Value::Null, super::Role::Author).await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);
    let (status, invalidated_projection) = super::call(
        path,
        "GET",
        &projection_uri,
        Value::Null,
        super::Role::Author,
    )
    .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "{invalidated_projection}"
    );
    let invalidated = &invalidated_projection["entries"][0]["eventConditionAssessment"];
    assert_eq!(invalidated["result"], "unknown");
    assert!(invalidated["explanationJobId"].is_null());
}
