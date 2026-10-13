use novel_codex_story_core::*;
use serde_json::{Value, json};

fn fixture() -> (SqliteStoryRepository, TaskRecord) {
    fixture_with_readonly(false)
}
fn fixture_with_readonly(readonly: bool) -> (SqliteStoryRepository, TaskRecord) {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = ProjectId::parse("adaptation-project").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: p.clone(),
            title: "原作".into(),
            logline: "".into(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("s0").unwrap(),
    )
    .unwrap();
    repo.save_candidate(SaveCandidateRequest {
        project_id: p.clone(),
        branch_id: BranchId::parse("main").unwrap(),
        object_id: ObjectId::parse("draft-chapter-1").unwrap(),
        object_kind: "draft".into(),
        revision_id: RevisionId::parse("novel-1").unwrap(),
        commit_id: CommitId::parse("commit-1").unwrap(),
        idempotency_key: IdempotencyKey::parse("save-1").unwrap(),
        request_digest: "request".into(),
        content_digest: "digest".into(),
        content_json: json!({"title":"雨夜","body":"她在雨中找到了失踪的信。"}).to_string(),
        expected_project_revision: 0,
        expected_snapshot_id: SnapshotId::parse("s0").unwrap(),
        next_snapshot_id: SnapshotId::parse("s1").unwrap(),
        expected_candidate_revision_id: None,
        chapter_projection: Some(ChapterProjection {
            chapter_id: "chapter-1".into(),
            number: 1,
            title: "雨夜".into(),
        }),
        task_output: None,
        linked_impact_ids: vec![],
    })
    .unwrap();
    repo.conversation_store()
        .create_conversation(
            &p,
            CreateConversationRequest {
                scope_id: None,
                thread_id: "thread".into(),
                title: "改编".into(),
                forked_from_id: None,
                last_turn_id: None,
            },
            1,
        )
        .unwrap();
    let source_digest = repo.project_source_watermarks(&p).unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let root=repo.create_budgeted_root_task(CreateTaskRequest{task_id:"root".into(),project_id:p,thread_id:"thread".into(),operation:"advisory".into(),input_revision:0,model:None,target_json:json!({"type":"project","readOnly":readonly,"mediaBudget":{"maxRequests":2,"unitLimits":{"images":2},"maxCostMinor":null,"currency":null,"expiresAt":4000000000000_i64}}).to_string(),input_manifest_json:json!({"authorIntent":"将第一章改成漫画，再基于漫画制作动画；保留找到信的结局。"}).to_string(),capability_hash:"a".repeat(64),capability_expires_at:4000000000},&InitialWorkBudget{authorization_id:"budget".into(),author_message_id:"message".into(),expected_base_snapshot_id:"s1".into(),expected_source_digest:source_digest,max_requests:Some(10),max_tokens:None,deadline_at:None},2).unwrap();
    (repo, root)
}
fn start(
    repo: &mut SqliteStoryRepository,
    root: &TaskRecord,
    step_id: &str,
    name: &str,
) -> TaskRecord {
    let work = repo
        .work_task_for_attempt(&root.project_id, &root.task_id)
        .unwrap();
    let step = work.steps.iter().find(|s| s.id == step_id).unwrap();
    let mut target = step.target.clone();
    target["resumeStepId"] = json!(step.id);
    repo.create_task(CreateTaskRequest {
        task_id: name.into(),
        project_id: root.project_id.clone(),
        thread_id: root.thread_id.clone(),
        operation: "advisory".into(),
        input_revision: step.input_revision,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: json!({"authorIntent":"模型任务说明不应替代原话"}).to_string(),
        capability_hash: "b".repeat(64),
        capability_expires_at: 4000000000,
    })
    .unwrap()
}
fn finish(repo: &mut SqliteStoryRepository, t: &TaskRecord) {
    repo.complete_task(
        &t.project_id,
        &t.task_id,
        &t.thread_id,
        "已核对本步骤真实回执".into(),
        None,
        10,
    )
    .unwrap();
    let work = repo
        .work_task_for_attempt(&t.project_id, &t.task_id)
        .unwrap();
    // Controlled usage receipt; no actual provider is invoked in this repository test.
    repo.charge_work_provider_usage(&t.project_id,&work.id,&t.task_id,&json!({"runId":t.task_id,"budgetId":format!("budget-{}",t.task_id),"status":"completed","limits":{"maxRequests":null,"maxTokens":null,"maxDurationMs":null},"usage":{"requests":1,"tokens":1,"unknownTokens":0,"overrun":false}})).unwrap();
}
fn request(
    repo: &SqliteStoryRepository,
    t: &TaskRecord,
    medium: CreativeMedium,
) -> RequestMediaAdaptation {
    let sources = repo.task_adaptation_sources(t, 0, 20).unwrap()["sources"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .map(|v| serde_json::from_value(v).unwrap())
        .collect();
    RequestMediaAdaptation {
        medium,
        title: "改编作品".into(),
        sources,
        instruction: "保留找到信的结局，按作者要求制作当前媒介；实际保存后再决定后继步骤".into(),
        idempotency_key: "adapt".into(),
    }
}
fn save(repo: &mut SqliteStoryRepository, t: &TaskRecord) -> MultimediaWorkspace {
    repo.update_task(
        &t.project_id,
        &t.task_id,
        &t.thread_id,
        TaskUpdate {
            status: "running".into(),
            error: None,
            result_summary: None,
            now: 5,
        },
    )
    .unwrap();
    let w = repo.media_task_workspace(t).unwrap();
    let mut document = serde_json::to_value(w.document).unwrap();
    document["creativeDesign"] = json!("雨夜发现信封，末镜揭开寄信人身份。");
    repo.save_task_multimedia_document(
        t,
        &SaveMultimediaDocument {
            expected_revision_id: None,
            idempotency_key: format!("save-{}", t.task_id),
            document: serde_json::from_value(document).unwrap(),
            read_set: vec![],
            sources: vec![],
        },
    )
    .unwrap()
}
#[test]
fn novel_comic_animation_share_work_author_intent_budget_and_real_dependencies() {
    let (mut repo, root) = fixture();
    let work = repo
        .work_task_for_attempt(&root.project_id, &root.task_id)
        .unwrap();
    let budget = serde_json::to_value(
        repo.work_budget_authorization(&root.project_id, &work.id)
            .unwrap(),
    )
    .unwrap();
    let r = request(&repo, &root, CreativeMedium::Comic);
    let first = repo.request_media_adaptation(&root, &r, 3).unwrap();
    assert_eq!(repo.request_media_adaptation(&root, &r, 4).unwrap(), first);
    assert_eq!(repo.creative_editions(&root.project_id).unwrap().len(), 2);
    finish(&mut repo, &root);
    let comic = start(
        &mut repo,
        &root,
        first["stepId"].as_str().unwrap(),
        "comic-task",
    );
    let manifest: Value = serde_json::from_str(&comic.input_manifest_json).unwrap();
    assert_eq!(
        manifest["authorIntent"],
        serde_json::from_str::<Value>(&root.input_manifest_json).unwrap()["authorIntent"]
    );
    assert_eq!(manifest["candidateInstruction"], r.instruction);
    let saved = save(&mut repo, &comic);
    let mut next = request(&repo, &comic, CreativeMedium::Animation);
    next.sources = vec![saved.document_source.unwrap()];
    let second = repo.request_media_adaptation(&comic, &next, 5).unwrap();
    finish(&mut repo, &comic);
    let animation = start(
        &mut repo,
        &root,
        second["stepId"].as_str().unwrap(),
        "animation-task",
    );
    let w = repo.media_task_workspace(&animation).unwrap();
    assert_eq!(w.edition.sources, next.sources);
    save(&mut repo, &animation);
    finish(&mut repo, &animation);
    let final_work = repo
        .work_task_for_attempt(&root.project_id, &animation.task_id)
        .unwrap();
    assert_eq!(final_work.id, work.id);
    assert_eq!(final_work.steps.len(), 3);
    assert_eq!(final_work.status, "completed");
    assert_eq!(repo.creative_editions(&root.project_id).unwrap().len(), 3);
    assert_eq!(
        serde_json::to_value(
            repo.work_budget_authorization(&root.project_id, &work.id)
                .unwrap()
        )
        .unwrap(),
        budget
    );
}
#[test]
fn adaptation_rejects_foreign_sources_readonly_and_mutated_retries_without_creating_editions() {
    let (mut repo, root) = fixture();
    let mut r = request(&repo, &root, CreativeMedium::Comic);
    r.sources[0].revision_id = "future".into();
    assert!(repo.request_media_adaptation(&root, &r, 3).is_err());
    assert_eq!(repo.creative_editions(&root.project_id).unwrap().len(), 1);
    let r = request(&repo, &root, CreativeMedium::Comic);
    repo.request_media_adaptation(&root, &r, 3).unwrap();
    let mut changed = r;
    changed.title = "同键不同请求".into();
    assert!(repo.request_media_adaptation(&root, &changed, 3).is_err());
    assert_eq!(repo.creative_editions(&root.project_id).unwrap().len(), 2);
}

#[test]
fn readonly_and_paused_work_cannot_register_adaptations() {
    let (mut repo, root) = fixture_with_readonly(true);
    let r = request(&repo, &root, CreativeMedium::Comic);
    assert!(repo.request_media_adaptation(&root, &r, 3).is_err());
    assert_eq!(repo.creative_editions(&root.project_id).unwrap().len(), 1);
    let (mut repo, root) = fixture();
    let r = request(&repo, &root, CreativeMedium::Comic);
    let work = repo
        .work_task_for_attempt(&root.project_id, &root.task_id)
        .unwrap();
    repo.control_work_task(
        &root.project_id,
        &work.id,
        "pause",
        work.control_revision,
        3,
    )
    .unwrap();
    assert!(repo.request_media_adaptation(&root, &r, 4).is_err());
    assert_eq!(repo.creative_editions(&root.project_id).unwrap().len(), 1);
}

#[test]
fn registered_adaptation_cannot_complete_without_saved_document() {
    let (mut repo, root) = fixture();
    let r = request(&repo, &root, CreativeMedium::Comic);
    let selected = repo.request_media_adaptation(&root, &r, 3).unwrap();
    finish(&mut repo, &root);
    let comic = start(
        &mut repo,
        &root,
        selected["stepId"].as_str().unwrap(),
        "empty-comic",
    );
    assert!(matches!(
        repo.complete_task(
            &comic.project_id,
            &comic.task_id,
            &comic.thread_id,
            "声称完成".into(),
            None,
            5
        ),
        Err(RepositoryError::TaskOutputMissing)
    ));
    assert_ne!(
        repo.work_task_for_attempt(&root.project_id, &comic.task_id)
            .unwrap()
            .status,
        "completed"
    );
}

#[test]
fn novel_root_budget_authorizes_only_registered_media_step() {
    let (mut repo, root) = fixture();
    let r = request(&repo, &root, CreativeMedium::Comic);
    let selected = repo.request_media_adaptation(&root, &r, 3).unwrap();
    finish(&mut repo, &root);
    let child = start(
        &mut repo,
        &root,
        selected["stepId"].as_str().unwrap(),
        "funded-comic",
    );
    let child = repo
        .update_task(
            &child.project_id,
            &child.task_id,
            &child.thread_id,
            TaskUpdate {
                status: "running".into(),
                error: None,
                result_summary: None,
                now: 5,
            },
        )
        .unwrap();
    let intent = repo
        .prepare_media_operation(
            &child,
            &PrepareMediaOperation {
                provider_id: "images".into(),
                kind: "image.generate".into(),
                parameters: json!({"prompt":"雨夜信封"}),
                idempotency_key: "panel-image".into(),
            },
            6,
        )
        .unwrap();
    assert_eq!(intent["grant"]["maxRequests"], 2);
    assert_eq!(intent["grant"]["workTaskId"], selected["workTaskId"]);
    assert_eq!(
        intent["grant"]["scopeId"],
        serde_json::from_str::<Value>(&child.target_json).unwrap()["scopeId"]
    );
    assert_eq!(intent["operation"]["target"], json!({"kind":"whole"}));
}

use novel_codex_story_core::{InitialWorkBudgetRepository, MultimediaAdaptationRepository, MultimediaOperationsRepository, MultimediaRepository, OrdinaryWorkBudgetRepository, ProjectRevisionRepository, TaskInputRepository, TasksRepository, WorkBudgetAuthorizationRepository, WorkTasksRepository};
