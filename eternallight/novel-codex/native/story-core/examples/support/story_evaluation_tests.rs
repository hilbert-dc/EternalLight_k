use std::path::PathBuf;

use novel_codex_story_core::*;
use pretty_assertions::assert_eq;
use rusqlite::{Connection, OpenFlags, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{evaluate, evaluation};

struct Fixture {
    _directory: tempfile::TempDir,
    database: PathBuf,
    annotations: PathBuf,
    gold: Value,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("中文作品.sqlite3");
    let annotations = directory.path().join("标注.json");
    let mut repository = SqliteStoryRepository::open(&database).unwrap();
    let project = ProjectId::parse("evaluation-port").unwrap();
    let branch = BranchId::parse("main").unwrap();
    let initial = SnapshotId::parse("initial").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "港城工程样本".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            branch.clone(),
            initial.clone(),
        )
        .unwrap();
    let body = "🙂林夕锁门，张衡因此迟到。后来他改走海路。";
    let content = json!({"chapterNumber":1,"title":"封闭的门","body":body,
        "sceneGroups":[{"id":"scene-1","sceneId":"scene-1","body":body}]})
    .to_string();
    let digest = format!("{:x}", Sha256::digest(content.as_bytes()));
    let object = ObjectId::parse("draft-chapter-1").unwrap();
    let revision = RevisionId::parse("draft-r1").unwrap();
    repository
        .save_candidate(SaveCandidateRequest {
            project_id: project.clone(),
            branch_id: branch,
            object_id: object.clone(),
            object_kind: "draft".into(),
            revision_id: revision.clone(),
            commit_id: CommitId::parse("save-1").unwrap(),
            idempotency_key: IdempotencyKey::parse("save-1").unwrap(),
            request_digest: digest.clone(),
            content_digest: digest,
            content_json: content,
            expected_project_revision: 0,
            expected_snapshot_id: initial,
            next_snapshot_id: SnapshotId::parse("after-save").unwrap(),
            expected_candidate_revision_id: None,
            chapter_projection: Some(ChapterProjection {
                chapter_id: "chapter-1".into(),
                number: 1,
                title: "封闭的门".into(),
            }),
            task_output: None,
            linked_impact_ids: Vec::new(),
        })
        .unwrap();
    let evidence = |quote: &str| {
        let start = body[..body.find(quote).unwrap()].chars().count();
        json!({"quote":quote,"start":start,"end":start+quote.chars().count()})
    };
    let entities = [
        json!({"id":"lock","kind":"event","displayName":"锁门","aliases":[],"evidence":evidence("锁门")}),
        json!({"id":"late","kind":"event","displayName":"迟到","aliases":[],"evidence":evidence("迟到")}),
        json!({"id":"cause","kind":"proposition","displayName":"锁门导致迟到","aliases":[],"evidence":evidence("林夕锁门，张衡因此迟到")}),
    ];
    let assertions = [
        json!({"kind":"relation","subjectId":"lock","targetId":"late","relationType":"causes",
        "predicate":"causes","propositionId":"cause","value":true,"validFrom":1,"evidence":evidence("林夕锁门，张衡因此迟到")}),
    ];
    let batch = repository
        .begin_modeling_batch(BeginModelingBatch {
            project_id: &project,
            object_id: &object,
            source_revision_id: &revision,
            expected_revision: 0,
            extractor_version: "engineering-v1",
            now: 2,
        })
        .unwrap();
    repository
        .create_task(CreateTaskRequest {
            task_id: "task-model".into(),
            project_id: project.clone(),
            thread_id: "thread-model".into(),
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
            task_id: "task-model",
            thread_id: "thread-model",
            entities: &entities,
            assertions: &assertions,
            now: 3,
        })
        .unwrap();
    repository
        .complete_task(
            &project,
            "task-model",
            "thread-model",
            "fixture".into(),
            /*now*/ 4,
        )
        .unwrap();
    // Keep committed content in WAL; a byte-copy-only reader would miss this update.
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .unwrap();
    connection
        .execute(
            "UPDATE story_projects_v2 SET logline='committed WAL' WHERE project_id=?1",
            [project.as_str()],
        )
        .unwrap();
    let gold = json!({"datasetId":"port-engineering-v1","revision":"1","projectId":project.as_str(),
        "annotation":{"kind":"engineeringFixture","reviewer":"implementer","reference":"repository-fixture"},
        "cases":[{"id":"literal","category":"literal","scenario":10,"query":{"type":"evidence","text":"锁门","snapshotId":null,
            "expected":[{"objectId":object.as_str(),"revisionId":revision.as_str(),"path":"/sceneGroups/0/body","start":3,"end":5,"quote":"锁门"}]}},
            {"id":"causal-impact","category":"explicitDependency","scenario":17,"query":{"type":"impact","entityId":"lock","snapshotId":null,"worldTime":2,"maxHops":2,
                "expected":[{"type":"entity","id":"late"},{"type":"text","objectId":object.as_str()}]}}]});
    std::fs::write(&annotations, serde_json::to_vec(&gold).unwrap()).unwrap();
    drop(connection);
    Fixture {
        _directory: directory,
        database,
        annotations,
        gold,
    }
}

