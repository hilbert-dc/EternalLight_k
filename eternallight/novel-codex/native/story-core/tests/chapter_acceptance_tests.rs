use novel_codex_story_core::*;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[path = "support/chapter_acceptance.rs"]
mod fixture;
use fixture::*;

#[test]
fn subset_adopts_candidate_a_and_preserves_formal_b_metadata_candidate_and_undo() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let base = chapter(
        "正式标题",
        vec![group("a", "正式 A"), group("b", "正式 B")],
        json!({"planRevisionId":"formal-plan"}),
    );
    let candidate = chapter(
        "候选标题",
        vec![group("a", "候选 A"), group("b", "候选 B")],
        json!({"planRevisionId":"candidate-plan"}),
    );
    setup(&mut repo, base.clone(), candidate.clone());
    let request = accept(&repo, "candidate", json!({"sceneGroupIds":["a"]}));
    let receipt = repo.accept_review(request.clone()).unwrap();
    let mut expected = base.clone();
    expected["sceneGroups"][0] = candidate["sceneGroups"][0].clone();
    expected["body"] = json!("候选 A\n\n---\n\n正式 B");
    expected["version"] = json!(2);
    expected["baseProjectRevision"] = json!(1);
    expected["source"] = json!("author");
    assert_eq!(accepted(&repo), expected);
    assert_eq!(
        repo.revision_content(&RevisionId::parse("candidate").unwrap())
            .unwrap(),
        Some(candidate.to_string())
    );
    assert_eq!(repo.accept_review(request).unwrap(), receipt);
    let acceptance = repo.acceptances(&project()).unwrap().remove(0);
    assert_ne!(acceptance.accepted_revision_id.as_str(), "candidate");
    assert_eq!(
        acceptance.previous_accepted_revision_id,
        Some(RevisionId::parse("base").unwrap())
    );
    repo.undo_acceptance(UndoAcceptanceRequest {
        project_id: project(),
        branch_id: branch(),
        target_commit_id: receipt.commit_id,
        commit_id: CommitId::parse("undo").unwrap(),
        idempotency_key: IdempotencyKey::parse("undo").unwrap(),
        request_digest: "undo".into(),
        expected_project_revision: 2,
        next_snapshot_id: SnapshotId::parse("undone").unwrap(),
    })
    .unwrap();
    assert_eq!(accepted(&repo), base);
}

#[test]
fn stable_scene_identity_merges_insertions_selected_moves_and_keeps_unselected_deletions() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let base = chapter(
        "正式",
        vec![
            group("a", "旧 A"),
            group("b", "旧 B"),
            group("c", "旧 C"),
            group("d", "旧 D"),
        ],
        Value::Null,
    );
    let mut replacement = group("a-new-group", "新 A");
    replacement["sceneId"] = json!("scene-a");
    let candidate = chapter(
        "候选",
        vec![
            group("c", "新 C"),
            replacement.clone(),
            group("x", "新增 X"),
            group("b", "新 B"),
        ],
        json!({"localOnly":true}),
    );
    setup(&mut repo, base.clone(), candidate.clone());
    repo.accept_review(accept(
        &repo,
        "candidate",
        json!({"sceneGroupIds":["a-new-group","x"]}),
    ))
    .unwrap();
    assert_eq!(
        accepted(&repo)["sceneGroups"],
        json!([
            group("b", "旧 B"),
            group("c", "旧 C"),
            replacement,
            group("x", "新增 X"),
            group("d", "旧 D")
        ])
    );
    assert_eq!(accepted(&repo)["title"], base["title"]);
    assert_eq!(accepted(&repo)["sourcePlanRefs"], base["sourcePlanRefs"]);
}

#[test]
fn all_candidate_groups_adopts_the_exact_full_chapter_including_deletion_and_metadata() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let base = chapter(
        "正式",
        vec![group("a", "旧 A"), group("b", "旧 B"), group("c", "旧 C")],
        Value::Null,
    );
    let candidate = chapter(
        "新标题",
        vec![group("c", "新 C"), group("a", "新 A")],
        json!({"planRevisionId":"new-plan"}),
    );
    setup(&mut repo, base, candidate.clone());
    repo.accept_review(accept(
        &repo,
        "candidate",
        json!({"sceneGroupIds":["c","a"]}),
    ))
    .unwrap();
    assert_eq!(accepted(&repo), candidate);
    assert_eq!(
        repo.acceptances(&project()).unwrap()[0]
            .accepted_revision_id
            .as_str(),
        "candidate"
    );
}

#[test]
fn invalid_empty_unknown_duplicate_or_locked_selections_are_atomic() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let mut locked = group("a", "受保护 A");
    locked["locked"] = json!(true);
    setup(
        &mut repo,
        chapter("正式", vec![locked, group("b", "旧 B")], Value::Null),
        chapter(
            "候选",
            vec![group("a", "新 A"), group("b", "新 B")],
            Value::Null,
        ),
    );
    let before = (
        repo.object_heads(&project()).unwrap(),
        repo.project_revision(&project()).unwrap(),
        repo.branch_head(&project(), &branch()).unwrap(),
        repo.acceptances(&project()).unwrap(),
    );
    for choices in [
        json!({}),
        json!({"sceneGroupIds":[]}),
        json!({"sceneGroupIds":["foreign"]}),
        json!({"sceneGroupIds":["b","b"]}),
        json!({"sceneGroupIds":["a"]}),
        json!({"sceneGroupIds":["a","b"]}),
        json!({"sceneGroupIds":["b"],"unexpected":true}),
    ] {
        assert!(
            repo.accept_review(accept(&repo, "candidate", choices))
                .is_err()
        );
        assert_eq!(
            (
                repo.object_heads(&project()).unwrap(),
                repo.project_revision(&project()).unwrap(),
                repo.branch_head(&project(), &branch()).unwrap(),
                repo.acceptances(&project()).unwrap()
            ),
            before
        );
    }
}

