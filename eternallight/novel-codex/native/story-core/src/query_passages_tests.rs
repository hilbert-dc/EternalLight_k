use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    BranchId, ChapterProjection, CommitId, CreateTaskRequest, IdempotencyKey, ObjectId, ProjectId,
    ProjectRecord, RevisionId, SaveCandidateRequest, SearchQuery, SearchScope, SnapshotId,
    SqliteStoryRepository, StoryQueryPlan, TaskRecord,
};

#[test]
fn missing_derived_search_rows_cannot_remove_authorized_source_passages() {
    let (_directory, mut repo, project) = fixture();
    save(
        &mut repo,
        &project,
        /*chapter*/ 1,
        json!({"body":"港城雨夜".repeat(200)}),
    );
    let task = task(&mut repo, &project);
    let plan = plan();
    let before = repo.query_task_plan(&task, &plan, /*now*/ 5).unwrap();
    assert_eq!(before["steps"][0]["results"].as_array().unwrap().len(), 2);
    assert!(before["steps"][0]["nextCursor"].is_string());
    repo.connection
        .execute(
            "DELETE FROM story_search_grams_v2 WHERE project_id = ?1",
            [project.as_str()],
        )
        .unwrap();
    repo.connection
        .execute(
            "DELETE FROM story_search_documents_v2 WHERE project_id = ?1",
            [project.as_str()],
        )
        .unwrap();
    assert_eq!(
        repo.query_task_plan(&task, &plan, /*now*/ 5).unwrap(),
        before
    );
}

#[test]
fn source_walk_skips_empty_chapters_and_preserves_scene_positions() {
    let (_directory, mut repo, project) = fixture();
    save(
        &mut repo,
        &project,
        /*chapter*/ 1,
        json!({"body":"","sceneGroups":[]}),
    );
    save(
        &mut repo,
        &project,
        /*chapter*/ 2,
        json!({"sceneGroups":[{"body":""},{"body":"雨🙂\0夜"},{"body":"离港"}]}),
    );
    save(
        &mut repo,
        &project,
        /*chapter*/ 3,
        json!({"body":"后章","sceneGroups":[{"body":"不应重复读取正文副本"}]}),
    );
    let task = task(&mut repo, &project);
    let mut request: Value = serde_json::to_value(plan()).unwrap();
    request["steps"][0]["operation"]["limit"] = json!(4);
    let request = serde_json::from_value(request).unwrap();
    let report = repo.query_task_plan(&task, &request, /*now*/ 5).unwrap();
    let rows = report["steps"][0]["results"].as_array().unwrap();
    let anchors = rows
        .iter()
        .map(|row| report["evidence"][row["evidenceRef"].as_str().unwrap()].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        anchors,
        vec![
            json!({"sourceObjectId":"draft-chapter-2","sourceRevisionId":"revision-2","sourcePath":"/sceneGroups/1/body","quote":"雨🙂\0夜","start":0,"end":4}),
            json!({"sourceObjectId":"draft-chapter-2","sourceRevisionId":"revision-2","sourcePath":"/sceneGroups/2/body","quote":"离港","start":0,"end":2}),
            json!({"sourceObjectId":"draft-chapter-3","sourceRevisionId":"revision-3","sourcePath":"/body","quote":"后章","start":0,"end":2}),
        ]
    );
    assert_eq!(report["steps"][0]["nextCursor"], Value::Null);
    assert_eq!(report["steps"][0]["coverage"]["endOfScopeFromCursor"], true);
    assert_eq!(report["steps"][0]["coverage"]["fullScanCertified"], false);
}

