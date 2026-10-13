use novel_codex_story_core::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[path = "../examples/support/long_work_fixture.rs"]
mod fixture;

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
        capability_hash: "a".repeat(/*n*/ 64),
        capability_expires_at: i64::MAX,
    })
    .unwrap()
}

fn save(
    repo: &mut SqliteStoryRepository,
    project: &ProjectId,
    id: &str,
    object: &str,
    kind: &str,
    content: Value,
    binding: Option<&TaskRecord>,
) {
    let content = content.to_string();
    let prior = repo
        .object_heads(project)
        .unwrap()
        .into_iter()
        .find(|head| head.object_id.as_str() == object)
        .and_then(|head| head.candidate_revision_id);
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse(object).unwrap(),
        object_kind: kind.into(),
        revision_id: RevisionId::parse(id).unwrap(),
        commit_id: CommitId::parse(format!("commit-{id}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(id).unwrap(),
        request_digest: content.clone(),
        content_digest: content.clone(),
        content_json: content,
        expected_project_revision: repo.project_revision(project).unwrap(),
        expected_snapshot_id: repo
            .branch_head(project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{id}")).unwrap(),
        expected_candidate_revision_id: prior,
        chapter_projection: None,
        task_output: binding.map(|task| TaskOutputBinding {
            task_id: task.task_id.clone(),
            thread_id: task.thread_id.clone(),
            output_kind: "commitmentExtraction".into(),
            completeness: None,
        }),
        linked_impact_ids: vec![],
    })
    .unwrap();
}

fn report(
    repo: &mut SqliteStoryRepository,
    project: &ProjectId,
    chapter: usize,
    count: usize,
    label: &str,
) {
    let source: Value = serde_json::from_str(
        &repo
            .revision_content(&RevisionId::parse(format!("revision-{chapter}")).unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let quote = source["body"]
        .as_str()
        .unwrap()
        .chars()
        .take(/*n*/ 4)
        .collect::<String>();
    let candidates = (0..count).map(|index|json!({
        "id":format!("proposal-{index}"),"kind":"foreshadowing","summary":label,"relatedEntityIds":[],
        "plotThreadId":null,"characterArcId":null,"setupEvidenceRefs":[{"objectId":format!("draft-chapter-{chapter}"),"revisionId":format!("revision-{chapter}"),"quote":quote,"start":0,"end":4}],
        "fulfillmentCriteria":"后续正文提供证据","targetWindow":{"kind":"open"},"status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null
    })).collect::<Vec<_>>();
    let bound = task(
        repo,
        project,
        &format!("extract-{chapter}"),
        "extractCommitments",
        json!({"chapterId":format!("chapter-{chapter}"),"chapterNumber":chapter,"sourceRevisionId":format!("revision-{chapter}")}),
    );
    save(
        repo,
        project,
        &format!("report-{chapter}"),
        &format!("commitment-extraction-chapter-{chapter}"),
        "commitmentExtraction",
        json!({"payload":{"chapterId":format!("chapter-{chapter}"),"sourceRevisionId":format!("revision-{chapter}"),"examinedRanges":[{"start":0,"end":4}],"candidates":candidates}}),
        Some(&bound),
    );
    repo.complete_task(
        project,
        &bound.task_id,
        &bound.thread_id,
        String::new(), None,
        /*now*/ 10,
    )
    .unwrap();
}

#[test]
fn planning_filters_future_before_limits_and_keeps_batch_identity_without_write_authority() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 4);
    report(
        &mut repo,
        &project,
        /*chapter*/ 4,
        /*count*/ 16,
        "未来秘密",
    );
    report(
        &mut repo,
        &project,
        /*chapter*/ 1,
        /*count*/ 1,
        "早期线索",
    );
    report(
        &mut repo,
        &project,
        /*chapter*/ 2,
        /*count*/ 1,
        "近期线索",
    );
    let bound = task(
        &mut repo,
        &project,
        "plan",
        "generatePlan",
        json!({"planFromChapter":3}),
    );
    let projection = repo.task_context_commitments(&bound).unwrap();
    assert_eq!(projection["entries"], json!([]));
    let proposals = &projection["proposalContext"]["entries"];
    assert_eq!(proposals.as_array().unwrap().len(), 2);
    assert_eq!(proposals[0]["reportRevisionId"], "report-2");
    assert_eq!(proposals[1]["reportRevisionId"], "report-1");
    assert_eq!(proposals[0]["proposalId"], proposals[1]["proposalId"]);
    assert_eq!(proposals[0]["authority"], "candidateProposal");
    assert_eq!(proposals[0]["established"], false);
    assert_eq!(projection["proposalContext"]["coverage"]["complete"], true);
    assert!(!projection.to_string().contains("未来秘密"));
    let writer = task(
        &mut repo,
        &project,
        "writer",
        "generateChapter",
        json!({"chapterId":"chapter-3","chapterNumber":3,"readingChapter":3,"sceneOrder":1}),
    );
    assert_eq!(
        repo.task_context_commitments(&writer).unwrap()["proposalContext"],
        json!({"entries":[],"coverage":{"requested":false,"complete":true,"includedEntries":0}})
    );
    let extract = task(
        &mut repo,
        &project,
        "extract-hidden",
        "extractCommitments",
        json!({"chapterId":"chapter-2","chapterNumber":2,"sourceRevisionId":"revision-2"}),
    );
    assert_eq!(
        repo.task_context_commitments(&extract).unwrap()["proposalContext"]["entries"],
        json!([])
    );
}

#[test]
fn author_establishment_removes_origin_proposal_and_source_edit_revokes_old_context() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 2);
    report(
        &mut repo, &project, /*chapter*/ 1, /*count*/ 2, "伏笔",
    );
    let before = task(&mut repo, &project, "before", "generateOutline", json!({}));
    let snapshot = repo
        .branch_head(&project, &BranchId::parse("main").unwrap())
        .unwrap()
        .unwrap()
        .snapshot_id;
    repo.establish_commitment(EstablishCommitment {
        project_id: project.clone(),
        origin: CommitmentExtractionOrigin {
            chapter_id: "chapter-1".into(),
            report_revision_id: "report-1".into(),
            proposal_id: "proposal-0".into(),
        },
        expected_project_revision: repo.project_revision(&project).unwrap(),
        base_snapshot_id: snapshot,
        idempotency_key: IdempotencyKey::parse("establish").unwrap(),
    })
    .unwrap();
    assert_eq!(
        repo.task_context_commitments(&before),
        Err(RepositoryError::TaskInputStale)
    );
    let after = task(&mut repo, &project, "after", "generateOutline", json!({}));
    let projected = repo.task_context_commitments(&after).unwrap();
    assert_eq!(projected["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        projected["proposalContext"]["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        projected["proposalContext"]["entries"][0]["proposalId"],
        "proposal-1"
    );
    save(
        &mut repo,
        &project,
        "source-edit",
        "draft-chapter-1",
        "draft",
        json!({"chapterNumber":1,"body":"新的正文","sceneGroups":[{"body":"新的正文"}]}),
        /*binding*/ None,
    );
    assert_eq!(
        repo.task_context_commitments(&after),
        Err(RepositoryError::TaskInputStale)
    );
    let fresh = task(&mut repo, &project, "fresh", "generateOutline", json!({}));
    assert_eq!(
        repo.task_context_commitments(&fresh).unwrap()["proposalContext"]["entries"],
        json!([])
    );
}

