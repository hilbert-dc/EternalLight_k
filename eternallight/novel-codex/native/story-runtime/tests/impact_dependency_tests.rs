use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::*;
use novel_codex_story_runtime::{RuntimeState, router};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use tower::ServiceExt;

const AUTHOR: &str = "dependency-author-token-123456789012345678";
const CANDIDATE: &str = "dependency-candidate-token-12345678901234";

fn save(
    repository: &mut SqliteStoryRepository,
    project: &ProjectId,
    head: SnapshotId,
    chapter: usize,
    suffix: &str,
    previous: Option<RevisionId>,
) -> SnapshotId {
    let next = SnapshotId::parse(format!("snapshot-{suffix}")).unwrap();
    let content = json!({"chapterNumber":chapter,"title":"信件","body":"信件引发调查，调查揭露证词，港城有白塔。"}).to_string();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project.clone(),
            branch_id: BranchId::parse("main").unwrap(),
            object_id: ObjectId::parse(format!("draft-chapter-{chapter}")).unwrap(),
            object_kind: "draft".into(),
            revision_id: RevisionId::parse(format!("revision-{suffix}")).unwrap(),
            commit_id: CommitId::parse(format!("commit-{suffix}")).unwrap(),
            idempotency_key: IdempotencyKey::parse(suffix).unwrap(),
            request_digest: content.clone(),
            content_digest: content.clone(),
            content_json: content,
            expected_project_revision: 0,
            expected_snapshot_id: head,
            next_snapshot_id: next.clone(),
            expected_candidate_revision_id: previous,
            chapter_projection: Some(ChapterProjection {
                chapter_id: format!("chapter-{chapter}"),
                number: chapter as i64,
                title: "信件".into(),
            }),
            task_output: None,
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    next
}

fn build(repository: &mut SqliteStoryRepository) -> (ProjectId, SnapshotId) {
    let project = ProjectId::parse("dependency-budget-project").unwrap();
    let mut head = SnapshotId::parse("snapshot-initial").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "依赖预算".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            head.clone(),
        )
        .unwrap();
    let evidence = json!({"quote":"信件","start":0,"end":2});
    let entities = [
        ("seed", "event"),
        ("investigation", "event"),
        ("testimony", "event"),
        ("port", "location"),
        ("proposition", "proposition"),
        ("echo-target", "event"),
        ("hidden", "event"),
    ]
    .map(
        |(id, kind)| json!({"id":id,"kind":kind,"displayName":id,"aliases":[],"evidence":evidence}),
    );
    for chapter in 1..=12 {
        head = save(
            repository,
            &project,
            head,
            chapter,
            &format!("chapter-{chapter}"),
            /*previous*/ None,
        );
        let revision = RevisionId::parse(format!("revision-chapter-{chapter}")).unwrap();
        let object = ObjectId::parse(format!("draft-chapter-{chapter}")).unwrap();
        let batch = repository
            .begin_modeling_batch(BeginModelingBatch {
                project_id: &project,
                object_id: &object,
                source_revision_id: &revision,
                expected_revision: 0,
                extractor_version: "dependency-fixture",
                now: 2,
            })
            .unwrap();
        let task = format!("task-{chapter}");
        let thread = format!("thread-{chapter}");
        repository
            .create_task(CreateTaskRequest {
                task_id: task.clone(),
                project_id: project.clone(),
                thread_id: thread.clone(),
                operation: "modelKnowledge".into(),
                input_revision: 0,
                model: None,
                target_json: json!({"batchId":batch.id}).to_string(),
                input_manifest_json: "{}".into(),
                capability_hash: "a".repeat(64),
                capability_expires_at: i64::MAX,
            })
            .unwrap();
        let relation = |from, to, kind| json!({"kind":"relation","subjectId":from,"targetId":to,"predicate":kind,"relationType":kind,"propositionId":"proposition","value":true,"validFrom":1,"evidence":evidence});
        let mut assertions = (0..90)
            .map(|_| relation("seed", "port", "locatedAt"))
            .collect::<Vec<_>>();
        // A dependency behind a non-dependency bridge must never enter the impact frontier.
        assertions.push(relation("seed", "echo-target", "echoes"));
        if chapter == 12 {
            assertions.push(relation("seed", "investigation", "causes"));
            assertions.push(relation("investigation", "testimony", "reveals"));
            assertions.push(relation("echo-target", "hidden", "causes"));
        }
        repository
            .publish_modeling_batch(PublishModelingBatch {
                project_id: &project,
                batch_id: &batch.id,
                task_id: &task,
                thread_id: &thread,
                entities: &entities,
                assertions: &assertions,
                now: 3,
            })
            .unwrap();
        repository
            .complete_task(
                &project,
                &task,
                &thread,
                "published dependency evidence".into(), None,
                /*now*/ 4,
            )
            .unwrap();
    }
    // Capture every published source without invalidating any of the modeled drafts.
    let content = json!({"premise":"依赖证据"}).to_string();
    let snapshot = SnapshotId::parse("snapshot-captured").unwrap();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project.clone(),
            branch_id: BranchId::parse("main").unwrap(),
            object_id: ObjectId::parse("brief").unwrap(),
            object_kind: "brief".into(),
            revision_id: RevisionId::parse("revision-brief").unwrap(),
            commit_id: CommitId::parse("commit-brief").unwrap(),
            idempotency_key: IdempotencyKey::parse("capture").unwrap(),
            request_digest: content.clone(),
            content_digest: content.clone(),
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

