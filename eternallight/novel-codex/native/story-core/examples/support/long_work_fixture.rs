use novel_codex_story_core::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const CLAIM_PAIRS: usize = 49;

pub fn build(
    repository: &mut SqliteStoryRepository,
    chapter_count: usize,
) -> (ProjectId, SnapshotId) {
    let project = ProjectId::parse("long-work-benchmark").unwrap();
    let branch = BranchId::parse("main").unwrap();
    let mut head = SnapshotId::parse("snapshot-initial").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "长篇合成作品".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            branch.clone(),
            head.clone(),
        )
        .unwrap();
    for chapter in 1..=chapter_count {
        let object = ObjectId::parse(format!("draft-chapter-{chapter}")).unwrap();
        let revision = RevisionId::parse(format!("revision-{chapter}")).unwrap();
        let (body, groups, entities, assertions) = chapter_fixture(chapter);
        let content = json!({"chapterNumber":chapter,"title":format!("第{chapter}章"),
            "body":body,"sceneGroups":groups})
        .to_string();
        let next = SnapshotId::parse(format!("snapshot-{chapter}")).unwrap();
        repository
            .save_candidate(SaveCandidateRequest {
                project_id: project.clone(),
                branch_id: branch.clone(),
                object_id: object.clone(),
                object_kind: "draft".into(),
                revision_id: revision.clone(),
                commit_id: CommitId::parse(format!("commit-{chapter}")).unwrap(),
                idempotency_key: IdempotencyKey::parse(format!("save-{chapter}")).unwrap(),
                request_digest: digest(&content),
                content_digest: digest(&content),
                content_json: content,
                expected_project_revision: 0,
                expected_snapshot_id: head,
                next_snapshot_id: next.clone(),
                expected_candidate_revision_id: None,
                chapter_projection: Some(ChapterProjection {
                    chapter_id: format!("chapter-{chapter}"),
                    number: chapter as i64,
                    title: format!("第{chapter}章"),
                }),
                task_output: None,
                linked_impact_ids: Vec::new(),
            })
            .unwrap();
        head = next;
        let batch = repository
            .begin_modeling_batch(BeginModelingBatch {
                project_id: &project,
                object_id: &object,
                source_revision_id: &revision,
                expected_revision: 0,
                extractor_version: "long-work-v1",
                now: 2,
            })
            .unwrap();
        let task_id = format!("task-{chapter}");
        let thread_id = format!("thread-{chapter}");
        repository
            .create_task(CreateTaskRequest {
                task_id: task_id.clone(),
                project_id: project.clone(),
                thread_id: thread_id.clone(),
                operation: "modelKnowledge".into(),
                input_revision: 0,
                model: None,
                target_json: json!({"batchId":batch.id}).to_string(),
                input_manifest_json: "{}".into(),
                capability_hash: "a".repeat(64),
                capability_expires_at: i64::MAX,
            })
            .unwrap();
        repository
            .publish_modeling_batch(PublishModelingBatch {
                project_id: &project,
                batch_id: &batch.id,
                task_id: &task_id,
                thread_id: &thread_id,
                entities: &entities,
                assertions: &assertions,
                now: 3,
            })
            .unwrap();
        repository
            .complete_task(
                &project,
                &task_id,
                &thread_id,
                "fixture published".into(),
                None,
                /*now*/ 4,
            )
            .unwrap();
        if chapter % 64 == 0 {
            eprintln!("Built {chapter}/{chapter_count} chapters");
        }
    }
    // A final creation snapshot captures the last published batch as well.
    let content = json!({"title":"合成基准捕获"}).to_string();
    let snapshot = SnapshotId::parse("snapshot-captured").unwrap();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project.clone(),
            branch_id: branch,
            object_id: ObjectId::parse("benchmark-brief").unwrap(),
            object_kind: "brief".into(),
            revision_id: RevisionId::parse("benchmark-brief-revision").unwrap(),
            commit_id: CommitId::parse("benchmark-capture").unwrap(),
            idempotency_key: IdempotencyKey::parse("benchmark-capture").unwrap(),
            request_digest: digest(&content),
            content_digest: digest(&content),
            content_json: content,
            expected_project_revision: 0,
            expected_snapshot_id: head,
            next_snapshot_id: snapshot.clone(),
            expected_candidate_revision_id: None,
            chapter_projection: None,
            task_output: None,
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    (project, snapshot)
}

fn digest(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}
pub fn marker(chapter: usize) -> String {
    format!("章证{}", char::from_u32(0x4e00 + chapter as u32).unwrap())
}

fn chapter_fixture(chapter: usize) -> (String, Value, Vec<Value>, Vec<Value>) {
    let mut text = marker(chapter);
    let mut entities = Vec::new();
    let mut assertions = Vec::new();
    for slot in 0..CLAIM_PAIRS {
        let name = format!("人物{}", char::from_u32(0x5000 + slot as u32).unwrap());
        let quote = format!("{name}在港城并知晓此事");
        let start = text.chars().count();
        text.push_str(&quote);
        let end = text.chars().count();
        let evidence = json!({"quote":quote,"start":start,"end":end});
        let person = format!("person-{slot}");
        let proposition = format!("location-{chapter}-{slot}");
        entities.push(json!({"id":person,"kind":"character","displayName":name,"aliases":[],"evidence":evidence}));
        entities.push(json!({"id":proposition,"kind":"proposition","displayName":quote,"aliases":[],"evidence":evidence}));
        assertions.push(json!({"kind":"relation","subjectId":person,"targetId":"port","predicate":"locatedAt",
            "relationType":"locatedAt","propositionId":proposition,"value":true,"validFrom":chapter,"evidence":evidence}));
        assertions.push(json!({"kind":"epistemic","subjectId":proposition,"propositionId":proposition,"predicate":"knows",
            "epistemicOwnerId":person,"epistemicState":"known","epistemicTime":chapter,"value":true,"evidence":evidence}));
    }
    let port_start = text[..text.find("港城").unwrap()].chars().count();
    entities.push(
        json!({"id":"port","kind":"location","displayName":"港城","aliases":[],
        "evidence":{"quote":"港城","start":port_start,"end":port_start + 2}}),
    );
    let mut seed = chapter as u64;
    while text.chars().count() < 2000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        text.push(char::from_u32(0x6000 + ((seed >> 32) % 512) as u32).unwrap());
    }
    let first = text.chars().take(1000).collect::<String>();
    let second = text.chars().skip(1000).collect::<String>();
    let groups = json!([{"id":format!("scene-{chapter}-a"),"sceneId":format!("scene-{chapter}-a"),"body":first},
        {"id":format!("scene-{chapter}-b"),"sceneId":format!("scene-{chapter}-b"),"body":second}]);
    // All anchored claims lie in the first scene; the separator occurs after them.
    (
        format!("{first}\n\n---\n\n{second}"),
        groups,
        entities,
        assertions,
    )
}