#[test]
fn production_queries_measure_unique_targets_and_resolve_unicode_evidence() {
    let fixture = fixture();
    let before = std::fs::read(&fixture.database).unwrap();
    let report = evaluate(&fixture.database, &fixture.annotations).unwrap();
    assert_eq!(
        report["channels"],
        json!({
        "evidence":{"expected":1,"recalled":1,"queries":1,"queryErrors":0,"invalidReturnedAnchors":0,"recall":1.0,"thresholdMet":true},
        "impact":{"expected":2,"recalled":2,"queries":1,"queryErrors":0,"invalidReturnedAnchors":0,"recall":1.0,"thresholdMet":true}})
    );
    assert_eq!(report["measuredThresholdsMet"], true);
    assert_eq!(report["fullArchitectureAcceptanceVerified"], false);
    assert_eq!(report["annotationClaim"]["independentlyVerified"], false);
    assert_eq!(report["referencedScenarios"], json!([10, 17]));
    assert_eq!(std::fs::read(&fixture.database).unwrap(), before);
    let serialized = report.to_string();
    assert!(
        !serialized.contains("林夕")
            && !serialized.contains("quote")
            && !serialized.contains("锁门")
    );
    assert_eq!(report["datasetSha256"].as_str().unwrap().len(), 64);
    assert_eq!(report["corpusCaptureSha256"].as_str().unwrap().len(), 64);
}

#[test]
fn natural_language_misses_and_unmeasured_channels_are_not_passes() {
    let mut fixture = fixture();
    fixture.gold["cases"][0]["query"]["text"] = json!("为什么张衡没能按时抵达？");
    std::fs::write(
        &fixture.annotations,
        serde_json::to_vec(&fixture.gold).unwrap(),
    )
    .unwrap();
    let report = evaluate(&fixture.database, &fixture.annotations).unwrap();
    assert_eq!(report["channels"]["evidence"]["recall"], 0.0);
    assert_eq!(
        report["cases"][0]["missed"],
        json!([{"objectId":"draft-chapter-1","revisionId":"draft-r1","path":"/sceneGroups/0/body","start":3,"end":5}])
    );
    assert_eq!(report["measuredThresholdsMet"], false);
    fixture.gold["cases"].as_array_mut().unwrap().pop();
    std::fs::write(
        &fixture.annotations,
        serde_json::to_vec(&fixture.gold).unwrap(),
    )
    .unwrap();
    let report = evaluate(&fixture.database, &fixture.annotations).unwrap();
    assert_eq!(report["channels"]["impact"]["recall"], Value::Null);
    assert_eq!(report["channels"]["impact"]["thresholdMet"], Value::Null);
}