fn impact(
    repository: &SqliteStoryRepository,
    project: &ProjectId,
    scope: ImpactAnalysisScope<'_>,
    limit: usize,
) -> Value {
    repository
        .analyze_explicit_impact(ImpactAnalysisQuery {
            project_id: project,
            scope,
            changed_entity_id: "seed",
            world_time: Some(2),
            max_hops: 3,
            limit,
            revision_pair: None,
        })
        .unwrap()
}

#[tokio::test]
async fn dependency_recall_filters_before_sql_limits_and_preserves_rule_provenance() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("dependencies.sqlite3");
    let mut repository = SqliteStoryRepository::open(&path).unwrap();
    let (project, snapshot) = build(&mut repository);
    let before = repository
        .branch_head(&project, &BranchId::parse("main").unwrap())
        .unwrap();
    let general = repository
        .query_relation_graph(RelationGraphQuery {
            project_id: &project,
            scope: RelationGraphScope::Current,
            entity_id: "seed",
            relation_type: None,
            direction: RelationDirection::Outgoing,
            world_time: Some(2),
            max_hops: 3,
            limit: 100,
        })
        .unwrap();
    assert_eq!(general["coverage"]["truncatedEdges"], true);
    assert!(
        general["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["relationType"] == "locatedAt")
    );
    let report = impact(
        &repository,
        &project,
        ImpactAnalysisScope::Current,
        /*limit*/ 50,
    );
    assert_eq!(report["dependencyRuleVersion"], "explicit-dependencies-v1");
    assert_eq!(report["coverage"]["explicitGraphComplete"], true);
    assert_eq!(report["coverage"]["graphCoverage"]["scannedEdges"], 2);
    assert_eq!(
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|finding| (
                finding["affectedEntityId"].clone(),
                finding["hopCount"].clone()
            ))
            .collect::<Vec<_>>(),
        vec![
            (json!("investigation"), json!(1)),
            (json!("testimony"), json!(2))
        ]
    );
    assert_eq!(
        report["findings"][1]["evidencePath"]
            .as_array()
            .unwrap()
            .iter()
            .map(|edge| edge["dependencyRuleId"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!("explicit-dependencies-v1/causes"),
            json!("explicit-dependencies-v1/reveals")
        ]
    );
    assert_eq!(report["coverage"]["globalImpactComplete"], false);
    assert_eq!(
        repository
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap(),
        before
    );
    let app = router(RuntimeState::new(path, AUTHOR.into(), CANDIDATE.into()).unwrap());
    let uri = format!(
        "/api/projects/{project}/impact-analysis?entityId=seed&worldTime=2&maxHops=3&limit=50"
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("authorization", format!("Bearer {AUTHOR}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let http_report: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(http_report, report);
    let capped = impact(
        &repository,
        &project,
        ImpactAnalysisScope::Current,
        /*limit*/ 1,
    );
    assert_eq!(capped["coverage"]["truncatedPaths"], true);
    assert_eq!(capped["coverage"]["explicitGraphComplete"], false);
    let head = save(
        &mut repository,
        &project,
        snapshot.clone(),
        /*chapter*/ 12,
        "changed",
        Some(RevisionId::parse("revision-chapter-12").unwrap()),
    );
    assert_eq!(
        impact(
            &repository,
            &project,
            ImpactAnalysisScope::Current,
            /*limit*/ 50
        )["findings"],
        json!([])
    );
    let frozen = impact(
        &repository,
        &project,
        ImpactAnalysisScope::Snapshot(&snapshot),
        /*limit*/ 50,
    );
    assert_eq!(frozen["findings"], report["findings"]);
    assert_eq!(
        repository
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap()
            .snapshot_id,
        head
    );
}

use novel_codex_story_core::{ImpactAnalysisRepository, ModelingOperationRepository, ModelingRepository, ProjectRevisionRepository, RelationGraphRepository, TasksRepository};
