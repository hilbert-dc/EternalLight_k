use novel_codex_story_core::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[path = "../examples/support/long_work_fixture.rs"]
mod fixture;

fn save_commitment(
    repo: &mut SqliteStoryRepository,
    project: &ProjectId,
    id: &str,
    chapter: usize,
    start: usize,
) {
    let revision = RevisionId::parse(format!("revision-{chapter}")).unwrap();
    let source: Value =
        serde_json::from_str(&repo.revision_content(&revision).unwrap().unwrap()).unwrap();
    let quote = source["body"]
        .as_str()
        .unwrap()
        .chars()
        .skip(start)
        .take(/*n*/ 4)
        .collect::<String>();
    let content = json!({"payload":{
        "id":id,"kind":"suspense","summary":format!("承诺 {id}"),"relatedEntityIds":["unresolved-future-person"],
        "plotThreadId":"unresolved-future-thread","characterArcId":null,
        "setupEvidenceRefs":[{"objectId":format!("draft-chapter-{chapter}"),"revisionId":revision.as_str(),
            "quote":quote,"start":start,"end":start+4}],
        "fulfillmentCriteria":"后续正文呈现结果","targetWindow":{"kind":"open"},
        "status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null
    }});
    save_value(repo, project, id, content);
}

fn save_value(repo: &mut SqliteStoryRepository, project: &ProjectId, id: &str, content: Value) {
    let content = content.to_string();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse(format!("commitment-{id}")).unwrap(),
        object_kind: "commitment".into(),
        revision_id: RevisionId::parse(format!("promise-{id}")).unwrap(),
        commit_id: CommitId::parse(format!("promise-commit-{id}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(format!("promise-save-{id}")).unwrap(),
        request_digest: content.clone(),
        content_digest: content.clone(),
        content_json: content,
        expected_project_revision: repo.project_revision(project).unwrap(),
        expected_snapshot_id: repo
            .branch_head(project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        next_snapshot_id: SnapshotId::parse(format!("promise-snapshot-{id}")).unwrap(),
        expected_candidate_revision_id: None,
        chapter_projection: None,
        task_output: None,
        linked_impact_ids: vec![],
    })
    .unwrap();
}

fn task(
    repo: &mut SqliteStoryRepository,
    project: &ProjectId,
    id: &str,
    operation: &str,
    target: Value,
) -> TaskRecord {
    repo.create_task(CreateTaskRequest {
        task_id: id.into(),
        project_id: project.clone(),
        thread_id: format!("thread-{id}"),
        operation: operation.into(),
        input_revision: repo.project_revision(project).unwrap(),
        model: None,
        target_json: target.to_string(),
        input_manifest_json: "{}".into(),
        capability_hash: "b".repeat(/*n*/ 64),
        capability_expires_at: i64::MAX,
    })
    .unwrap()
}

#[test]
fn scope_filters_future_records_before_limits_and_withholds_unbound_metadata() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 4);
    for index in 0..20 {
        save_commitment(
            &mut repo,
            &project,
            &format!("a-future-{index}"),
            /*chapter*/ 4,
            /*start*/ 0,
        );
    }
    save_commitment(
        &mut repo,
        &project,
        "z-visible",
        /*chapter*/ 1,
        /*start*/ 0,
    );
    let plan = task(
        &mut repo,
        &project,
        "rolling",
        "generatePlan",
        json!({"planFromChapter":3}),
    );
    let projected = repo.task_context_commitments(&plan).unwrap();
    assert_eq!(projected["entries"].as_array().unwrap().len(), 1);
    assert_eq!(projected["entries"][0]["id"], "z-visible");
    assert_eq!(
        projected["entries"][0]["authority"],
        "candidateNarrativeInterpretation"
    );
    assert_eq!(
        projected["entries"][0]["literaryFulfillmentVerified"],
        false
    );
    assert_eq!(projected["coverage"]["globalCompletenessClaimed"], false);
    assert!(!projected.to_string().contains("future"));
    let all = task(&mut repo, &project, "outline", "generateOutline", json!({}));
    let projected = repo.task_context_commitments(&all).unwrap();
    assert!(projected["entries"].as_array().unwrap().len() <= 16);
    assert_eq!(projected["coverage"]["truncated"], true);
    assert!(projected["entries"][0]["authorMetadata"].is_object());
    assert!(projected["entries"].to_string().len() <= 12_000);
}

