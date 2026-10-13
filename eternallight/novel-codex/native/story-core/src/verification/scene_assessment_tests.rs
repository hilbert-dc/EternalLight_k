use crate::{
    BranchId, CommitId, CreateTaskRequest, IdempotencyKey, ObjectId, ProjectId, ProjectRecord,
    RepositoryError, RevisionId, SaveCandidateRequest, SnapshotId, SqliteStoryRepository,
    TaskOutputBinding,
};
use pretty_assertions::assert_eq;
use rusqlite::params;
use serde_json::{Value, json};

use super::validate_scene_assessment_write;

#[test]
fn oversized_frozen_commitment_is_rejected_instead_of_truncating_required_criteria() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("oversized-assessment").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "Budget".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
    let body = "塔".repeat(/*n*/ 2000);
    save(&mut repository, &project, ("draft-first", "draft", "source"), ("initial", "source-snapshot"),
        json!({"chapterId":"first","chapterNumber":1,"body":body,"sceneGroups":[{"id":"group","body":body}]}), /*task_output*/None).unwrap();
    save(&mut repository, &project, ("commitment-lamp", "commitment", "lamp-1"), ("source-snapshot", "lamp-snapshot"),
        json!({"payload":{"id":"lamp","kind":"suspense","summary":"谜".repeat(/*n*/500),"fulfillmentCriteria":"秘".repeat(/*n*/1000),
            "relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,"targetWindow":{"kind":"open"},"status":"open","decisionReason":null,
            "setupEvidenceRefs":[{"objectId":"draft-first","revisionId":"source","quote":body,"start":0,"end":2000}],"fulfillmentEvidenceRefs":[]}}), /*task_output*/None).unwrap();
    let before = repository
        .branch_head(&project, &BranchId::parse("main").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        crate::scene_commitment_context::resolve(
            &repository.connection,
            &project,
            &json!({"type":"commitment","commitmentId":"lamp","revisionId":"lamp-1"}),
            /*chapter*/ 2
        ),
        Err(RepositoryError::InvalidContent(
            "bound commitment input exceeds 9KB".into()
        ))
    );
    assert_eq!(
        repository
            .branch_head(&project, &BranchId::parse("main").unwrap())
            .unwrap()
            .unwrap(),
        before
    );
}

