use novel_codex_story_core::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn project() -> ProjectId {
    ProjectId::parse("commitment-project").unwrap()
}
fn branch() -> BranchId {
    BranchId::parse("main").unwrap()
}
fn setup() -> SqliteStoryRepository {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project(),
            title: "悬念".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        branch(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    save(
        &mut repo,
        "draft-chapter-one",
        "draft",
        json!({"chapterNumber":1,"body":"灯将熄灭。灯火归来。"}),
        "draft-one",
        /*prior*/ None,
    )
    .unwrap();
    repo
}
fn save(
    repo: &mut SqliteStoryRepository,
    object: &str,
    kind: &str,
    content: Value,
    id: &str,
    prior: Option<&str>,
) -> Result<CandidateReceipt, RepositoryError> {
    repo.save_candidate(SaveCandidateRequest {
        project_id: project(),
        branch_id: branch(),
        object_id: ObjectId::parse(object).unwrap(),
        object_kind: kind.into(),
        revision_id: RevisionId::parse(id).unwrap(),
        commit_id: CommitId::parse(format!("commit-{id}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(id).unwrap(),
        request_digest: content.to_string(),
        content_digest: content.to_string(),
        content_json: content.to_string(),
        expected_project_revision: repo.project_revision(&project()).unwrap(),
        expected_snapshot_id: repo
            .branch_head(&project(), &branch())
            .unwrap()
            .unwrap()
            .snapshot_id,
        next_snapshot_id: SnapshotId::parse(format!("snapshot-{id}")).unwrap(),
        expected_candidate_revision_id: prior.map(|id| RevisionId::parse(id).unwrap()),
        chapter_projection: None,
        task_output: None,
        linked_impact_ids: Vec::new(),
    })
}
fn anchor(quote: &str, start: usize, end: usize) -> Value {
    json!({"objectId":"draft-chapter-one","revisionId":"draft-one","quote":quote,"start":start,"end":end})
}
fn content() -> Value {
    json!({"payload":{"id":"lamp","kind":"suspense","summary":"灯能否再次亮起","relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,
        "setupEvidenceRefs":[anchor("灯将熄灭", /*start*/ 0, /*end*/ 4)],"fulfillmentCriteria":"正文明确灯火归来","targetWindow":{"kind":"chapterRange","fromChapterId":"chapter-one","throughChapterId":"chapter-one"},
        "status":"open","fulfillmentEvidenceRefs":[],"decisionReason":null}})
}
fn review(repo: &mut SqliteStoryRepository, revision: &str) -> ReviewRecord {
    repo.create_review(CreateReviewRequest {
        review_id: format!("review-{revision}"),
        project_id: project(),
        object_id: ObjectId::parse("commitment-lamp").unwrap(),
        object_kind: "commitment".into(),
        revision_id: RevisionId::parse(revision).unwrap(),
        idempotency_key: IdempotencyKey::parse(format!("review-{revision}")).unwrap(),
        request_digest: revision.into(),
        checks_json: "[]".into(),
        proposals_json: "[]".into(),
        modeling_batch_id: None,
        selections_json: "{}".into(),
    })
    .unwrap()
}
fn accept(
    repo: &mut SqliteStoryRepository,
    review: ReviewRecord,
) -> Result<AcceptanceReceipt, RepositoryError> {
    repo.accept_review(AcceptReviewRequest {
        project_id: project(),
        branch_id: branch(),
        review_id: review.review_id.clone(),
        commit_id: CommitId::parse(format!("accept-{}", review.review_id)).unwrap(),
        idempotency_key: IdempotencyKey::parse(format!("accept-{}", review.review_id)).unwrap(),
        request_digest: review.review_id.clone(),
        expected_project_revision: review.project_revision,
        next_snapshot_id: SnapshotId::parse(format!("accepted-{}", review.review_id)).unwrap(),
        selections_json: "{}".into(),
    })
}

#[test]
fn commitment_status_requires_resolvable_evidence_and_preserves_atomic_failure() {
    let mut repo = setup();
    let before = repo.branch_head(&project(), &branch()).unwrap();
    let mut cases = Vec::new();
    let mut invalid = content();
    invalid["payload"]["status"] = json!("fulfilled");
    cases.push(invalid);
    let mut invalid = content();
    invalid["payload"]["status"] = json!("partiallyFulfilled");
    cases.push(invalid);
    let mut invalid = content();
    invalid["payload"]["status"] = json!("abandoned");
    cases.push(invalid);
    let mut invalid = content();
    invalid["payload"]["setupEvidenceRefs"][0]["quote"] = json!("伪造引用");
    cases.push(invalid);
    let mut invalid = content();
    invalid["payload"]["setupEvidenceRefs"][0]["revisionId"] = json!("missing");
    cases.push(invalid);
    let mut invalid = content();
    invalid["payload"]["targetWindow"]["throughChapterId"] = json!("future-chapter");
    cases.push(invalid);
    let mut invalid = content();
    invalid["payload"]["setupEvidenceRefs"]
        .as_array_mut()
        .unwrap()
        .push(anchor("灯将熄灭", /*start*/ 0, /*end*/ 4));
    cases.push(invalid);
    for (index, value) in cases.into_iter().enumerate() {
        assert!(
            save(
                &mut repo,
                "commitment-lamp",
                "commitment",
                value,
                &format!("invalid-{index}"),
                /*prior*/ None
            )
            .is_err()
        );
        assert_eq!(repo.branch_head(&project(), &branch()).unwrap(), before);
    }
    let mut value = content();
    value["payload"]["status"] = json!("partiallyFulfilled");
    value["payload"]["fulfillmentEvidenceRefs"] =
        json!([anchor("灯火归来", /*start*/ 5, /*end*/ 9)]);
    save(
        &mut repo,
        "commitment-lamp",
        "commitment",
        value.clone(),
        "partial",
        /*prior*/ None,
    )
    .unwrap();
    value["payload"]["status"] = json!("fulfilled");
    save(
        &mut repo,
        "commitment-lamp",
        "commitment",
        value,
        "fulfilled",
        Some("partial"),
    )
    .unwrap();
    let result = repo
        .narrative_commitment(&project(), "lamp", /*snapshot*/ None)
        .unwrap();
    assert_eq!(result["payload"]["status"], "fulfilled");
    assert_eq!(result["literaryFulfillmentVerified"], false);
    assert_eq!(result["confirmationState"], "candidate");
    let mut abandoned = content();
    abandoned["payload"]["targetWindow"] = json!({"kind":"open"});
    abandoned["payload"]["status"] = json!("abandoned");
    abandoned["payload"]["decisionReason"] = json!("作者保留开放结局");
    abandoned["payload"]["fulfillmentEvidenceRefs"] =
        json!([anchor("灯火归来", /*start*/ 5, /*end*/ 9)]);
    save(
        &mut repo,
        "commitment-lamp",
        "commitment",
        abandoned,
        "abandoned",
        Some("fulfilled"),
    )
    .unwrap();
    assert_eq!(
        repo.narrative_commitment(&project(), "lamp", /*snapshot*/ None)
            .unwrap()["payload"]["status"],
        "abandoned"
    );
    assert_eq!(
        repo.narrative_commitment(&project(), "lamp", /*snapshot*/ None)
            .unwrap()["payload"]["fulfillmentEvidenceRefs"],
        json!([anchor("灯火归来", /*start*/ 5, /*end*/ 9)])
    );
}

#[test]
fn commitment_confirmation_and_undo_replay_at_their_own_snapshots() {
    let mut repo = setup();
    let saved = save(
        &mut repo,
        "commitment-lamp",
        "commitment",
        content(),
        "open",
        /*prior*/ None,
    )
    .unwrap();
    let review = review(&mut repo, "open");
    let receipt = accept(&mut repo, review).unwrap();
    let accepted = repo
        .branch_head(&project(), &branch())
        .unwrap()
        .unwrap()
        .snapshot_id;
    assert_eq!(
        repo.narrative_commitment(&project(), "lamp", /*snapshot*/ None)
            .unwrap()["confirmationState"],
        "confirmed"
    );
    repo.undo_acceptance(UndoAcceptanceRequest {
        project_id: project(),
        branch_id: branch(),
        target_commit_id: receipt.commit_id,
        commit_id: CommitId::parse("undo").unwrap(),
        idempotency_key: IdempotencyKey::parse("undo").unwrap(),
        request_digest: "undo".into(),
        expected_project_revision: receipt.result_revision,
        next_snapshot_id: SnapshotId::parse("undo-snapshot").unwrap(),
    })
    .unwrap();
    assert_eq!(
        repo.narrative_commitment(&project(), "lamp", /*snapshot*/ None)
            .unwrap()["confirmationState"],
        "candidate"
    );
    assert_eq!(
        repo.narrative_commitment(&project(), "lamp", Some(&accepted))
            .unwrap()["confirmationState"],
        "confirmed"
    );
    assert_eq!(
        repo.narrative_commitment(&project(), "lamp", Some(&saved.result_snapshot_id))
            .unwrap()["confirmationState"],
        "candidate"
    );
}

#[test]
fn source_edit_blocks_confirmation_but_does_not_rewrite_historical_commitments() {
    let mut repo = setup();
    let saved = save(
        &mut repo,
        "commitment-lamp",
        "commitment",
        content(),
        "open",
        /*prior*/ None,
    )
    .unwrap();
    let review = review(&mut repo, "open");
    let historical = repo
        .narrative_commitment(&project(), "lamp", Some(&saved.result_snapshot_id))
        .unwrap();
    save(
        &mut repo,
        "draft-chapter-one",
        "draft",
        json!({"chapterNumber":7,"body":"灯没有归来。"}),
        "draft-two",
        Some("draft-one"),
    )
    .unwrap();
    let before = repo.branch_head(&project(), &branch()).unwrap();
    assert_eq!(
        accept(&mut repo, review).unwrap_err(),
        RepositoryError::TaskInputStale
    );
    assert_eq!(repo.branch_head(&project(), &branch()).unwrap(), before);
    assert_eq!(
        repo.narrative_commitment(&project(), "lamp", Some(&saved.result_snapshot_id))
            .unwrap(),
        historical
    );
    let current = repo
        .narrative_commitment(&project(), "lamp", /*snapshot*/ None)
        .unwrap();
    assert_eq!(current["sourceCurrent"], false);
    assert_eq!(current["windowPosition"]["fromChapterNumber"], 7);
    assert_eq!(current["payload"]["status"], "open");
    assert!(
        save(
            &mut repo,
            "commitment-lamp",
            "commitment",
            content(),
            "stale-save",
            Some("open")
        )
        .is_err()
    );
}

#[test]
fn foreign_evidence_and_canon_fact_injection_cannot_enter_commitment_acceptance() {
    let mut repo = setup();
    let foreign = ProjectId::parse("foreign").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: foreign.clone(),
            title: "他作".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        branch(),
        SnapshotId::parse("foreign-initial").unwrap(),
    )
    .unwrap();
    repo.apply_commit(CommitRequest {
        project_id: foreign.clone(),
        branch_id: branch(),
        commit_id: CommitId::parse("foreign-commit").unwrap(),
        idempotency_key: IdempotencyKey::parse("foreign-commit").unwrap(),
        request_digest: "foreign".into(),
        base_snapshot_id: SnapshotId::parse("foreign-initial").unwrap(),
        next_snapshot_id: SnapshotId::parse("foreign-next").unwrap(),
        revisions: vec![RevisionRecord {
            project_id: foreign,
            object_id: ObjectId::parse("draft-foreign").unwrap(),
            object_kind: "draft".into(),
            revision_id: RevisionId::parse("foreign-draft").unwrap(),
            parent_revision_id: None,
            content_digest: "foreign".into(),
            content_json: json!({"body":"灯将熄灭"}).to_string(),
        }],
    })
    .unwrap();
    let before = repo.branch_head(&project(), &branch()).unwrap();
    let mut bad = content();
    bad["payload"]["setupEvidenceRefs"][0]["objectId"] = json!("draft-foreign");
    bad["payload"]["setupEvidenceRefs"][0]["revisionId"] = json!("foreign-draft");
    assert!(
        save(
            &mut repo,
            "commitment-lamp",
            "commitment",
            bad,
            "foreign-reference",
            /*prior*/ None
        )
        .is_err()
    );
    assert_eq!(repo.branch_head(&project(), &branch()).unwrap(), before);
    save(
        &mut repo,
        "commitment-lamp",
        "commitment",
        content(),
        "open",
        /*prior*/ None,
    )
    .unwrap();
    let review = repo
        .create_review(CreateReviewRequest {
            review_id: "forged-facts".into(),
            project_id: project(),
            object_id: ObjectId::parse("commitment-lamp").unwrap(),
            object_kind: "commitment".into(),
            revision_id: RevisionId::parse("open").unwrap(),
            idempotency_key: IdempotencyKey::parse("forged-facts").unwrap(),
            request_digest: "forged".into(),
            checks_json: "[]".into(),
            proposals_json: "[{\"predicate\":\"secret\",\"value\":true}]".into(),
            modeling_batch_id: None,
            selections_json: "{}".into(),
        })
        .unwrap();
    let before = repo.branch_head(&project(), &branch()).unwrap();
    assert!(accept(&mut repo, review).is_err());
    assert_eq!(repo.branch_head(&project(), &branch()).unwrap(), before);
    assert_eq!(repo.project_revision(&project()).unwrap(), 0);
}

#[test]
fn locked_confirmed_commitment_cannot_be_replaced_by_a_new_candidate() {
    let mut repo = setup();
    save(
        &mut repo,
        "commitment-lamp",
        "commitment",
        content(),
        "open",
        /*prior*/ None,
    )
    .unwrap();
    let review = review(&mut repo, "open");
    accept(&mut repo, review).unwrap();
    repo.set_object_lock(
        &project(),
        &ObjectId::parse("commitment-lamp").unwrap(),
        /*locked*/ true,
        /*expected_project_revision*/ 1,
    )
    .unwrap();
    let before = repo
        .narrative_commitment(&project(), "lamp", /*snapshot*/ None)
        .unwrap();
    let mut changed = content();
    changed["payload"]["summary"] = json!("擅自改变锁定承诺");
    assert_eq!(
        save(
            &mut repo,
            "commitment-lamp",
            "commitment",
            changed,
            "locked-replacement",
            Some("open")
        )
        .unwrap_err(),
        RepositoryError::ObjectLocked
    );
    assert_eq!(
        repo.narrative_commitment(&project(), "lamp", /*snapshot*/ None)
            .unwrap(),
        before
    );
}

#[test]
fn future_chapter_window_is_saved_and_its_historical_position_is_frozen() {
    let mut repo = setup();
    repo.materialize_standalone_chapter(
        &project(),
        "目录首章",
        /*expected_revision*/ 0,
        "first",
        "first",
        /*now*/ 1,
    )
    .unwrap();
    let future = repo
        .materialize_standalone_chapter(
            &project(),
            "尚未起草",
            /*expected_revision*/ 1,
            "future",
            "future",
            /*now*/ 2,
        )
        .unwrap();
    let mut value = content();
    value["payload"]["targetWindow"]["throughChapterId"] = json!(future.chapter_id);
    let receipt = save(
        &mut repo,
        "commitment-lamp",
        "commitment",
        value.clone(),
        "window",
        /*prior*/ None,
    )
    .unwrap();
    let historical = repo
        .narrative_commitment(&project(), "lamp", Some(&receipt.result_snapshot_id))
        .unwrap();
    assert_eq!(
        historical["windowPosition"],
        json!({"kind":"chapterRange","fromChapterNumber":1,"throughChapterNumber":2,"orderKnown":true})
    );
    save(
        &mut repo,
        &format!("draft-{}", future.chapter_id),
        "draft",
        json!({"chapterNumber":3,"body":"未来章节终于开始。"}),
        "future-draft",
        /*prior*/ None,
    )
    .unwrap();
    assert_eq!(
        repo.narrative_commitment(&project(), "lamp", Some(&receipt.result_snapshot_id))
            .unwrap(),
        historical
    );
    assert_eq!(
        repo.narrative_commitment(&project(), "lamp", /*snapshot*/ None)
            .unwrap()["windowPosition"]["throughChapterNumber"],
        3
    );
    save(
        &mut repo,
        "draft-zero",
        "draft",
        json!({"chapterNumber":0,"body":"未编入有效章次的暂存稿。"}),
        "zero-draft",
        /*prior*/ None,
    )
    .unwrap();
    let before = repo.branch_head(&project(), &branch()).unwrap();
    let mut nonpositive = value.clone();
    nonpositive["payload"]["targetWindow"]["fromChapterId"] = json!("zero");
    assert!(
        save(
            &mut repo,
            "commitment-lamp",
            "commitment",
            nonpositive,
            "zero-window",
            Some("window")
        )
        .is_err()
    );
    value["payload"]["targetWindow"]["fromChapterId"] = json!(future.chapter_id);
    value["payload"]["targetWindow"]["throughChapterId"] = json!("chapter-one");
    assert!(
        save(
            &mut repo,
            "commitment-lamp",
            "commitment",
            value.clone(),
            "reversed",
            Some("window")
        )
        .is_err()
    );
    value["payload"]["targetWindow"]["throughChapterId"] = json!("absent-chapter");
    assert!(
        save(
            &mut repo,
            "commitment-lamp",
            "commitment",
            value,
            "absent",
            Some("window")
        )
        .is_err()
    );
    assert_eq!(repo.branch_head(&project(), &branch()).unwrap(), before);
}

#[test]
fn catalog_pagination_keeps_one_snapshot_without_new_objects_or_revisions() {
    let mut repo = setup();
    save(
        &mut repo,
        "commitment-lamp",
        "commitment",
        content(),
        "lamp-candidate",
        /*prior*/ None,
    )
    .unwrap();
    let mut second = content();
    second["payload"]["id"] = json!("second");
    second["payload"]["summary"] = json!("原来的第二个承诺");
    save(
        &mut repo,
        "commitment-second",
        "commitment",
        second.clone(),
        "second-one",
        /*prior*/ None,
    )
    .unwrap();
    let first = repo
        .narrative_commitments_page(&project(), /*cursor*/ None, /*limit*/ 1)
        .unwrap();
    let cursor = first["nextCursor"].as_str().unwrap();
    second["payload"]["summary"] = json!("后来修改的第二个承诺");
    save(
        &mut repo,
        "commitment-second",
        "commitment",
        second,
        "second-two",
        Some("second-one"),
    )
    .unwrap();
    let mut third = content();
    third["payload"]["id"] = json!("third");
    save(
        &mut repo,
        "commitment-third",
        "commitment",
        third,
        "third-one",
        /*prior*/ None,
    )
    .unwrap();
    let last = repo
        .narrative_commitments_page(&project(), Some(cursor), /*limit*/ 1)
        .unwrap();
    assert_eq!(
        last,
        json!({"snapshotId":first["snapshotId"],"nextCursor":null,"data":[{"id":"second","summary":"原来的第二个承诺","kind":"suspense","status":"open","revisionId":"second-one","confirmationState":"candidate","sourceCurrent":true}]})
    );
    assert_eq!(
        repo.narrative_commitments_page(&project(), /*cursor*/ None, /*limit*/ 20)
            .unwrap()["data"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    for limit in [0, 21] {
        assert!(
            repo.narrative_commitments_page(&project(), /*cursor*/ None, limit)
                .is_err()
        );
    }
    assert!(
        repo.narrative_commitments_page(&project(), Some(cursor), /*limit*/ 2)
            .is_err()
    );
    assert!(
        repo.narrative_commitments_page(
            &ProjectId::parse("foreign").unwrap(),
            Some(cursor),
            /*limit*/ 1
        )
        .is_err()
    );
    let mut forged: Value = serde_json::from_str(cursor).unwrap();
    forged["snapshotId"] = json!("absent-snapshot");
    assert!(
        repo.narrative_commitments_page(&project(), Some(&forged.to_string()), /*limit*/ 1)
            .is_err()
    );
}