#[test]
fn paging_decodes_only_the_selected_source_and_fails_when_that_source_is_corrupt() {
    let (_directory, mut repo, project) = fixture();
    save(
        &mut repo,
        &project,
        /*chapter*/ 1,
        json!({"body":"港城雨夜".repeat(200)}),
    );
    save(
        &mut repo,
        &project,
        /*chapter*/ 2,
        json!({"body":"之后的来源"}),
    );
    let task = task(&mut repo, &project);
    let baseline = repo.query_task_plan(&task, &plan(), /*now*/ 5).unwrap();
    // Fixture corruption must not make unrelated source pages decode an entire book.
    repo.connection.execute("UPDATE story_revisions_v2 SET content_json = 'broken' WHERE revision_id = 'revision-2'",[]).unwrap();
    assert_eq!(
        repo.query_task_plan(&task, &plan(), /*now*/ 5).unwrap(),
        baseline
    );
    let mut cursor: Value =
        serde_json::from_str(baseline["steps"][0]["nextCursor"].as_str().unwrap()).unwrap();
    cursor["position"] = json!({"chapter":2,"object":"draft-chapter-2","field":0,"start":0});
    let mut selected: Value = serde_json::to_value(plan()).unwrap();
    selected["steps"][0]["operation"]["cursor"] = json!(cursor.to_string());
    let selected = serde_json::from_value(selected).unwrap();
    assert!(matches!(
        repo.query_task_plan(&task, &selected, /*now*/ 5),
        Err(crate::RepositoryError::CorruptState)
    ));
}

#[test]
fn project_retrieval_uses_the_selected_historical_revision_for_passages_and_text() {
    let (_directory, mut repo, project) = fixture();
    save(
        &mut repo,
        &project,
        /*chapter*/ 1,
        json!({"body":"林夕把密函藏进白塔。"}),
    );
    let content = json!({"chapterNumber":1,"title":"白塔","body":"林夕把密函烧掉。"}).to_string();
    let digest = format!("{:x}", Sha256::digest(content.as_bytes()));
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("draft-chapter-1").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse("revision-1-newer").unwrap(),
        commit_id: CommitId::parse("commit-1-newer").unwrap(),
        idempotency_key: IdempotencyKey::parse("save-1-newer").unwrap(),
        request_digest: digest.clone(),
        content_digest: digest,
        content_json: content,
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse("snapshot-1").unwrap(),
        next_snapshot_id: SnapshotId::parse("snapshot-1-newer").unwrap(),
        expected_candidate_revision_id: Some(RevisionId::parse("revision-1").unwrap()),
        chapter_projection: Some(ChapterProjection {
            chapter_id: "chapter-1".into(),
            number: 1,
            title: "白塔".into(),
        }),
        task_output: None,
        linked_impact_ids: Vec::new(),
    })
    .unwrap();
    repo.set_working_baseline(
        &project,
        "draft",
        "chapter-1",
        &RevisionId::parse("revision-1").unwrap(),
        0,
        5,
    )
    .unwrap();
    save(
        &mut repo,
        &project,
        /*chapter*/ 2,
        json!({"body":"林夕巡过港口。"}),
    );
    let task = task(&mut repo, &project);
    let report = repo.query_task_plan(&task, &plan(), 5).unwrap();
    let evidence_ref = report["steps"][0]["results"][0]["evidenceRef"]
        .as_str()
        .unwrap();
    assert_eq!(
        report["evidence"][evidence_ref]["sourceRevisionId"],
        "revision-1"
    );
    assert_eq!(
        report["evidence"][evidence_ref]["quote"],
        "林夕把密函藏进白塔。"
    );
    let text_plan: StoryQueryPlan = serde_json::from_value(json!({
        "question":"密函发生了什么？","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[{"id":"text","operation":{"type":"text","query":"藏进","limit":5}}]
    }))
    .unwrap();
    let matches = repo.query_task_plan(&task, &text_plan, 5).unwrap();
    assert_eq!(
        matches["steps"][0]["results"][0]["sourceRevisionId"],
        "revision-1"
    );
    let absent_plan: StoryQueryPlan = serde_json::from_value(json!({
        "question":"密函发生了什么？","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[{"id":"text","operation":{"type":"text","query":"烧掉","limit":5}}]
    }))
    .unwrap();
    let absent = repo.query_task_plan(&task, &absent_plan, 5).unwrap();
    assert!(absent["steps"][0]["results"].as_array().unwrap().is_empty());
    let structured: StoryQueryPlan = serde_json::from_value(json!({
        "question":"林夕知道什么？","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[{"id":"state","operation":{"type":"state","entityId":"linxi",
            "predicate":"knows","perspective":"character","ownerId":"linxi","epistemicTime":1}}]
    }))
    .unwrap();
    assert!(matches!(repo.query_task_plan(&task, &structured, 5),
        Err(crate::RepositoryError::InvalidContent(message)) if message.contains("selected source")));
    let page = repo
        .search_documents(SearchQuery {
            project_id: &project,
            snapshot_id: None,
            text: "林夕",
            kind: Some("draft"),
            scope: SearchScope::All,
            limit: 1,
            cursor: None,
        })
        .unwrap();
    assert_eq!(page["results"][0]["sourceRevisionId"], "revision-1");
    let cursor = page["nextCursor"].as_str().unwrap().to_owned();
    repo.set_working_baseline(
        &project,
        "draft",
        "chapter-1",
        &RevisionId::parse("revision-1-newer").unwrap(),
        1,
        6,
    )
    .unwrap();
    assert!(matches!(
        repo.search_documents(SearchQuery {
            project_id: &project,
            snapshot_id: None,
            text: "林夕",
            kind: Some("draft"),
            scope: SearchScope::All,
            limit: 1,
            cursor: Some(&cursor),
        }),
        Err(crate::RepositoryError::StaleSearchCursor)
    ));
}

