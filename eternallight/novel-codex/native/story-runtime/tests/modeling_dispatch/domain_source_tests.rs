use super::*;
use pretty_assertions::assert_eq;
use rusqlite::params;

#[test]
fn domain_registration_failure_rolls_back_the_entire_dispatch() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registration.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 1);
    edit(&mut repo, &project, "registration");
    let db = rusqlite::Connection::open(&path).unwrap();
    let counts = || {
        db.query_row("SELECT (SELECT COUNT(*) FROM story_work_tasks_v1),(SELECT COUNT(*) FROM story_work_steps_v1),(SELECT COUNT(*) FROM story_modeling_batches_v2),(SELECT COUNT(*) FROM story_domain_task_sources_v1)",[],|row| Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?,row.get::<_,i64>(3)?))).unwrap()
    };
    let before = counts();
    db.execute_batch("CREATE TRIGGER fail_domain_registration BEFORE INSERT ON story_domain_task_sources_v1 BEGIN SELECT RAISE(ABORT,'source registration failed'); END;").unwrap();
    assert!(repo.dispatch_modeling_intent(&project, i64::MAX).is_err());
    assert_eq!(counts(), before);
    db.execute_batch("DROP TRIGGER fail_domain_registration;")
        .unwrap();
    let work = dispatch(&mut repo, &project);
    assert_eq!(repo.work_task(&project, &work.id).unwrap(), work);
}

#[test]
fn client_cannot_inject_domain_identity_or_downgrade_a_broken_registration() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("injection.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 1);
    edit(&mut repo, &project, "injection");
    let work = dispatch(&mut repo, &project);
    for manifest in [
        json!({"domainTaskSource":{}}),
        json!({"domainInstruction":"自行覆盖"}),
        json!({"authorMessageId":"invented-author"}),
    ] {
        let mut request = claim(&project, &work, /*number*/ 1);
        request.input_manifest_json = manifest.to_string();
        assert_eq!(
            repo.create_task(request),
            Err(RepositoryError::TaskBindingInvalid)
        );
        assert_eq!(repo.work_task(&project, &work.id).unwrap(), work);
    }
    let db = rusqlite::Connection::open(&path).unwrap();
    let stored: String = db
        .query_row(
            "SELECT source_json FROM story_domain_task_sources_v1 WHERE step_id=?1",
            [&work.steps[0].id],
            |row| row.get(0),
        )
        .unwrap();
    db.execute(
        "DELETE FROM story_domain_task_sources_v1 WHERE step_id=?1",
        [&work.steps[0].id],
    )
    .unwrap();
    assert_eq!(
        repo.create_task(claim(&project, &work, /*number*/ 1)),
        Err(RepositoryError::TaskBindingInvalid)
    );
    db.execute(
        "INSERT INTO story_domain_task_sources_v1(step_id,source_json) VALUES(?1,?2)",
        params![work.steps[0].id, stored],
    )
    .unwrap();
    let mut damaged_source: Value = serde_json::from_str(&stored).unwrap();
    damaged_source["sourceRevisionId"] = json!("another-revision");
    db.execute(
        "UPDATE story_domain_task_sources_v1 SET source_json=?1 WHERE step_id=?2",
        params![damaged_source.to_string(), work.steps[0].id],
    )
    .unwrap();
    assert_eq!(
        repo.create_task(claim(&project, &work, /*number*/ 1)),
        Err(RepositoryError::TaskBindingInvalid)
    );
    assert_eq!(repo.work_task(&project, &work.id).unwrap(), work);
    db.execute(
        "UPDATE story_domain_task_sources_v1 SET source_json=?1 WHERE step_id=?2",
        params![stored, work.steps[0].id],
    )
    .unwrap();
    let mut wrong_generation: Value = serde_json::from_str(&stored).unwrap();
    wrong_generation["intentGeneration"] =
        json!(wrong_generation["intentGeneration"].as_i64().unwrap() + 1);
    db.execute(
        "UPDATE story_domain_task_sources_v1 SET source_json=?1 WHERE step_id=?2",
        params![wrong_generation.to_string(), work.steps[0].id],
    )
    .unwrap();
    assert_eq!(
        repo.create_task(claim(&project, &work, /*number*/ 1)),
        Err(RepositoryError::TaskBindingInvalid)
    );
    db.execute(
        "UPDATE story_domain_task_sources_v1 SET source_json=?1 WHERE step_id=?2",
        params![stored, work.steps[0].id],
    )
    .unwrap();
    let mut request = claim(&project, &work, /*number*/ 1);
    request.input_manifest_json =
        json!({"authorIntent":"宿主生成的包装，不是作者原话"}).to_string();
    let task = repo.create_task(request).unwrap();
    let manifest: Value = serde_json::from_str(&task.input_manifest_json).unwrap();
    assert!(manifest.get("authorIntent").is_none());
    assert_eq!(manifest["domainTaskSource"]["workTaskId"], work.id);
    repo.assert_task_inputs_current(&task).unwrap();
}