#[test]
fn a_selected_deletion_removes_only_that_formal_scene() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    setup(
        &mut repo,
        chapter(
            "正式",
            vec![group("a", "A"), group("b", "B"), group("c", "C")],
            Value::Null,
        ),
        chapter(
            "候选",
            vec![group("a", "新 A"), group("c", "新 C")],
            Value::Null,
        ),
    );
    repo.accept_review(accept(&repo, "candidate", json!({"sceneGroupIds":["b"]})))
        .unwrap();
    assert_eq!(
        accepted(&repo)["sceneGroups"],
        json!([group("a", "A"), group("c", "C")])
    );
}

#[test]
fn empty_candidate_does_not_turn_a_single_selected_deletion_into_whole_chapter_adoption() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    setup(
        &mut repo,
        chapter(
            "正式",
            vec![group("a", "A"), group("b", "B")],
            json!({"localOnly":true}),
        ),
        chapter("候选", vec![], Value::Null),
    );
    repo.accept_review(accept(&repo, "candidate", json!({"sceneGroupIds":["a"]})))
        .unwrap();
    assert_eq!(accepted(&repo)["sceneGroups"], json!([group("b", "B")]));
    assert_eq!(accepted(&repo)["title"], "正式");
}

#[test]
fn first_partial_adoption_has_explicit_local_lineage_not_candidate_plan_claims() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project(),
            title: "首次部分采用".into(),
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
        "candidate",
        chapter(
            "章节",
            vec![group("a", "A"), group("b", "B")],
            json!({"planRevisionId":"candidate-plan"}),
        ),
    );
    review(&mut repo, "candidate");
    repo.accept_review(accept(&repo, "candidate", json!({"sceneGroupIds":["a"]})))
        .unwrap();
    assert_eq!(
        accepted(&repo)["sourcePlanRefs"],
        json!({"lineageStatus":"current","localOnly":true})
    );
    assert_eq!(accepted(&repo)["completeness"], "partial");
}

#[test]
fn partial_adoption_can_review_and_adopt_remaining_candidate_again_and_undo_without_losing_b() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let candidate = chapter(
        "候选",
        vec![group("a", "新 A"), group("b", "新 B")],
        Value::Null,
    );
    setup(
        &mut repo,
        chapter("正式", vec![group("a", "A"), group("b", "B")], Value::Null),
        candidate.clone(),
    );
    repo.accept_review(accept(&repo, "candidate", json!({"sceneGroupIds":["a"]})))
        .unwrap();
    let first = accepted(&repo);
    let fresh = repo
        .create_review(CreateReviewRequest {
            review_id: "review-remaining".into(),
            project_id: project(),
            object_id: ObjectId::parse("draft-chapter-one").unwrap(),
            object_kind: "draft".into(),
            revision_id: RevisionId::parse("candidate").unwrap(),
            idempotency_key: IdempotencyKey::parse("review-remaining").unwrap(),
            request_digest: "remaining".into(),
            checks_json: "[]".into(),
            proposals_json: "[]".into(),
            modeling_batch_id: None,
            selections_json: "{}".into(),
        })
        .unwrap();
    assert_eq!((fresh.status, fresh.project_revision), ("ready".into(), 2));
    let mut next = accept(&repo, "remaining", json!({"sceneGroupIds":["b"]}));
    next.review_id = "review-remaining".into();
    let receipt = repo.accept_review(next).unwrap();
    assert_eq!(
        accepted(&repo)["version"].as_u64(),
        first["version"].as_u64().map(|version| version + 1)
    );
    assert_eq!(accepted(&repo)["body"], "新 A\n\n---\n\n新 B");
    assert_eq!(accepted(&repo)["title"], "正式");
    repo.undo_acceptance(UndoAcceptanceRequest {
        project_id: project(),
        branch_id: branch(),
        target_commit_id: receipt.commit_id,
        commit_id: CommitId::parse("undo-remaining").unwrap(),
        idempotency_key: IdempotencyKey::parse("undo-remaining").unwrap(),
        request_digest: "undo-remaining".into(),
        expected_project_revision: 3,
        next_snapshot_id: SnapshotId::parse("undone-remaining").unwrap(),
    })
    .unwrap();
    assert_eq!(accepted(&repo), first);
    let head = repo.object_heads(&project()).unwrap().remove(0);
    assert_eq!(
        head.candidate_revision_id,
        Some(RevisionId::parse("candidate").unwrap())
    );
    assert_eq!(
        repo.revision_content(&RevisionId::parse("candidate").unwrap())
            .unwrap(),
        Some(candidate.to_string())
    );
}

use novel_codex_story_core::{ProjectRevisionRepository, WorkflowRepository};