#[test]
fn scene_prefix_and_pov_are_whole_item_disclosure_gates() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 2);
    save_commitment(
        &mut repo,
        &project,
        "hidden-scene",
        /*chapter*/ 2,
        /*start*/ 1100,
    );
    save_commitment(
        &mut repo, &project, "visible", /*chapter*/ 2, /*start*/ 0,
    );
    let writer = task(
        &mut repo,
        &project,
        "writer",
        "generateChapter",
        json!({
            "chapterId":"chapter-2","chapterNumber":2,"readingChapter":2,"sceneOrder":1,
            "scope":{"type":"throughScene","endSceneOrder":1},
            "readScope":{"type":"throughScene","endSceneOrder":1}
        }),
    );
    let write_only_fence = task(&mut repo, &project, "write-only-prefix", "generateChapter",
        json!({"chapterId":"chapter-2","chapterNumber":2,"readingChapter":2,"sceneOrder":1,
            "scope":{"type":"throughScene","endSceneOrder":1}}));
    assert_eq!(repo.task_context_commitments(&write_only_fence).unwrap()["entries"].as_array().unwrap().len(), 2,
        "a mutation fence does not restrict necessary reading");
    let projected = repo.task_context_commitments(&writer).unwrap();
    assert_eq!(projected["entries"].as_array().unwrap().len(), 1);
    assert_eq!(projected["entries"][0]["id"], "visible");
    assert!(!projected.to_string().contains("hidden-scene"));
    let pov = task(
        &mut repo,
        &project,
        "pov",
        "generateChapter",
        json!({
            "chapterId":"chapter-2","chapterNumber":2,"readingChapter":2,"sceneOrder":1,
            "povCharacterId":"person-2-0","storyTime":2
        }),
    );
    let hidden = repo.task_context_commitments(&pov).unwrap();
    assert_eq!(
        hidden,
        json!({"entries":[],"proposalContext":{"entries":[],"coverage":{"requested":false,"complete":true,"includedEntries":0}},"coverage":{
            "complete":false,"povEvidenceUnverified":true,"semanticCompleteness":"unknown","includedEntries":0
        }})
    );
    let mut forged = writer.clone();
    forged.target_json =
        json!({"chapterId":"chapter-2","readingChapter":2,"sceneOrder":2}).to_string();
    assert_eq!(
        repo.task_context_commitments(&forged),
        Err(RepositoryError::TaskBindingInvalid)
    );
    repo.update_task(
        &project,
        &writer.task_id,
        &writer.thread_id,
        TaskUpdate {
            status: "cancelled".into(),
            error: None,
            result_summary: None,
            now: 100,
        },
    )
    .unwrap();
    assert_eq!(
        repo.task_context_commitments(&writer),
        Err(RepositoryError::TaskNotWritable)
    );
}

#[test]
fn commitment_changes_invalidate_existing_task_context() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 1);
    let old = task(&mut repo, &project, "before", "generateOutline", json!({}));
    save_commitment(
        &mut repo, &project, "new", /*chapter*/ 1, /*start*/ 0,
    );
    assert_eq!(
        repo.task_context_commitments(&old),
        Err(RepositoryError::TaskInputStale)
    );
    let new = task(&mut repo, &project, "after", "generateOutline", json!({}));
    assert_eq!(
        repo.task_context_commitments(&new).unwrap()["entries"][0]["revisionId"],
        "promise-new"
    );
    let content = json!({"chapterNumber":1,"body":"作者已经修改了原文","sceneGroups":[{"body":"作者已经修改了原文"}]}).to_string();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("draft-chapter-1").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse("edited-source").unwrap(),
        commit_id: CommitId::parse("edited-source-commit").unwrap(),
        idempotency_key: IdempotencyKey::parse("edited-source-key").unwrap(),
        request_digest: content.clone(),
        content_digest: content.clone(),
        content_json: content,
        expected_project_revision: repo.project_revision(&project).unwrap(),
        expected_snapshot_id: repo
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        next_snapshot_id: SnapshotId::parse("edited-source-snapshot").unwrap(),
        expected_candidate_revision_id: Some(RevisionId::parse("revision-1").unwrap()),
        chapter_projection: None,
        task_output: None,
        linked_impact_ids: vec![],
    })
    .unwrap();
    assert_eq!(
        repo.task_context_commitments(&new),
        Err(RepositoryError::TaskInputStale)
    );
    let fresh = task(&mut repo, &project, "edited", "generateOutline", json!({}));
    assert_eq!(
        repo.task_context_commitments(&fresh).unwrap()["entries"],
        json!([])
    );
    assert_eq!(
        repo.narrative_commitment(&project, "new", /*snapshot*/ None)
            .unwrap()["payload"]["status"],
        "open"
    );
}

#[test]
fn future_fulfillment_does_not_leave_a_spoiler_summary_in_a_partial_projection() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 3);
    save_commitment(
        &mut repo, &project, "setup", /*chapter*/ 1, /*start*/ 0,
    );
    let mut content: Value = serde_json::from_str(
        &repo
            .revision_content(&RevisionId::parse("promise-setup").unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let source: Value = serde_json::from_str(
        &repo
            .revision_content(&RevisionId::parse("revision-3").unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    content["payload"]["id"] = json!("fulfilled");
    content["payload"]["summary"] = json!("未来的背叛者终于现身");
    content["payload"]["status"] = json!("fulfilled");
    content["payload"]["fulfillmentEvidenceRefs"] = json!([{
        "objectId":"draft-chapter-3","revisionId":"revision-3",
        "quote":source["body"].as_str().unwrap().chars().take(/*n*/ 4).collect::<String>(),"start":0,"end":4
    }]);
    save_value(&mut repo, &project, "fulfilled", content);
    let plan = task(
        &mut repo,
        &project,
        "partial",
        "generatePlan",
        json!({"planFromChapter":2}),
    );
    let result = repo.task_context_commitments(&plan).unwrap();
    assert_eq!(result["entries"].as_array().unwrap().len(), 1);
    assert_eq!(result["entries"][0]["id"], "setup");
    assert!(!result.to_string().contains("未来的背叛者"));
}

use novel_codex_story_core::{CommitmentContextRepository, NarrativeCommitmentsRepository, ProjectRevisionRepository, TasksRepository};