#[test]
fn corrupted_domain_binding_blocks_reads_publication_completion_and_retry() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("binding.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 1);
    edit(&mut repo, &project, "binding");
    let work = dispatch(&mut repo, &project);
    let task = repo
        .create_task(claim(&project, &work, /*number*/ 1))
        .unwrap();
    let original: Value = serde_json::from_str(&task.input_manifest_json).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let batch = work.steps[0].target["batchId"].as_str().unwrap();
    for key in [
        "workTaskId",
        "stepId",
        "batchId",
        "sourceRevisionId",
        "chapterId",
        "objectId",
        "intentGeneration",
    ] {
        let mut changed = original.clone();
        changed["domainTaskSource"][key] = json!("wrong-source");
        db.execute(
            "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
            params![changed.to_string(), task.task_id],
        )
        .unwrap();
        let damaged = repo.task(&project, &task.task_id).unwrap().unwrap();
        assert_eq!(
            repo.assert_task_inputs_current(&damaged),
            Err(RepositoryError::TaskBindingInvalid)
        );
        assert_eq!(
            repo.publish_modeling_batch(PublishModelingBatch {
                project_id: &project,
                batch_id: batch,
                task_id: &task.task_id,
                thread_id: &task.thread_id,
                entities: &[],
                assertions: &[],
                now: 2
            })
            .err(),
            Some(RepositoryError::TaskBindingInvalid)
        );
        assert_eq!(
            repo.complete_task(
                &project,
                &task.task_id,
                &task.thread_id,
                "done".into(),
                /*now*/ 2
            ),
            Err(RepositoryError::TaskBindingInvalid)
        );
    }
    for key in [
        "authorIntent",
        "authorMessageId",
        "candidateInstruction",
        "planningReviewAuthorIntentBinding",
    ] {
        let mut mixed = original.clone();
        mixed[key] = json!("invented-author");
        db.execute(
            "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
            params![mixed.to_string(), task.task_id],
        )
        .unwrap();
        assert_eq!(
            repo.assert_task_inputs_current(&repo.task(&project, &task.task_id).unwrap().unwrap()),
            Err(RepositoryError::TaskBindingInvalid)
        );
    }
    db.execute(
        "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
        params![original.to_string(), task.task_id],
    )
    .unwrap();
    repo.update_task(
        &project,
        &task.task_id,
        &task.thread_id,
        TaskUpdate {
            status: "failed".into(),
            error: Some("interrupted".into()),
            result_summary: None,
            now: 3,
        },
    )
    .unwrap();
    let blocked = repo.work_task(&project, &work.id).unwrap();
    let resumed = repo
        .control_work_task(
            &project,
            &work.id,
            "resume",
            blocked.version,
            /*now*/ 4,
        )
        .unwrap();
    let mut corrupted = original.clone();
    corrupted["domainTaskSource"]["workTaskId"] = json!("another-work");
    db.execute(
        "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
        params![corrupted.to_string(), task.task_id],
    )
    .unwrap();
    assert_eq!(
        repo.create_task(claim(&project, &resumed, /*number*/ 2)),
        Err(RepositoryError::TaskBindingInvalid)
    );
    assert_eq!(repo.work_task(&project, &work.id).unwrap(), resumed);
    db.execute(
        "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
        params![original.to_string(), task.task_id],
    )
    .unwrap();
    drop(repo);
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let next = repo
        .create_task(claim(&project, &resumed, /*number*/ 2))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&next.input_manifest_json).unwrap()["domainTaskSource"],
        original["domainTaskSource"]
    );
    repo.publish_modeling_batch(PublishModelingBatch {
        project_id: &project,
        batch_id: batch,
        task_id: &next.task_id,
        thread_id: &next.thread_id,
        entities: &[],
        assertions: &[],
        now: 5,
    })
    .unwrap();
    repo.complete_task(
        &project,
        &next.task_id,
        &next.thread_id,
        "published".into(),
        /*now*/ 6,
    )
    .unwrap();
    assert_eq!(
        repo.work_task(&project, &work.id).unwrap().status,
        "completed"
    );
}