#[test]
fn oversized_proposal_is_omitted_whole_without_starving_a_small_later_candidate() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 2);
    report(
        &mut repo,
        &project,
        /*chapter*/ 2,
        /*count*/ 1,
        &"😀".repeat(/*n*/ 500),
    );
    report(
        &mut repo,
        &project,
        /*chapter*/ 1,
        /*count*/ 1,
        "小提案",
    );
    let bound = task(&mut repo, &project, "outline", "generateOutline", json!({}));
    let projected = repo.task_context_commitments(&bound).unwrap();
    let proposals = &projected["proposalContext"];
    assert_eq!(proposals["coverage"]["oversizedEntriesOmitted"], true);
    assert_eq!(proposals["coverage"]["complete"], false);
    assert_eq!(proposals["entries"].as_array().unwrap().len(), 1);
    assert_eq!(proposals["entries"][0]["summary"], "小提案");
    assert!(proposals["entries"].to_string().len() <= 8_000);
}

#[test]
fn selected_proposal_limit_reports_missing_scope_without_merging_local_ids() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 2);
    report(
        &mut repo, &project, /*chapter*/ 1, /*count*/ 16, "线索",
    );
    let bound = task(
        &mut repo,
        &project,
        "bounded",
        "generatePlan",
        json!({"planFromChapter":2}),
    );
    let projected = repo.task_context_commitments(&bound).unwrap();
    assert_eq!(
        projected["proposalContext"]["entries"]
            .as_array()
            .unwrap()
            .len(),
        12
    );
    assert_eq!(projected["proposalContext"]["coverage"]["complete"], false);
    assert_eq!(projected["proposalContext"]["coverage"]["truncated"], true);
    assert!(projected["proposalContext"]["entries"].to_string().len() <= 8_000);
}