#[test]
fn frozen_proposal_survives_report_replacement_but_not_author_establishment() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("frozen-assessment").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "Frozen".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
    save(&mut repository, &project, ("draft-first", "draft", "source"), ("initial", "source-snapshot"),
        json!({"chapterId":"first","chapterNumber":1,"body":"白塔的秘密。","sceneGroups":[{"id":"group","body":"白塔的秘密。"}]}), None).unwrap();
    repository.connection.execute(
        "INSERT INTO story_chapters_v2(chapter_id,project_id,number,title,status) VALUES ('first',?1,1,'First','draft')",
        [project.as_str()],
    ).unwrap();
    let proposal = json!({"id":"lamp","kind":"suspense","summary":"白塔的秘密","fulfillmentCriteria":"解释原来的秘密",
        "relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,"targetWindow":{"kind":"open"},"status":"open","decisionReason":null,
        "setupEvidenceRefs":[{"objectId":"draft-first","revisionId":"source","quote":"白塔","start":0,"end":2}],"fulfillmentEvidenceRefs":[]});
    for (id, before, after) in [
        ("report-1", "source-snapshot", "report-snapshot-1"),
        ("report-2", "report-snapshot-1", "report-snapshot-2"),
    ] {
        let task = repository
            .create_task(CreateTaskRequest {
                task_id: format!("task-{id}"),
                project_id: project.clone(),
                thread_id: format!("thread-{id}"),
                operation: "extractCommitments".into(),
                input_revision: 0,
                model: None,
                target_json:
                    json!({"chapterId":"first","chapterNumber":1,"sourceRevisionId":"source"})
                        .to_string(),
                input_manifest_json: "{}".into(),
                capability_hash: "a".repeat(/*n*/ 64),
                capability_expires_at: i64::MAX,
            })
            .unwrap();
        let mut candidate = proposal.clone();
        if id == "report-2" {
            candidate["fulfillmentCriteria"] = json!("新报告中的另一种解释");
        }
        save(&mut repository, &project, ("commitment-extraction-first", "commitmentExtraction", id), (before, after),
            json!({"payload":{"chapterId":"first","sourceRevisionId":"source","examinedRanges":[{"start":0,"end":2}],"candidates":[candidate]}}),
            Some(TaskOutputBinding { task_id: task.task_id.clone(), thread_id: task.thread_id.clone(), output_kind: "commitmentExtraction".into(), completeness: None })).unwrap();
        repository
            .complete_task(
                &project,
                &task.task_id,
                &task.thread_id,
                "saved".into(), None,
                /*now*/ 2,
            )
            .unwrap();
    }
    let original = json!({"type":"proposal","reportRevisionId":"report-1","proposalId":"lamp","sourceRevisionId":"source"});
    let frozen = crate::scene_commitment_context::resolve(
        &repository.connection,
        &project,
        &original,
        /*chapter*/ 2,
    )
    .unwrap();
    assert_eq!(frozen["referenceState"], "current");
    assert_eq!(frozen["fulfillmentCriteria"], "解释原来的秘密");
    let newer = json!({"type":"proposal","reportRevisionId":"report-2","proposalId":"lamp","sourceRevisionId":"source"});
    let established = repository
        .establish_commitment(crate::EstablishCommitment {
            project_id: project.clone(),
            origin: crate::CommitmentExtractionOrigin {
                chapter_id: "first".into(),
                report_revision_id: "report-2".into(),
                proposal_id: "lamp".into(),
            },
            expected_project_revision: 0,
            base_snapshot_id: SnapshotId::parse("report-snapshot-2").unwrap(),
            idempotency_key: IdempotencyKey::parse("establish").unwrap(),
        })
        .unwrap();
    assert_eq!(
        crate::scene_commitment_context::resolve(
            &repository.connection,
            &project,
            &newer,
            /*chapter*/ 2
        )
        .unwrap()["referenceState"],
        "authorEstablished"
    );
    assert_eq!(
        crate::scene_commitment_context::resolve(
            &repository.connection,
            &project,
            &original,
            /*chapter*/ 2
        )
        .unwrap(),
        frozen
    );
    let old_revision = established.receipt.revision_id.as_str();
    let independent = json!({"type":"commitment","commitmentId":established.commitment_id,"revisionId":old_revision});
    let object = format!("commitment-{}", established.commitment_id);
    let mut changed: Value = serde_json::from_str(
        &repository
            .revision_content(&established.receipt.revision_id)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    changed["payload"]["fulfillmentCriteria"] = json!("作者调整后的新标准");
    let before = repository
        .branch_head(&project, &BranchId::parse("main").unwrap())
        .unwrap()
        .unwrap()
        .snapshot_id;
    save(
        &mut repository,
        &project,
        (&object, "commitment", "author-adjustment"),
        (before.as_str(), "author-snapshot"),
        changed,
        None,
    )
    .unwrap();
    assert_eq!(
        crate::scene_commitment_context::resolve(
            &repository.connection,
            &project,
            &independent,
            /*chapter*/ 2
        )
        .unwrap()["referenceState"],
        "revisionSuperseded"
    );
    let other = ProjectId::parse("other-project").unwrap();
    assert_eq!(
        crate::scene_commitment_context::resolve(
            &repository.connection,
            &other,
            &original,
            /*chapter*/ 2
        ),
        Err(RepositoryError::TaskOutputForbidden)
    );
}