fn fixture() -> (tempfile::TempDir, SqliteStoryRepository, ProjectId) {
    let directory = tempfile::tempdir().unwrap();
    let mut repo = SqliteStoryRepository::open(directory.path().join("源文.sqlite3")).unwrap();
    let project = ProjectId::parse("source-scan").unwrap();
    let branch = BranchId::parse("main").unwrap();
    let initial = SnapshotId::parse("snapshot-initial").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "原文召回".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        branch.clone(),
        initial.clone(),
    )
    .unwrap();
    (directory, repo, project)
}

fn save(repo: &mut SqliteStoryRepository, project: &ProjectId, chapter: usize, mut content: Value) {
    let branch = BranchId::parse("main").unwrap();
    let head = repo
        .branch_head(project, &branch)
        .unwrap()
        .unwrap()
        .snapshot_id;
    content["title"] = json!("白塔");
    content["chapterNumber"] = json!(chapter);
    let content = content.to_string();
    let digest = format!("{:x}", Sha256::digest(content.as_bytes()));
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: branch,
        object_id: ObjectId::parse(format!("draft-chapter-{chapter}")).unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse(format!("revision-{chapter}")).unwrap(),
        commit_id: CommitId::parse(format!("commit-{chapter}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(format!("save-{chapter}")).unwrap(),
        request_digest: digest.clone(),
        content_digest: digest,
        content_json: content,
        expected_project_revision: 0,
        expected_snapshot_id: head,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{chapter}")).unwrap(),
        expected_candidate_revision_id: None,
        chapter_projection: Some(ChapterProjection {
            chapter_id: format!("chapter-{chapter}"),
            number: chapter as i64,
            title: "白塔".into(),
        }),
        task_output: None,
        linked_impact_ids: Vec::new(),
    })
    .unwrap();
}

fn task(repo: &mut SqliteStoryRepository, project: &ProjectId) -> TaskRecord {
    repo.create_task(CreateTaskRequest {
        task_id: "passage-task".into(),
        project_id: project.clone(),
        thread_id: "passage-thread".into(),
        operation: "advisory".into(),
        input_revision: 0,
        model: None,
        target_json: json!({"type":"project"}).to_string(),
        input_manifest_json: "{}".into(),
        capability_hash: "a".repeat(64),
        capability_expires_at: i64::MAX,
    })
    .unwrap()
}

fn plan() -> StoryQueryPlan {
    serde_json::from_value(json!({"question":"原文中的行为意味着什么？","byteBudget":9000,"ranking":{"type":"literal"},
        "steps":[{"id":"read","operation":{"type":"passages","cursor":null,"maxChars":160,"limit":2}}]})).unwrap()
}