fn plan_write(
    repo: &mut SqliteStoryRepository,
    project: &ProjectId,
    task: &TaskRecord,
    id: &str,
    references: Value,
) -> Result<CandidateReceipt, RepositoryError> {
    let content=json!({"payload":{"chapters":[{"chapterPlanId":"next-plan","chapterNumber":2,"scenes":[{
        "id":"next-scene","expectedEffects":[{"id":"next-effect","dimension":"narrative","description":"推进前文悬念","commitmentRefs":references}]
    }]}]}}).to_string();
    repo.save_candidate(SaveCandidateRequest {
        project_id: project.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("artifact-plan").unwrap(),
        object_kind: "plan".into(),
        revision_id: RevisionId::parse(id).unwrap(),
        commit_id: CommitId::parse(format!("commit-{id}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(id).unwrap(),
        request_digest: content.clone(),
        content_digest: content.clone(),
        content_json: content,
        expected_project_revision: repo.project_revision(project).unwrap(),
        expected_snapshot_id: repo
            .branch_head(project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{id}")).unwrap(),
        expected_candidate_revision_id: None,
        chapter_projection: None,
        task_output: Some(TaskOutputBinding {
            task_id: task.task_id.clone(),
            thread_id: task.thread_id.clone(),
            output_kind: "plan".into(),
            completeness: None,
        }),
        linked_impact_ids: vec![],
    })
}

#[test]
fn generated_plan_binds_exact_current_commitments_and_proposals_and_rolls_back_invalid_links() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let (project, _) = fixture::build(&mut repo, /*chapter_count*/ 2);
    report(
        &mut repo, &project, /*chapter*/ 1, /*count*/ 2, "线索",
    );
    report(
        &mut repo, &project, /*chapter*/ 2, /*count*/ 1, "未来",
    );
    let snapshot = repo
        .branch_head(&project, &BranchId::parse("main").unwrap())
        .unwrap()
        .unwrap()
        .snapshot_id;
    let established = repo
        .establish_commitment(EstablishCommitment {
            project_id: project.clone(),
            origin: CommitmentExtractionOrigin {
                chapter_id: "chapter-1".into(),
                report_revision_id: "report-1".into(),
                proposal_id: "proposal-0".into(),
            },
            expected_project_revision: repo.project_revision(&project).unwrap(),
            base_snapshot_id: snapshot,
            idempotency_key: IdempotencyKey::parse("link-establish").unwrap(),
        })
        .unwrap();
    let bound = task(
        &mut repo,
        &project,
        "link-plan",
        "generatePlan",
        json!({"planFromChapter":2}),
    );
    let head = repo
        .branch_head(&project, &BranchId::parse("main").unwrap())
        .unwrap()
        .unwrap();
    let valid_proposal = json!({"type":"proposal","reportRevisionId":"report-1","proposalId":"proposal-1","sourceRevisionId":"revision-1"});
    let valid_commitment = json!({"type":"commitment","commitmentId":established.commitment_id,"revisionId":established.receipt.revision_id.as_str()});
    let unrestricted = task(
        &mut repo,
        &project,
        "initial-link-plan",
        "generatePlan",
        json!({}),
    );
    assert_eq!(
        plan_write(
            &mut repo,
            &project,
            &unrestricted,
            "initial-future-link",
            json!([{"type":"proposal","reportRevisionId":"report-2","proposalId":"proposal-0","sourceRevisionId":"revision-2"}])
        ),
        Err(RepositoryError::TaskOutputForbidden)
    );
    assert_eq!(
        repo.branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap(),
        head
    );
    let invalids = [
        json!({"type":"proposal","reportRevisionId":"report-2","proposalId":"proposal-0","sourceRevisionId":"revision-2"}),
        json!({"type":"proposal","reportRevisionId":"report-1","proposalId":"proposal-0","sourceRevisionId":"revision-1"}),
        json!({"type":"proposal","reportRevisionId":"report-1","proposalId":"proposal-1","sourceRevisionId":"revision-2"}),
        json!({"type":"proposal","reportRevisionId":"report-1","proposalId":"unknown","sourceRevisionId":"revision-1"}),
        json!({"type":"commitment","commitmentId":established.commitment_id,"revisionId":"revision-1"}),
        json!({"type":"commitment","commitmentId":"foreign","revisionId":established.receipt.revision_id.as_str()}),
    ];
    for (index, reference) in invalids.into_iter().enumerate() {
        assert_eq!(
            plan_write(
                &mut repo,
                &project,
                &bound,
                &format!("invalid-{index}"),
                json!([reference])
            ),
            Err(RepositoryError::TaskOutputForbidden)
        );
        assert_eq!(
            repo.branch_head(&project, &BranchId::parse("main").unwrap())
                .unwrap()
                .unwrap(),
            head
        );
        assert_eq!(
            repo.task(&project, &bound.task_id)
                .unwrap()
                .unwrap()
                .output_refs_json,
            "[]"
        );
    }
    let references = json!([valid_proposal, valid_commitment]);
    let receipt = plan_write(
        &mut repo,
        &project,
        &bound,
        "linked-plan",
        references.clone(),
    )
    .unwrap();
    let saved: Value = serde_json::from_str(
        &repo
            .revision_content(&receipt.revision_id)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        saved["payload"]["chapters"][0]["scenes"][0]["expectedEffects"][0]["commitmentRefs"],
        references
    );
    assert_eq!(
        repo.complete_task(
            &project,
            &bound.task_id,
            &bound.thread_id,
            String::new(), None,
            /*now*/ 15
        )
        .unwrap()
        .status,
        "completed"
    );
}

use novel_codex_story_core::{CommitmentContextRepository, CommitmentEstablishmentRepository, ProjectRevisionRepository, TasksRepository};