fn save(
    repository: &mut SqliteStoryRepository,
    project_id: &ProjectId,
    object: (&str, &str, &str),
    snapshots: (&str, &str),
    content: Value,
    task_output: Option<TaskOutputBinding>,
) -> Result<(), RepositoryError> {
    let (object_id, kind, revision_id) = object;
    let (before, after) = snapshots;
    repository.save_candidate(SaveCandidateRequest {
        project_id: project_id.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse(object_id).unwrap(),
        object_kind: kind.into(),
        revision_id: RevisionId::parse(revision_id).unwrap(),
        commit_id: CommitId::parse(format!("commit-{revision_id}")).unwrap(),
        idempotency_key: IdempotencyKey::parse(format!("save-{revision_id}")).unwrap(),
        request_digest: revision_id.into(),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse(before).unwrap(),
        next_snapshot_id: SnapshotId::parse(after).unwrap(),
        expected_candidate_revision_id: repository
            .object_heads(project_id)?
            .into_iter()
            .find(|head| head.object_id.as_str() == object_id)
            .and_then(|head| head.candidate_revision_id),
        content_digest: revision_id.into(),
        content_json: content.to_string(),
        chapter_projection: None,
        task_output,
        linked_impact_ids: Vec::new(),
    })?;
    Ok(())
}