#[test]
fn poor_categories_cannot_hide_behind_aggregate_recall() {
    let mut fixture = fixture();
    let template = fixture.gold["cases"][0].clone();
    for index in 1..9 {
        let mut case = template.clone();
        case["id"] = json!(format!("literal-{index}"));
        fixture.gold["cases"].as_array_mut().unwrap().push(case);
    }
    let mut missed = template;
    missed["id"] = json!("semantic-miss");
    missed["category"] = json!("naturalLanguage");
    missed["query"]["text"] = json!("为什么张衡没能按时抵达？");
    fixture.gold["cases"].as_array_mut().unwrap().push(missed);
    std::fs::write(
        &fixture.annotations,
        serde_json::to_vec(&fixture.gold).unwrap(),
    )
    .unwrap();
    let report = evaluate(&fixture.database, &fixture.annotations).unwrap();
    assert_eq!(report["channels"]["evidence"]["recall"], 0.9);
    assert_eq!(report["channels"]["evidence"]["thresholdMet"], true);
    assert_eq!(report["measuredThresholdsMet"], false);
    assert_eq!(
        report["categories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["category"] == "naturalLanguage")
            .unwrap()["channels"]["evidence"]["recall"],
        0.0
    );
}

#[test]
fn malformed_gold_cannot_inflate_scores_or_claim_foreign_evidence() {
    let fixture = fixture();
    let mut empty = fixture.gold.clone();
    empty["cases"][0]["query"]["expected"] = json!([]);
    assert!(evaluation::parse_dataset(&serde_json::to_vec(&empty).unwrap()).is_err());
    let mut duplicate = fixture.gold.clone();
    let anchor = duplicate["cases"][0]["query"]["expected"][0].clone();
    duplicate["cases"][0]["query"]["expected"]
        .as_array_mut()
        .unwrap()
        .push(anchor);
    assert!(evaluation::parse_dataset(&serde_json::to_vec(&duplicate).unwrap()).is_err());
    let mut unknown = fixture.gold.clone();
    unknown["cases"][0]["query"]["sql"] = json!("select 1");
    assert!(evaluation::parse_dataset(&serde_json::to_vec(&unknown).unwrap()).is_err());
    for change in [
        json!({"quote":"藏信"}),
        json!({"path":"/body"}),
        json!({"objectId":"another-draft"}),
        json!({"revisionId":"another-revision"}),
        json!({"start":2,"end":4}),
    ] {
        let mut gold = fixture.gold.clone();
        for (key, value) in change.as_object().unwrap() {
            gold["cases"][0]["query"]["expected"][0][key] = value.clone();
        }
        std::fs::write(&fixture.annotations, serde_json::to_vec(&gold).unwrap()).unwrap();
        // /body is valid gold but was not returned, and must not receive field-level credit.
        if change["path"] == "/body" {
            assert_eq!(
                evaluate(&fixture.database, &fixture.annotations).unwrap()["channels"]["evidence"]
                    ["recall"],
                0.0
            );
        } else {
            assert!(evaluate(&fixture.database, &fixture.annotations).is_err());
        }
    }
}

#[test]
fn invalid_returned_source_and_failed_queries_are_recorded_without_raw_text() {
    let fixture = fixture();
    let connection = Connection::open(&fixture.database).unwrap();
    connection.execute("UPDATE story_search_documents_v2 SET text='🙂林夕锁门造假' WHERE field_path='/sceneGroups/0/body'",[]).unwrap();
    let report = evaluate(&fixture.database, &fixture.annotations).unwrap();
    // The query's short valid span remains resolvable even if a non-evidence suffix is corrupt.
    assert_eq!(report["channels"]["evidence"]["invalidReturnedAnchors"], 0);
    connection.execute("UPDATE story_search_documents_v2 SET text='锁门假位置' WHERE field_path='/sceneGroups/0/body'",[]).unwrap();
    let report = evaluate(&fixture.database, &fixture.annotations).unwrap();
    assert_eq!(report["channels"]["evidence"]["invalidReturnedAnchors"], 1);
    assert_eq!(report["channels"]["evidence"]["recall"], 0.0);
    let mut gold = fixture.gold.clone();
    gold["cases"][1]["query"]["snapshotId"] = json!("missing-snapshot");
    std::fs::write(&fixture.annotations, serde_json::to_vec(&gold).unwrap()).unwrap();
    let report = evaluate(&fixture.database, &fixture.annotations).unwrap();
    assert_eq!(report["channels"]["impact"]["queryErrors"], 1);
    assert_eq!(report["cases"][1]["coverage"], Value::Null);
    assert_eq!(report["measuredThresholdsMet"], false);
}

#[test]
fn committed_wal_is_captured_without_checkpointing_the_original() {
    let fixture = fixture();
    let connection = Connection::open(&fixture.database).unwrap();
    connection
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .unwrap();
    connection.execute("UPDATE story_projects_v2 SET logline='new committed value' WHERE project_id='evaluation-port'",[]).unwrap();
    // A valid production project committed only to WAL distinguishes capture from byte-copy.
    let mut repository = SqliteStoryRepository::open(&fixture.database).unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: ProjectId::parse("wal-project").unwrap(),
                title: "WAL 作品".into(),
                logline: "new committed value".into(),
                created_at: 5,
                updated_at: 5,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("wal-initial").unwrap(),
        )
        .unwrap();
    drop(repository);
    let mut missing_project = fixture.gold.clone();
    missing_project["projectId"] = json!("wal-project");
    std::fs::write(
        &fixture.annotations,
        serde_json::to_vec(&missing_project).unwrap(),
    )
    .unwrap();
    let wal_path = PathBuf::from(format!("{}-wal", fixture.database.display()));
    let before_wal = std::fs::read(&wal_path).unwrap();
    let before = std::fs::read(&fixture.database).unwrap();
    // The project exists only in WAL, so finding it and then rejecting its stale source binding
    // proves the capture included WAL rather than merely testing unchanged indexed content.
    let error = evaluate(&fixture.database, &fixture.annotations)
        .unwrap_err()
        .to_string();
    assert_eq!(error, "gold anchor is invalid or outside its scope");
    assert_eq!(std::fs::read(&fixture.database).unwrap(), before);
    assert_eq!(std::fs::read(&wal_path).unwrap(), before_wal);
    let readonly =
        Connection::open_with_flags(&fixture.database, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        readonly
            .query_row(
                "SELECT logline FROM story_projects_v2 WHERE project_id=?1",
                params!["wal-project"],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "new committed value"
    );
}
