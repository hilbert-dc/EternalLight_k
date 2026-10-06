use super::{
    AssociateChapterPlanRequest, ChapterBinding, RebindChapterPlanRequest, ReboundChapterPlan,
};
use crate::{BranchId, ProjectId, ProjectRecord, SnapshotId, SqliteStoryRepository};
use pretty_assertions::assert_eq;
use rusqlite::params;
use serde_json::json;

#[test]
fn association_requires_a_unique_matching_accepted_plan_entry_and_replays_once() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("association").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: project.clone(),
            title: "港城".into(),
            logline: String::new(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    let chapter = repo
        .materialize_standalone_chapter(super::MaterializeStandaloneChapterRequest {
            project_id: &project,
            title: "密函",
            volume_id: None,
            expected_revision: 0,
            idempotency_key: "chapter",
            request_digest: "chapter",
            now: 1,
        })
        .unwrap()
        .binding;
    for (revision, content) in [
        (
            "accepted-plan",
            json!({"payload":{"chapters":[
                {"chapterPlanId":"valid","chapterNumber":1},
                {"chapterPlanId":"another-valid","chapterNumber":1},
                {"chapterPlanId":"wrong-number","chapterNumber":2},
                {"chapterPlanId":"invalid-pov","chapterNumber":1,"povCharacterId":17},
                {"chapterPlanId":"invalid-time","chapterNumber":1,"storyTime":"tomorrow"},
                {"chapterPlanId":"duplicate","chapterNumber":1},
                {"chapterPlanId":"duplicate","chapterNumber":1}
            ]}}),
        ),
        (
            "candidate-plan",
            json!({"payload":{"chapters":[
                {"chapterPlanId":"candidate-only","chapterNumber":1},
                {"chapterPlanId":"wrong-number","chapterNumber":2},
                {"chapterPlanId":"invalid-pov","chapterNumber":1,"povCharacterId":17},
                {"chapterPlanId":"invalid-time","chapterNumber":1,"storyTime":"tomorrow"},
                {"chapterPlanId":"duplicate","chapterNumber":1},
                {"chapterPlanId":"duplicate","chapterNumber":1}
            ]}}),
        ),
    ] {
        repo.connection.execute("INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, 'artifact-plan', 'plan', 'fixture', ?3)",
            params![revision,project.as_str(),content.to_string()]).unwrap();
    }
    repo.connection.execute("INSERT INTO story_object_heads_v2(project_id, object_id, object_kind, candidate_revision_id, accepted_revision_id) VALUES (?1, 'artifact-plan', 'plan', 'candidate-plan', 'accepted-plan')",[project.as_str()]).unwrap();
    for plan_id in [
        "missing",
        "wrong-number",
        "duplicate",
        "candidate-only",
        "invalid-pov",
        "invalid-time",
    ] {
        assert!(
            repo.associate_chapter_plan(AssociateChapterPlanRequest {
                project_id: &project,
                chapter_id: &chapter.chapter_id,
                chapter_plan_id: plan_id,
                expected_revision: 1,
                idempotency_key: plan_id,
                request_digest: plan_id,
                now: 2,
            })
            .is_err()
        );
        assert_eq!(repo.project_revision(&project).unwrap(), 1);
        assert_eq!(
            repo.chapter_bindings(&project).unwrap(),
            Vec::<ChapterBinding>::new()
        );
    }
    let request = || AssociateChapterPlanRequest {
        project_id: &project,
        chapter_id: &chapter.chapter_id,
        chapter_plan_id: "valid",
        expected_revision: 1,
        idempotency_key: "associate",
        request_digest: "associate",
        now: 2,
    };
    let expected = ChapterBinding {
        plan_revision_id: "accepted-plan".into(),
        chapter_plan_id: "valid".into(),
        chapter_id: chapter.chapter_id.clone(),
        chapter_number: 1,
        result_revision: 2,
    };
    assert_eq!(repo.associate_chapter_plan(request()).unwrap(), expected);
    assert_eq!(repo.associate_chapter_plan(request()).unwrap(), expected);
    assert_eq!(
        repo.chapter_bindings(&project).unwrap(),
        vec![expected.clone()]
    );
    assert!(
        repo.associate_chapter_plan(AssociateChapterPlanRequest {
            project_id: &project,
            chapter_id: &chapter.chapter_id,
            chapter_plan_id: "another-valid",
            expected_revision: 2,
            idempotency_key: "replace",
            request_digest: "replace",
            now: 3,
        })
        .is_err()
    );
    assert_eq!(repo.project_revision(&project).unwrap(), 2);
    assert_eq!(
        repo.chapter_bindings(&project).unwrap(),
        vec![expected.clone()]
    );
    let count:i64 = repo.connection.query_row("SELECT COUNT(*) FROM story_domain_receipts_v2 WHERE project_id=?1 AND operation='associate-plan'",[project.as_str()],|row|row.get(0)).unwrap();
    assert_eq!(count, 1);

    let draft_object = format!("draft-{}", chapter.chapter_id);
    let prose = json!({"payload":{"body":"他藏好密函。"}}).to_string();
    repo.connection.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) VALUES ('draft-source',?1,?2,'draft','prose',?3)",params![project.as_str(),draft_object,prose]).unwrap();
    repo.connection.execute("INSERT INTO story_object_heads_v2(project_id,object_id,object_kind,candidate_revision_id,locked) VALUES (?1,?2,'draft','draft-source',0)",params![project.as_str(),draft_object]).unwrap();
    for plan_id in [
        "missing",
        "wrong-number",
        "duplicate",
        "invalid-pov",
        "invalid-time",
    ] {
        assert!(
            repo.rebind_chapter_plan(RebindChapterPlanRequest {
                project_id: &project,
                chapter_id: &chapter.chapter_id,
                previous_plan_revision_id: "accepted-plan",
                previous_chapter_plan_id: "valid",
                source_revision_id: "draft-source",
                plan_revision_id: "candidate-plan",
                chapter_plan_id: plan_id,
                expected_revision: 2,
                idempotency_key: plan_id,
                request_digest: plan_id,
                now: 4,
            })
            .is_err()
        );
        assert_eq!(
            repo.chapter_bindings(&project).unwrap(),
            vec![expected.clone()]
        );
    }
    let rebind = || RebindChapterPlanRequest {
        project_id: &project,
        chapter_id: &chapter.chapter_id,
        previous_plan_revision_id: "accepted-plan",
        previous_chapter_plan_id: "valid",
        source_revision_id: "draft-source",
        plan_revision_id: "candidate-plan",
        chapter_plan_id: "candidate-only",
        expected_revision: 2,
        idempotency_key: "rebind",
        request_digest: "rebind",
        now: 4,
    };
    repo.connection
        .execute(
            "UPDATE story_object_heads_v2 SET locked=1 WHERE object_id=?1",
            [&draft_object],
        )
        .unwrap();
    assert!(repo.rebind_chapter_plan(rebind()).is_err());
    assert_eq!(repo.chapter_bindings(&project).unwrap(), vec![expected]);
    repo.connection
        .execute(
            "UPDATE story_object_heads_v2 SET locked=0 WHERE object_id=?1",
            [&draft_object],
        )
        .unwrap();
    let rebound = ReboundChapterPlan {
        previous_plan_revision_id: "accepted-plan".into(),
        previous_chapter_plan_id: "valid".into(),
        source_revision_id: "draft-source".into(),
        binding: ChapterBinding {
            plan_revision_id: "candidate-plan".into(),
            chapter_plan_id: "candidate-only".into(),
            chapter_id: chapter.chapter_id.clone(),
            chapter_number: 1,
            result_revision: 3,
        },
    };
    assert_eq!(repo.rebind_chapter_plan(rebind()).unwrap(), rebound);
    assert_eq!(repo.rebind_chapter_plan(rebind()).unwrap(), rebound);
    assert_eq!(
        repo.chapter_bindings(&project).unwrap(),
        vec![rebound.binding.clone()]
    );
    let stored: String = repo
        .connection
        .query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE revision_id='draft-source'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, prose);
    let receipt: String = repo.connection.query_row("SELECT result_json FROM story_domain_receipts_v2 WHERE project_id=?1 AND operation='rebind-plan'",[project.as_str()],|row|row.get(0)).unwrap();
    assert_eq!(
        serde_json::from_str::<ReboundChapterPlan>(&receipt).unwrap(),
        rebound
    );
}