#[test]
fn assessment_is_task_bound_and_rejects_cross_scene_or_stale_evidence() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = ProjectId::parse("project-assessment").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "Assessment".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("snapshot-0").unwrap(),
        )
        .unwrap();
    save(&mut repository, &project_id, ("draft-prologue","draft","prior-1"), ("snapshot-0","snapshot-prior"), json!({"chapterId":"prologue","chapterNumber":1,"body":"白塔的秘密。","sceneGroups":[{"id":"prior-group","body":"白塔的秘密。"}]}), None).unwrap();
    save(&mut repository, &project_id, ("commitment-lamp","commitment","promise-1"), ("snapshot-prior","snapshot-promise"), json!({"payload":{
        "id":"lamp","kind":"suspense","summary":"白塔的秘密","fulfillmentCriteria":"解释白塔秘密的原因",
        "relatedEntityIds":[],"plotThreadId":null,"characterArcId":null,"targetWindow":{"kind":"open"},"status":"open","decisionReason":null,
        "setupEvidenceRefs":[{"objectId":"draft-prologue","revisionId":"prior-1","quote":"白塔","start":0,"end":2}],"fulfillmentEvidenceRefs":[]
    }}), None).unwrap();
    let reference = json!({"type":"commitment","commitmentId":"lamp","revisionId":"promise-1"});
    let plan = json!({"payload":{"chapters":[{
        "chapterPlanId":"plan-chapter-1", "chapterNumber":2,
        "scenes":[
            {"id":"scene-a","goal":"记下钟声","expectedEffects":[{"id":"effect-a","dimension":"knowledge","description":"叙述者记下钟声","commitmentRefs":[reference]} ]},
            {"id":"scene-b","goal":"看到灯灭","expectedEffects":[{"id":"effect-b","dimension":"world","description":"白塔夜灯熄灭","commitmentRefs":[]} ]}
        ]
    }]}});
    save(
        &mut repository,
        &project_id,
        ("artifact-plan", "plan", "plan-1"),
        ("snapshot-promise", "snapshot-1"),
        plan,
        None,
    )
    .unwrap();
    repository.connection.execute(
        "INSERT INTO story_chapters_v2(chapter_id,project_id,number,title,status) VALUES ('chapter-1',?1,2,'Two','draft')",
        [project_id.as_str()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_chapter_plan_bindings_v2(project_id,plan_revision_id,chapter_plan_id,chapter_id,created_at) VALUES (?1,'plan-1','plan-chapter-1','chapter-1',1)",
        [project_id.as_str()],
    ).unwrap();
    let draft = json!({
        "chapterId":"chapter-1", "chapterNumber":2,
        "body":"晨钟响起。\n\n---\n\n夜灯熄灭。",
        "sceneGroups":[
            {"id":"group-a","sceneId":"scene-a","body":"晨钟响起。"},
            {"id":"group-b","sceneId":"scene-b","body":"夜灯熄灭。"}
        ],
        "sourcePlanRefs":{"localOnly":false,"planRevisionId":"plan-1","chapterPlanId":"plan-chapter-1"}
    });
    save(
        &mut repository,
        &project_id,
        ("draft-chapter-1", "draft", "draft-1"),
        ("snapshot-1", "snapshot-2"),
        draft,
        None,
    )
    .unwrap();
    let assertions = json!([
        {"kind":"event","subjectId":"morning","predicate":"ring","storyTime":null,
            "value":{"participants":[{"role":"item","entityId":"bell"}],"preconditions":[],"effects":[],"untilExclusive":null},
            "evidence":{"quote":"晨钟","start":0,"end":2}},
        {"kind":"event","subjectId":"night","predicate":"extinguish","storyTime":null,
            "value":{"participants":[{"role":"item","entityId":"lamp"}],"preconditions":[],"effects":[],"untilExclusive":null},
            "evidence":{"quote":"夜灯","start":12,"end":14}}
    ]);
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id,project_id,object_id,source_revision_id,input_project_revision,extractor_version,status,assertions_json,entities_json,created_at) VALUES ('batch-1',?1,'draft-chapter-1','draft-1',0,'test','ready',?2,?3,1)",
        params![project_id.as_str(), assertions.to_string(), json!([
            {"id":"morning","kind":"event","displayName":"晨钟响起","aliases":[],"evidence":assertions[0]["evidence"]},
            {"id":"night","kind":"event","displayName":"夜灯熄灭","aliases":[],"evidence":assertions[1]["evidence"]},
            {"id":"bell","kind":"item","displayName":"晨钟","aliases":[],"evidence":assertions[0]["evidence"]},
            {"id":"lamp","kind":"item","displayName":"夜灯","aliases":[],"evidence":assertions[1]["evidence"]}
        ]).to_string()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_knowledge_heads_v2(project_id,object_id,batch_id) VALUES (?1,'draft-chapter-1','batch-1')",
        [project_id.as_str()],
    ).unwrap();
    let task = repository
        .create_task(CreateTaskRequest {
            task_id: "assessment-task".into(),
            project_id: project_id.clone(),
            thread_id: "assessment-thread".into(),
            operation: "assessSceneOutcome".into(),
            input_revision: 0,
            model: None,
            target_json: json!({
                "chapterId":"chapter-1", "chapterNumber":2,"planRevisionId":"plan-1",
                "draftRevisionId":"draft-1", "knowledgeBatchId":"batch-1",
                "inputSnapshotId":"snapshot-2", "intentGeneration":0
            })
            .to_string(),
            input_manifest_json: "{}".into(),
            capability_hash: "a".repeat(64),
            capability_expires_at: 100,
        })
        .unwrap();
    let valid = json!({
        "chapterId":"chapter-1", "planRevisionId":"plan-1",
        "draftRevisionId":"draft-1", "knowledgeBatchId":"batch-1",
        "inputSnapshotId":"snapshot-2", "intentGeneration":0,
        "assessments":[
            {"sceneId":"scene-a","verdict":"realized","rationale":"钟声被写入正文",
             "recommendation":"continue","effectAssessments":[{"effectId":"effect-a","verdict":"realized",
               "rationale":"钟声见于正文","actualEventRefs":[{"modelingBatchId":"batch-1","assertionIndex":0}],
               "commitmentAssessments":[{"reference":reference,"referenceState":"current","outcome":"advanced","rationale":"出现了与秘密有关的钟声，但尚未解释原因","actualEventRefs":[{"modelingBatchId":"batch-1","assertionIndex":0}]}]}]},
            {"sceneId":"scene-b","verdict":"deviated","rationale":"灯灭的因果仍待展开",
             "recommendation":"repairDraft","effectAssessments":[{"effectId":"effect-b","verdict":"deviated",
               "rationale":"灯灭发生，但原因不清楚","actualEventRefs":[{"modelingBatchId":"batch-1","assertionIndex":1}],"commitmentAssessments":[]}]}
        ]
    });
    let context = repository.task_context_commitments(&task).unwrap();
    assert_eq!(
        context["assessmentContext"]["entries"][0]["fulfillmentCriteria"],
        "解释白塔秘密的原因"
    );
    assert_eq!(
        context["assessmentContext"]["entries"][0]["reference"],
        reference
    );
    let assessment_path = "/assessments/0/effectAssessments/0/commitmentAssessments";
    for (field, value) in [
        (
            "reference",
            json!({"type":"commitment","commitmentId":"other-project","revisionId":"promise-1"}),
        ),
        ("referenceState", json!("sourceChanged")),
        ("actualEventRefs", json!([])),
        (
            "actualEventRefs",
            json!([{"modelingBatchId":"batch-1","assertionIndex":1}]),
        ),
        ("outcome", json!("confirmAutomatically")),
        ("status", json!("fulfilled")),
    ] {
        let mut forged = valid.clone();
        forged.pointer_mut(assessment_path).unwrap()[0][field] = value;
        assert_eq!(
            validate_scene_assessment_write(&repository.connection, &task, &forged.to_string()),
            Err(RepositoryError::TaskOutputForbidden)
        );
    }
    let mut missing = valid.clone();
    *missing.pointer_mut(assessment_path).unwrap() = json!([]);
    assert_eq!(
        validate_scene_assessment_write(&repository.connection, &task, &missing.to_string()),
        Err(RepositoryError::TaskOutputForbidden)
    );
    let mut contradiction = valid.clone();
    contradiction.pointer_mut(assessment_path).unwrap()[0]["outcome"] = json!("contradicted");
    assert_eq!(
        validate_scene_assessment_write(&repository.connection, &task, &contradiction.to_string()),
        Err(RepositoryError::TaskOutputForbidden)
    );
    contradiction["assessments"][0]["verdict"] = json!("deviated");
    contradiction["assessments"][0]["recommendation"] = json!("askAuthor");
    assert_eq!(
        validate_scene_assessment_write(&repository.connection, &task, &contradiction.to_string()),
        Ok(())
    );
    let mut insufficient = valid.clone();
    insufficient.pointer_mut(assessment_path).unwrap()[0]["outcome"] =
        json!("insufficientEvidence");
    insufficient.pointer_mut(assessment_path).unwrap()[0]["actualEventRefs"] = json!([]);
    insufficient["assessments"][0]["verdict"] = json!("insufficientEvidence");
    assert_eq!(
        validate_scene_assessment_write(&repository.connection, &task, &insufficient.to_string()),
        Ok(())
    );
    let mut duplicate = valid.clone();
    let item = duplicate.pointer(assessment_path).unwrap()[0].clone();
    duplicate
        .pointer_mut(assessment_path)
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(item);
    assert_eq!(
        validate_scene_assessment_write(&repository.connection, &task, &duplicate.to_string()),
        Err(RepositoryError::TaskOutputForbidden)
    );
    let mut cross_scene = valid.clone();
    cross_scene["assessments"][0]["effectAssessments"][0]["actualEventRefs"][0]["assertionIndex"] =
        json!(1);
    assert!(matches!(
        validate_scene_assessment_write(&repository.connection, &task, &cross_scene.to_string()),
        Err(RepositoryError::TaskOutputForbidden)
    ));
    let mut missing_effect = valid.clone();
    missing_effect["assessments"][0]["effectAssessments"] = json!([]);
    assert!(
        validate_scene_assessment_write(&repository.connection, &task, &missing_effect.to_string())
            .is_err()
    );
    let mut inconsistent_verdict = valid.clone();
    inconsistent_verdict["assessments"][1]["verdict"] = json!("realized");
    assert!(
        validate_scene_assessment_write(
            &repository.connection,
            &task,
            &inconsistent_verdict.to_string()
        )
        .is_err()
    );
    assert!(matches!(
        save(
            &mut repository,
            &project_id,
            (
                "scene-assessment-chapter-1",
                "sceneAssessment",
                "assessment-unbound"
            ),
            ("snapshot-2", "snapshot-unbound"),
            valid.clone(),
            None,
        ),
        Err(RepositoryError::TaskOutputForbidden)
    ));
    save(
        &mut repository,
        &project_id,
        (
            "scene-assessment-chapter-1",
            "sceneAssessment",
            "assessment-1",
        ),
        ("snapshot-2", "snapshot-3"),
        valid.clone(),
        Some(TaskOutputBinding {
            task_id: task.task_id.clone(),
            thread_id: task.thread_id.clone(),
            output_kind: "sceneAssessment".into(),
            completeness: None,
        }),
    )
    .unwrap();
    assert_eq!(
        repository
            .complete_task(
                &project_id,
                &task.task_id,
                &task.thread_id,
                "done".into(), None,
                2
            )
            .unwrap()
            .status,
        "completed"
    );
    assert!(
        repository
            .scene_assessment(&project_id, "chapter-1")
            .unwrap()
            .unwrap()
            .current
    );
    assert!(
        repository
            .with_read_snapshot(|| repository
                .scene_assessment(&project_id, "chapter-1")
                .unwrap()
                .unwrap()
                .current)
            .unwrap()
    );
    repository
        .connection
        .execute(
            "UPDATE story_chapters_v2 SET number=3 WHERE project_id=?1 AND chapter_id='chapter-1'",
            [project_id.as_str()],
        )
        .unwrap();
    assert!(
        !repository
            .scene_assessment(&project_id, "chapter-1")
            .unwrap()
            .unwrap()
            .current
    );
    repository
        .connection
        .execute(
            "UPDATE story_chapters_v2 SET number=2 WHERE project_id=?1 AND chapter_id='chapter-1'",
            [project_id.as_str()],
        )
        .unwrap();
    save(&mut repository, &project_id, ("draft-prologue","draft","prior-2"), ("snapshot-3","snapshot-4"), json!({"chapterId":"prologue","chapterNumber":1,"body":"白塔已消失。","sceneGroups":[{"id":"prior-group","body":"白塔已消失。"}]}), None).unwrap();
    let historical = repository
        .scene_assessment(&project_id, "chapter-1")
        .unwrap()
        .unwrap();
    assert!(!historical.current);
    assert_eq!(historical.content, valid);
    assert_eq!(
        crate::scene_commitment_context::resolve(
            &repository.connection,
            &project_id,
            &reference,
            /*chapter*/ 2
        )
        .unwrap()["referenceState"],
        "sourceChanged"
    );
    assert_eq!(
        crate::scene_commitment_context::resolve(
            &repository.connection,
            &project_id,
            &reference,
            /*chapter*/ 1
        ),
        Err(RepositoryError::TaskOutputForbidden)
    );
    repository.connection.execute(
        "UPDATE story_object_heads_v2 SET candidate_revision_id = 'draft-other' WHERE project_id = ?1 AND object_id = 'draft-chapter-1'",
        [project_id.as_str()],
    ).unwrap();
    assert!(matches!(
        validate_scene_assessment_write(
            &repository.connection,
            &task,
            &json!({
                "chapterId":"chapter-1", "planRevisionId":"plan-1", "draftRevisionId":"draft-1",
                "knowledgeBatchId":"batch-1", "inputSnapshotId":"snapshot-2",
                "intentGeneration":0, "assessments":[]
            })
            .to_string()
        ),
        Err(RepositoryError::TaskInputStale)
    ));
}

use crate::{CommitmentContextRepository, CommitmentEstablishmentRepository, ProjectRevisionRepository, ReadSnapshotRepository, SceneAssessmentRepository, TasksRepository};