#[tokio::test]
async fn domain_registration_is_not_a_grant_and_retries_share_settled_engine_usage() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("budget.sqlite3");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 1);
    edit(&mut repo, &project, "budget-source");
    let work = dispatch(&mut repo, &project);
    assert!(
        repo.create_funded_task(claim(&project, &work, /*number*/ 1), /*now*/ 1)
            .is_err()
    );
    assert_eq!(repo.work_task(&project, &work.id).unwrap(), work);
    let author = "domain-source-author-123456789012345678";
    let app = router(RuntimeState::new(
        path.clone(),
        author.into(),
        "domain-source-candidate-12345678901234".into(),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/projects/{project}/creative-workspace"))
                .header("authorization", format!("Bearer {author}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let workspace: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let authorization = WorkBudgetAuthorization {
        expected_version: work.version,
        expected_project_revision: 0,
        expected_source_digest: workspace["sourceDigest"].as_str().unwrap().into(),
        max_requests: 3,
        max_tokens: 700,
        deadline_at: now + 600,
    };
    repo.authorize_work_budget(&project, &work.id, &authorization, now)
        .unwrap();
    let first = repo
        .create_funded_task(claim(&project, &work, /*number*/ 1), now)
        .unwrap();
    let grant = repo
        .task_provider_budget(&project, &first.task_id, now)
        .unwrap();
    let original: Value = serde_json::from_str(&first.input_manifest_json).unwrap();
    let mut damaged = original.clone();
    damaged["domainTaskSource"]["batchId"] = json!("different-batch");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
        params![damaged.to_string(), first.task_id],
    )
    .unwrap();
    assert_eq!(
        repo.task_provider_budget(&project, &first.task_id, now),
        Err(RepositoryError::TaskBindingInvalid)
    );
    db.execute(
        "UPDATE story_tasks_v2 SET input_manifest_json=?1 WHERE task_id=?2",
        params![original.to_string(), first.task_id],
    )
    .unwrap();
    repo.update_task(
        &project,
        &first.task_id,
        &first.thread_id,
        TaskUpdate {
            status: "failed".into(),
            error: Some("interrupted".into()),
            result_summary: None,
            now: 3,
        },
    )
    .unwrap();
    let blocked = repo.work_task(&project, &work.id).unwrap();
    let resumed = repo
        .control_work_task(
            &project,
            &work.id,
            "resume",
            blocked.version,
            /*now*/ 4,
        )
        .unwrap();
    assert!(
        repo.create_funded_task(claim(&project, &resumed, /*number*/ 2), now)
            .is_err()
    );
    assert_eq!(repo.work_task(&project, &work.id).unwrap(), resumed);
    let engine_path = path.with_extension("engine.sqlite3");
    let mut engine = eternallight_engine_core::PersistentBudget::new(&engine_path,&first.task_id,
        json!({"options":{"id":grant.budget_id,"maxRequests":grant.max_requests,"maxTokens":grant.max_tokens,"startedAt":1000},"now":1000})).unwrap();
    engine
        .apply(
            "budget.reserve",
            json!({"parameters":{"maxOutputTokens":100},"now":1001}),
        )
        .unwrap();
    engine.complete("failed").unwrap();
    let receipt =
        eternallight_engine_core::PersistentBudget::receipt(&engine_path, &first.task_id).unwrap();
    let charged = repo
        .charge_work_provider_usage(&project, &work.id, &first.task_id, &receipt)
        .unwrap();
    assert_eq!((charged.used_requests, charged.unknown_tokens), (1, 100));
    assert_eq!(
        repo.charge_work_provider_usage(&project, &work.id, &first.task_id, &receipt)
            .unwrap(),
        charged
    );
    drop(repo);
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let second = repo
        .create_funded_task(claim(&project, &resumed, /*number*/ 2), now)
        .unwrap();
    let next = repo
        .task_provider_budget(&project, &second.task_id, now)
        .unwrap();
    assert_eq!(
        next,
        TaskProviderBudget {
            task_id: second.task_id.clone(),
            work_task_id: work.id.clone(),
            budget_id: format!("budget-{}", second.task_id),
            max_requests: 2,
            max_tokens: 600,
            deadline_at: now + 600
        }
    );
    assert_eq!(
        repo.work_budget_authorization(&project, &work.id).unwrap(),
        Some(authorization)
    );
    assert_eq!(
        serde_json::from_str::<Value>(&second.input_manifest_json).unwrap()["domainTaskSource"],
        original["domainTaskSource"]
    );
}
