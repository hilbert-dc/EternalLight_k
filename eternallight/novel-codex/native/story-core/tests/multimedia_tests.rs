use novel_codex_story_core::*;
use serde_json::json;
use sha2::{Digest, Sha256};

fn create(
    repo: &mut SqliteStoryRepository,
    medium: CreativeMedium,
    key: &str,
) -> CreativeProjectCreated {
    repo.create_creative_project(
        &CreateCreativeProjectRequest {
            title: "独立创作".into(),
            logline: String::new(),
            medium,
            idempotency_key: key.into(),
        },
        100,
    )
    .unwrap()
}
fn project(p: &CreativeProjectCreated) -> ProjectId {
    ProjectId::parse(p.project.id.clone()).unwrap()
}
fn document(title: &str) -> MultimediaDocument {
    serde_json::from_value(json!({"kind":"comic","title":title,"format":"pages","pages":[{"id":"page-a","title":"第一页","width":800,"height":1200,"readingDirection":"rightToLeft","panels":[{"id":"panel-a","description":"雨夜","rect":{"x":0,"y":0,"width":800,"height":600},"assetRevisionId":null,"referenceAssetRevisionIds":[],"balloons":[],"locked":false}]}]})).unwrap()
}
fn save(doc: MultimediaDocument, revision: Option<String>, key: &str) -> SaveMultimediaDocument {
    SaveMultimediaDocument {
        expected_revision_id: revision,
        document: doc,
        idempotency_key: key.into(),
        read_set: vec![],
        sources: vec![],
    }
}
#[test]
fn standalone_editions_are_durable_without_novel_rows() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("creative.db");
    let mut repo = SqliteStoryRepository::open(&file).unwrap();
    for (i, medium) in [CreativeMedium::Comic, CreativeMedium::Animation]
        .into_iter()
        .enumerate()
    {
        let p = create(&mut repo, medium, &format!("create-{i}"));
        let e = &p.editions[0];
        assert_eq!(p.editions.len(), 1);
        assert_eq!(e.medium, medium);
        assert!(repo.require_novel_project(&project(&p)).is_err());
        let w = repo
            .multimedia_workspace(&project(&p), &e.id, None)
            .unwrap();
        assert_eq!(w.document.medium(), medium);
        assert!(w.revision_id.is_none());
    }
    drop(repo);
    let repo = SqliteStoryRepository::open(&file).unwrap();
    assert_eq!(repo.list_projects().unwrap().len(), 2);
    let db = rusqlite::Connection::open(&file).unwrap();
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM story_chapters_v2", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
#[test]
fn typed_document_cas_replay_and_branches_are_isolated() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic, "create");
    let pid = project(&p);
    let e = &p.editions[0];
    let request = save(document("初稿"), None, "save-1");
    let first = repo
        .save_multimedia_document(&pid, &e.id, None, &request)
        .unwrap();
    let replay = repo
        .save_multimedia_document(&pid, &e.id, None, &request)
        .unwrap();
    assert_eq!(first.revision_id, replay.revision_id);
    assert!(matches!(
        repo.save_multimedia_document(&pid, &e.id, None, &save(document("冲突"), None, "save-1")),
        Err(RepositoryError::IdempotencyConflict)
    ));
    assert!(
        repo.save_multimedia_document(&pid, &e.id, None, &save(document("过期"), None, "save-2"))
            .is_err()
    );
    let branch = repo
        .create_media_branch(
            &pid,
            &e.id,
            &CreateMediaBranch {
                source_branch_id: e.branch_id.clone(),
                title: "试验稿".into(),
                idempotency_key: "branch".into(),
            },
        )
        .unwrap();
    repo.save_multimedia_document(
        &pid,
        &e.id,
        Some(&branch.branch_id),
        &save(document("分支稿"), first.revision_id.clone(), "branch-save"),
    )
    .unwrap();
    assert_eq!(
        repo.multimedia_workspace(&pid, &e.id, None)
            .unwrap()
            .document,
        first.document
    );
    let another = repo
        .create_creative_edition(
            &pid,
            &CreateEditionRequest {
                title: "动画".into(),
                medium: CreativeMedium::Animation,
                idempotency_key: "animation".into(),
                sources: vec![],
            },
            101,
        )
        .unwrap();
    assert!(
        repo.multimedia_workspace(&pid, &another.id, Some(&branch.branch_id))
            .is_err()
    );
}
#[test]
fn pinned_adaptation_remains_valid_and_current_dependency_detects_change() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic, "create");
    let pid = project(&p);
    let e = &p.editions[0];
    let first = repo
        .save_multimedia_document(
            &pid,
            &e.id,
            None,
            &save(document("来源"), None, "source-first"),
        )
        .unwrap();
    let source = MediaSourceRef {
        project_id: pid.to_string(),
        scope_id: e.scope_id.clone(),
        branch_id: e.branch_id.clone(),
        object_id: e.id.clone(),
        revision_id: first.revision_id.clone().unwrap(),
        digest: format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&first.document).unwrap())
        ),
        selector: MediaSelector::Panel {
            page_id: "page-a".into(),
            panel_id: "panel-a".into(),
            region: None,
        },
    };
    let target = repo
        .create_creative_edition(
            &pid,
            &CreateEditionRequest {
                title: "另一漫画版".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "target".into(),
                sources: vec![source.clone()],
            },
            101,
        )
        .unwrap();
    repo.save_multimedia_document(
        &pid,
        &e.id,
        None,
        &save(document("来源更新"), first.revision_id, "source-next"),
    )
    .unwrap();
    let mut request = save(document("改编"), None, "adapt");
    request.read_set.push(MediaReadDependency {
        source: source.clone(),
        freshness: MediaFreshness::MustBeCurrent,
    });
    assert!(
        repo.save_multimedia_document(&pid, &target.id, None, &request)
            .is_err()
    );
    request.read_set[0].freshness = MediaFreshness::Pinned;
    let saved = repo
        .save_multimedia_document(&pid, &target.id, None, &request)
        .unwrap();
    assert!(saved.revision_id.is_some());
    request.expected_revision_id = saved.revision_id;
    request.idempotency_key = "forged-source".into();
    request.read_set[0].source.scope_id = target.scope_id.clone();
    assert!(
        repo.save_multimedia_document(&pid, &target.id, None, &request)
            .is_err()
    );
}
#[test]
fn asset_import_requires_registered_pin_and_scope() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic, "create");
    let pid = project(&p);
    let e = &p.editions[0];
    let r = RegisterMediaAsset {
        operation_id: "import-a".into(),
        idempotency_key: "asset-a".into(),
        asset: MediaAssetInput {
            name: "角色.png".into(),
            kind: MediaAssetKind::Image,
            mime_type: "image/png".into(),
            digest: "a".repeat(64),
            bytes: 50,
            width: Some(32),
            height: Some(32),
            duration: None,
            sample_rate: None,
            channels: None,
        },
        sources: vec![],
    };
    assert!(repo.register_media_asset(&pid, &e.id, &r, 101).is_err());
    repo.begin_media_import(
        &pid,
        &e.id,
        &BeginMediaImport {
            operation_id: "import-a".into(),
            idempotency_key: "begin-a".into(),
            name: "角色.png".into(),
        },
        100,
    )
    .unwrap();
    let asset = repo.register_media_asset(&pid, &e.id, &r, 101).unwrap();
    assert!(asset.preview_url.is_empty());
    assert_eq!(
        repo.register_media_asset(&pid, &e.id, &r, 102)
            .unwrap()
            .asset_revision_id,
        asset.asset_revision_id
    );
    let other = repo
        .create_creative_edition(
            &pid,
            &CreateEditionRequest {
                title: "其他版".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "other".into(),
                sources: vec![],
            },
            101,
        )
        .unwrap();
    let mut d = document("越界");
    if let MultimediaDocument::Comic { pages, .. } = &mut d {
        pages[0].panels[0].asset_revision_id = Some(asset.asset_revision_id);
    }
    assert!(
        repo.save_multimedia_document(&pid, &other.id, None, &save(d, None, "forged-use"))
            .is_err()
    );
}
#[test]
fn regions_lock_and_exact_time_are_checked() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic, "create");
    let pid = project(&p);
    let e = &p.editions[0];
    let mut d = document("锁定");
    if let MultimediaDocument::Comic { pages, .. } = &mut d {
        pages[0].panels[0].locked = true;
    }
    let first = repo
        .save_multimedia_document(&pid, &e.id, None, &save(d, None, "lock"))
        .unwrap();
    let mut changed = first.document.clone();
    if let MultimediaDocument::Comic { pages, .. } = &mut changed {
        pages[0].panels[0].description = "不能覆盖".into();
    }
    assert!(matches!(
        repo.save_multimedia_document(
            &pid,
            &e.id,
            None,
            &save(changed, first.revision_id.clone(), "locked-change")
        ),
        Err(RepositoryError::ObjectLocked)
    ));
    let mut bad = first.document;
    if let MultimediaDocument::Comic { pages, .. } = &mut bad {
        pages[0].panels[0].rect.width = 900.0;
    }
    assert!(
        repo.save_multimedia_document(&pid, &e.id, None, &save(bad, first.revision_id, "overflow"))
            .is_err()
    );
    assert!(
        serde_json::from_value::<MediaTime>(
            json!({"value":1.5,"rate":{"numerator":24,"denominator":1}})
        )
        .is_err()
    );
}

#[test]
fn task_reads_frozen_media_and_writes_only_its_edition() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic, "create");
    let pid = project(&p);
    let e = &p.editions[0];
    repo.conversation_store()
        .create_conversation(
            &pid,
            CreateConversationRequest {
                scope_id: Some(e.scope_id.clone()),
                thread_id: "media-thread".into(),
                title: "漫画".into(),
                forked_from_id: None,
                last_turn_id: None,
            },
            100,
        )
        .unwrap();
    let first = repo
        .save_multimedia_document(
            &pid,
            &e.id,
            None,
            &save(document("任务来源"), None, "before-task"),
        )
        .unwrap();
    let request=CreateTaskRequest{task_id:"media-task".into(),project_id:pid.clone(),thread_id:"media-thread".into(),operation:"advisory".into(),input_revision:first.revision,model:None,target_json:json!({"type":"edition","editionId":e.id,"scopeId":e.scope_id,"branchId":e.branch_id,"revisionId":first.revision_id}).to_string(),input_manifest_json:json!({"authorIntent":"调整第一页"}).to_string(),capability_hash:"a".repeat(64),capability_expires_at:4_000_000_000};
    let task = repo.create_task(request.clone()).unwrap();
    let frozen = repo.media_task_workspace(&task).unwrap();
    assert_eq!(frozen.revision_id, first.revision_id);
    let modified = repo
        .save_task_multimedia_document(
            &task,
            &save(document("任务修改"), first.revision_id.clone(), "task-save"),
        )
        .unwrap();
    assert_ne!(modified.revision_id, first.revision_id);
    assert_eq!(
        repo.media_task_workspace(&task).unwrap().revision_id,
        first.revision_id
    );
    repo.assert_task_inputs_current(&task).unwrap();
    let other = repo
        .create_creative_edition(
            &pid,
            &CreateEditionRequest {
                title: "其他版".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "other".into(),
                sources: vec![],
            },
            101,
        )
        .unwrap();
    let mut wrong = request;
    wrong.task_id = "wrong-task".into();
    wrong.target_json=json!({"type":"edition","editionId":other.id,"scopeId":other.scope_id,"branchId":other.branch_id}).to_string();
    assert!(repo.create_task(wrong).is_err());
    repo.save_multimedia_document(
        &pid,
        &e.id,
        None,
        &save(document("用户后改"), modified.revision_id, "author-after"),
    )
    .unwrap();
    assert!(repo.assert_task_inputs_current(&task).is_err());
}

#[test]
fn media_root_budget_binds_its_branch_and_source_digest() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic, "create");
    let pid = project(&p);
    let e = &p.editions[0];
    repo.conversation_store()
        .create_conversation(
            &pid,
            CreateConversationRequest {
                scope_id: Some(e.scope_id.clone()),
                thread_id: "budget-thread".into(),
                title: "漫画".into(),
                forked_from_id: None,
                last_turn_id: None,
            },
            100,
        )
        .unwrap();
    let w = repo.multimedia_workspace(&pid, &e.id, None).unwrap();
    let request=CreateTaskRequest{task_id:"assigned-by-budget".into(),project_id:pid,thread_id:"budget-thread".into(),operation:"advisory".into(),input_revision:w.revision,model:None,target_json:json!({"type":"edition","editionId":e.id,"scopeId":e.scope_id,"branchId":e.branch_id,"revisionId":null}).to_string(),input_manifest_json:json!({"authorIntent":"画一篇漫画"}).to_string(),capability_hash:"a".repeat(64),capability_expires_at:4_000_000_000};
    let initial = InitialWorkBudget {
        authorization_id: "media-authorization".into(),
        author_message_id: "media-author-message".into(),
        expected_base_snapshot_id: w.head_snapshot_id,
        expected_source_digest: w.source_digest,
        max_requests: Some(5),
        max_tokens: Some(10000),
        deadline_at: Some(10000),
    };
    let task = repo
        .create_budgeted_root_task(request.clone(), &initial, 100)
        .unwrap();
    assert_eq!(
        repo.create_budgeted_root_task(request, &initial, 100)
            .unwrap()
            .task_id,
        task.task_id
    );
    assert_eq!(repo.media_task_scope(&task).unwrap().id, e.id);
}

#[test]
fn image_only_author_message_keeps_empty_text_and_requires_owned_frozen_images() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("image-dialogue.db");
    let mut repo = SqliteStoryRepository::open(&file).unwrap();
    let p = create(&mut repo, CreativeMedium::Comic, "image-dialogue");
    let pid = project(&p);
    let edition = &p.editions[0];
    repo.begin_media_import(
        &pid,
        &edition.id,
        &BeginMediaImport {
            operation_id: "image-only-import".into(),
            idempotency_key: "image-only-pin".into(),
            name: "参考.png".into(),
        },
        100,
    )
    .unwrap();
    let asset = repo
        .register_media_asset(
            &pid,
            &edition.id,
            &RegisterMediaAsset {
                operation_id: "image-only-import".into(),
                idempotency_key: "image-only-asset".into(),
                asset: MediaAssetInput {
                    name: "参考.png".into(),
                    kind: MediaAssetKind::Image,
                    mime_type: "image/png".into(),
                    digest: "a".repeat(64),
                    bytes: 50,
                    width: Some(32),
                    height: Some(32),
                    duration: None,
                    sample_rate: None,
                    channels: None,
                },
                sources: vec![],
            },
            101,
        )
        .unwrap();
    repo.conversation_store()
        .create_conversation(
            &pid,
            CreateConversationRequest {
                scope_id: Some(edition.scope_id.clone()),
                thread_id: "image-thread".into(),
                title: "图片对话".into(),
                forked_from_id: None,
                last_turn_id: None,
            },
            100,
        )
        .unwrap();
    let workspace = repo.multimedia_workspace(&pid, &edition.id, None).unwrap();
    let media = json!({"scope":{"projectId":pid.as_str(),"editionId":edition.id,"scopeId":edition.scope_id,"branchId":edition.branch_id},"expectedRevisionId":workspace.revision_id,"selection":null,"imageAssetRevisionIds":[asset.asset_revision_id]});
    let body = json!({"messageId":"media-submission","sessionId":"media-host","input":{"text":"","focus":null,"connectionId":null,"media":media},"trigger":{"kind":"immediate"}});
    for field in ["imageAssetRevisionIds", "expectedRevisionId"] {
        let mut invalid = body.clone();
        invalid["input"]["media"][field] = if field == "imageAssetRevisionIds" {
            json!(["foreign-image"])
        } else {
            json!("stale-revision")
        };
        assert!(
            repo.conversation_store()
                .submit_conversation_message(
                    &pid,
                    "image-thread",
                    serde_json::from_value(invalid).unwrap(),
                    102
                )
                .is_err()
        );
    }
    repo.conversation_store()
        .submit_conversation_message(
            &pid,
            "image-thread",
            serde_json::from_value(body).unwrap(),
            102,
        )
        .unwrap();
    repo.conversation_store()
        .change_conversation_message(
            &pid,
            "image-thread",
            "media-submission",
            serde_json::from_value(
                json!({"action":"claim","expectedRevision":0,"sessionId":"media-host"}),
            )
            .unwrap(),
        )
        .unwrap();
    let request = CreateTaskRequest { task_id:"image-only-root".into(), project_id:pid.clone(), thread_id:"image-thread".into(), operation:"advisory".into(), input_revision:workspace.revision, model:None,
        target_json:json!({"type":"edition","editionId":edition.id,"scopeId":edition.scope_id,"branchId":edition.branch_id,"revisionId":workspace.revision_id,"selection":null,"imageAssetRevisionIds":[asset.asset_revision_id],"messageDispatch":{"messageId":"media-submission","sessionId":"media-host","revision":1}}).to_string(),
        input_manifest_json:json!({"authorIntent":""}).to_string(), capability_hash:"a".repeat(64), capability_expires_at:4_000_000_000 };
    for (index, images) in [
        json!([]),
        json!(["missing-image"]),
        json!([asset.asset_revision_id, asset.asset_revision_id]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut invalid = request.clone();
        invalid.task_id = format!("invalid-image-{index}");
        let mut target: serde_json::Value = serde_json::from_str(&invalid.target_json).unwrap();
        target["imageAssetRevisionIds"] = images;
        invalid.target_json = target.to_string();
        assert!(repo.create_task(invalid).is_err());
    }
    let other = create(&mut repo, CreativeMedium::Comic, "another-image-scope");
    let mut foreign = request.clone();
    foreign.task_id = "foreign-image-scope".into();
    let mut target: serde_json::Value = serde_json::from_str(&foreign.target_json).unwrap();
    target["scopeId"] = json!(other.editions[0].scope_id);
    foreign.target_json = target.to_string();
    assert!(repo.create_task(foreign).is_err());
    let initial = InitialWorkBudget {
        authorization_id: "image-only-authorization".into(),
        author_message_id: "image-only-message".into(),
        expected_base_snapshot_id: workspace.head_snapshot_id,
        expected_source_digest: workspace.source_digest,
        max_requests: Some(5),
        max_tokens: Some(10000),
        deadline_at: Some(10000),
    };
    let task = repo
        .create_budgeted_root_task(request, &initial, 100)
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&task.input_manifest_json).unwrap()["authorIntent"],
        ""
    );
    let receipt = repo
        .conversation_store()
        .conversation_message(&pid, "image-thread", "media-submission")
        .unwrap();
    assert_eq!(receipt["taskId"], task.task_id);
    assert_eq!(receipt["input"]["media"], media);
    assert_eq!(receipt["input"]["text"], "");
    repo.bind_task_turn(&pid, &task.task_id, "image-thread", "image-only-turn")
        .unwrap();
    drop(repo);
    let mut repo = SqliteStoryRepository::open(&file).unwrap();
    assert_eq!(
        repo.conversation_store()
            .thread_author_messages(&pid, "image-thread", &["image-only-turn".into()])
            .unwrap()["messages"],
        json!([{"turnId":"image-only-turn","text":"","imageAssetRevisionIds":[asset.asset_revision_id]}])
    );
}

fn media_budget_task(repo: &mut SqliteStoryRepository, p: &CreativeProjectCreated) -> TaskRecord {
    let pid = project(p);
    let e = &p.editions[0];
    repo.conversation_store()
        .create_conversation(
            &pid,
            CreateConversationRequest {
                scope_id: Some(e.scope_id.clone()),
                thread_id: "op-thread".into(),
                title: "媒体生成".into(),
                forked_from_id: None,
                last_turn_id: None,
            },
            100,
        )
        .unwrap();
    let w = repo.multimedia_workspace(&pid, &e.id, None).unwrap();
    let request=CreateTaskRequest{task_id:"set-by-budget".into(),project_id:pid,thread_id:"op-thread".into(),operation:"advisory".into(),input_revision:w.revision,model:None,target_json:json!({"type":"edition","editionId":e.id,"scopeId":e.scope_id,"branchId":e.branch_id,"mediaBudget":{"maxRequests":5,"unitLimits":{"images":5,"videoMilliseconds":30000,"audioMilliseconds":30000,"characters":5000},"maxCostMinor":null,"currency":null,"expiresAt":200000}}).to_string(),input_manifest_json:json!({"authorIntent":"生成漫画素材"}).to_string(),capability_hash:"a".repeat(64),capability_expires_at:4_000_000_000};
    repo.create_budgeted_root_task(
        request,
        &InitialWorkBudget {
            authorization_id: "original-media-grant".into(),
            author_message_id: "author-media-generation".into(),
            expected_base_snapshot_id: w.head_snapshot_id,
            expected_source_digest: w.source_digest,
            max_requests: Some(5),
            max_tokens: Some(10000),
            deadline_at: Some(10000),
        },
        100,
    )
    .unwrap()
}
fn operation_asset(
    repo: &mut SqliteStoryRepository,
    p: &CreativeProjectCreated,
    op: &str,
    index: usize,
) -> MediaAsset {
    let video = p.editions[0].medium == CreativeMedium::Animation;
    repo.register_media_asset(
        &project(p),
        &p.editions[0].id,
        &RegisterMediaAsset {
            operation_id: format!("{op}-output-{index}"),
            idempotency_key: format!("{op}-asset-{index}"),
            asset: MediaAssetInput {
                name: format!("output-{index}.png"),
                kind: if video {
                    MediaAssetKind::Video
                } else {
                    MediaAssetKind::Image
                },
                mime_type: if video { "video/mp4" } else { "image/png" }.into(),
                digest: format!("{:064x}", index + 1),
                bytes: 64,
                width: Some(32),
                height: Some(32),
                duration: if video {
                    Some(MediaTime {
                        value: 120,
                        rate: Rational {
                            numerator: 24,
                            denominator: 1,
                        },
                    })
                } else {
                    None
                },
                sample_rate: None,
                channels: None,
            },
            sources: vec![],
        },
        110,
    )
    .unwrap()
}
#[test]
fn media_operations_pin_before_dispatch_and_replay_same_grant() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic, "op-project");
    let task = media_budget_task(&mut repo, &p);
    let request = PrepareMediaOperation {
        provider_id: "configured-images".into(),
        kind: "image.generate".into(),
        parameters: json!({"prompt":"雨夜"}),
        idempotency_key: "first-image".into(),
    };
    let op = repo
        .prepare_media_operation(&task, &request, 100000)
        .unwrap();
    assert_eq!(op["grant"]["id"], "original-media-grant");
    assert_eq!(op["operation"]["reservation"]["units"]["images"], 1);
    assert_eq!(op["authorization"]["canDispatch"], true);
    assert_eq!(
        repo.prepare_media_operation(&task, &request, 100000)
            .unwrap()["operation"]["id"],
        op["operation"]["id"]
    );
    let mut conflicting = request.clone();
    conflicting.parameters = json!({"prompt":"不同请求"});
    assert!(matches!(
        repo.prepare_media_operation(&task, &conflicting, 100000),
        Err(RepositoryError::IdempotencyConflict)
    ));
    assert!(
        repo.complete_task(
            &project(&p),
            &task.task_id,
            &task.thread_id,
            "已完成".into(),
            None,
            101
        )
        .is_err()
    );
    let id = op["operation"]["id"].as_str().unwrap();
    let no_outputs = MediaOperationSeal {
        state: "ready".into(),
        outputs: Some(MediaOperationOutputs {
            asset_revision_ids: vec![],
            artifact_revision_id: None,
        }),
        provider_receipt: None,
        error: None,
    };
    assert!(
        repo.seal_media_operation(&project(&p), id, &no_outputs, 110000)
            .is_err()
    );
    repo.begin_media_operation_outputs(&project(&p), id, 1, 110000)
        .unwrap();
    let asset = operation_asset(&mut repo, &p, id, 0);
    let receipt = MediaOperationSeal {
        state: "ready".into(),
        outputs: Some(MediaOperationOutputs {
            asset_revision_ids: vec![asset.asset_revision_id],
            artifact_revision_id: None,
        }),
        provider_receipt: Some(json!({"id":"provider-result"})),
        error: None,
    };
    repo.seal_media_operation(&project(&p), id, &receipt, 111000)
        .unwrap();
    repo.seal_media_operation(&project(&p), id, &receipt, 112000)
        .unwrap();
    assert_eq!(
        repo.media_operation_intent(&project(&p), id, 112000)
            .unwrap()["state"],
        "ready"
    );
    assert!(
        repo.multimedia_workspace(&project(&p), &p.editions[0].id, None)
            .unwrap()
            .revision_id
            .is_none()
    );
}
#[test]
fn media_output_manifest_blocks_partial_seals_and_late_results_do_not_overwrite() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Animation, "batch-project");
    let task = media_budget_task(&mut repo, &p);
    let request = PrepareMediaOperation {
        provider_id: "configured-video".into(),
        kind: "video.generate".into(),
        parameters: json!({"prompt":"镜头","duration":5,"ratio":"1280:720"}),
        idempotency_key: "video".into(),
    };
    let intent = repo
        .prepare_media_operation(&task, &request, 100000)
        .unwrap();
    let op = intent["operation"]["id"].as_str().unwrap();
    repo.begin_media_operation_outputs(&project(&p), op, 2, 101000)
        .unwrap();
    let first = operation_asset(&mut repo, &p, op, 0);
    let mut seal = MediaOperationSeal {
        state: "ready".into(),
        outputs: Some(MediaOperationOutputs {
            asset_revision_ids: vec![first.asset_revision_id],
            artifact_revision_id: None,
        }),
        provider_receipt: None,
        error: None,
    };
    assert!(
        repo.seal_media_operation(&project(&p), op, &seal, 102000)
            .is_err()
    );
    let second = operation_asset(&mut repo, &p, op, 1);
    seal.outputs
        .as_mut()
        .unwrap()
        .asset_revision_ids
        .push(second.asset_revision_id);
    let late = repo
        .media_operation_intent(&project(&p), op, 300000)
        .unwrap();
    assert_eq!(late["authorization"]["canDispatch"], false);
    assert_eq!(late["authorization"]["canReconcile"], true);
    repo.seal_media_operation(&project(&p), op, &seal, 300000)
        .unwrap();
    assert!(
        repo.multimedia_workspace(&project(&p), &p.editions[0].id, None)
            .unwrap()
            .revision_id
            .is_none()
    );
    let another = create(&mut repo, CreativeMedium::Comic, "other-project");
    assert!(
        repo.media_operation_intent(&project(&another), op, 300000)
            .is_err()
    );
}

#[test]
fn operation_dispatch_stops_after_own_source_changes_and_failure_does_not_forge_outputs() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic, "op-cas");
    let task = media_budget_task(&mut repo, &p);
    let request = PrepareMediaOperation {
        provider_id: "configured-images".into(),
        kind: "image.generate".into(),
        parameters: json!({"prompt":"画第一页"}),
        idempotency_key: "source-bound".into(),
    };
    let prepared = repo
        .prepare_media_operation(&task, &request, 100000)
        .unwrap();
    let op = prepared["operation"]["id"].as_str().unwrap();
    repo.save_task_multimedia_document(&task, &save(document("模型后改"), None, "task-revised"))
        .unwrap();
    assert_eq!(
        repo.media_operation_intent(&project(&p), op, 101000)
            .unwrap()["authorization"]["canDispatch"],
        false
    );
    repo.seal_media_operation(
        &project(&p),
        op,
        &MediaOperationSeal {
            state: "failed".into(),
            outputs: None,
            provider_receipt: Some(json!({"notSent":true})),
            error: Some("服务未配置".into()),
        },
        101000,
    )
    .unwrap();
    let mut new_request = request.clone();
    new_request.idempotency_key = "after-own-save".into();
    let after = repo
        .prepare_media_operation(&task, &new_request, 101000)
        .unwrap();
    assert_eq!(after["authorization"]["canDispatch"], true);
    assert_eq!(
        after["operation"]["source"]["revisionId"],
        repo.multimedia_workspace(&project(&p), &p.editions[0].id, None)
            .unwrap()
            .revision_id
            .unwrap()
    );
    repo.seal_media_operation(
        &project(&p),
        after["operation"]["id"].as_str().unwrap(),
        &MediaOperationSeal {
            state: "cancelled".into(),
            outputs: None,
            provider_receipt: Some(json!({"notSent":true})),
            error: None,
        },
        102000,
    )
    .unwrap();
    let ready = MediaOperationSeal {
        state: "ready".into(),
        outputs: Some(MediaOperationOutputs {
            asset_revision_ids: vec!["forged".into()],
            artifact_revision_id: None,
        }),
        provider_receipt: None,
        error: None,
    };
    assert!(matches!(
        repo.seal_media_operation(&project(&p), op, &ready, 102000),
        Err(RepositoryError::IdempotencyConflict)
    ));
    let result = repo
        .complete_task(
            &project(&p),
            &task.task_id,
            &task.thread_id,
            "生成服务不可用，已保留分镜。".into(),
            None,
            103,
        )
        .unwrap();
    assert_eq!(result.status, "completed");
}

#[test]
fn workspace_pages_are_explicit_complete_entities_with_continuation() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic, "paging");
    let mut doc = document("分页");
    if let MultimediaDocument::Comic { pages, .. } = &mut doc {
        let template = pages[0].panels[0].clone();
        pages[0].panels = (0..5)
            .map(|n| {
                let mut panel = template.clone();
                panel.id = format!("panel-{n}");
                panel.description = "述".repeat(6000);
                panel
            })
            .collect();
    }
    let w = repo
        .save_multimedia_document(
            &project(&p),
            &p.editions[0].id,
            None,
            &save(doc, None, "save-pages"),
        )
        .unwrap();
    let overview = w.page(&MediaWorkspacePageRequest::default()).unwrap();
    assert!(overview.get("document").is_none());
    assert_eq!(overview["counts"]["panels"], 5);
    assert_eq!(overview["partialView"], true);
    let req = MediaWorkspacePageRequest {
        section: Some("panels".into()),
        page_id: Some("page-a".into()),
        limit: Some(3),
        ..Default::default()
    };
    let first = w.page(&req).unwrap();
    assert!(first.to_string().len() <= 64 * 1024);
    assert!(first["nextOffset"].is_u64());
    assert_eq!(first["items"][0]["textFields"][0]["totalChars"], 6000);
    assert!(first["items"][0].get("description").is_none());
    let mut next = req.clone();
    next.offset = Some(first["nextOffset"].as_u64().unwrap() as usize);
    let last = w.page(&next).unwrap();
    assert!(last["nextOffset"].is_null());
    assert_eq!(
        first["items"].as_array().unwrap().len() + last["items"].as_array().unwrap().len(),
        5
    );
    let mut bad = req;
    bad.page_id = Some("wrong-page".into());
    assert!(w.page(&bad).is_err());
}

#[test]
fn media_source_reads_only_frozen_admitted_revision_and_selector() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic, "source-reader");
    let pid = project(&p);
    let first = repo
        .save_multimedia_document(
            &pid,
            &p.editions[0].id,
            None,
            &save(document("改编源"), None, "source-one"),
        )
        .unwrap();
    let source = first.document_source.clone().unwrap();
    assert_ne!(source.digest, first.source_digest);
    let edition = repo
        .create_creative_edition(
            &pid,
            &CreateEditionRequest {
                title: "改编版".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "adapted".into(),
                sources: vec![source.clone()],
            },
            101,
        )
        .unwrap();
    let adapted = CreativeProjectCreated {
        project: p.project.clone(),
        active_edition_id: edition.id.clone(),
        editions: vec![edition],
    };
    let task = media_budget_task(&mut repo, &adapted);
    repo.save_multimedia_document(
        &pid,
        &p.editions[0].id,
        None,
        &save(document("之后的新稿"), first.revision_id, "source-two"),
    )
    .unwrap();
    let pages = repo
        .read_task_media_source(
            &task,
            &source,
            &MediaWorkspacePageRequest {
                section: Some("pages".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(pages["revisionId"], source.revision_id);
    assert_eq!(pages["documentHeader"]["title"], "改编源");
    let mut selected = source.clone();
    selected.selector = MediaSelector::Panel {
        page_id: "page-a".into(),
        panel_id: "panel-a".into(),
        region: None,
    };
    assert_eq!(
        repo.read_task_media_source(&task, &selected, &MediaWorkspacePageRequest::default())
            .unwrap()["panel"]["id"],
        "panel-a"
    );
    let mut forged = source.clone();
    forged.digest = "f".repeat(64);
    assert!(
        repo.read_task_media_source(&task, &forged, &MediaWorkspacePageRequest::default())
            .is_err()
    );
    let current = repo
        .multimedia_workspace(&pid, &p.editions[0].id, None)
        .unwrap()
        .document_source
        .unwrap();
    assert!(
        repo.read_task_media_source(&task, &current, &MediaWorkspacePageRequest::default())
            .is_err()
    );
}

#[test]
fn transcriptions_are_typed_durable_results_with_unicode_paging() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Animation, "transcription");
    let pid = project(&p);
    let e = &p.editions[0];
    repo.begin_media_import(
        &pid,
        &e.id,
        &BeginMediaImport {
            operation_id: "voice-import".into(),
            idempotency_key: "voice-pin".into(),
            name: "voice.wav".into(),
        },
        100,
    )
    .unwrap();
    let audio = repo
        .register_media_asset(
            &pid,
            &e.id,
            &RegisterMediaAsset {
                operation_id: "voice-import".into(),
                idempotency_key: "voice-asset".into(),
                asset: MediaAssetInput {
                    name: "voice.wav".into(),
                    kind: MediaAssetKind::Audio,
                    mime_type: "audio/wav".into(),
                    digest: "a".repeat(64),
                    bytes: 48044,
                    width: None,
                    height: None,
                    duration: Some(MediaTime {
                        value: 48000,
                        rate: Rational {
                            numerator: 48000,
                            denominator: 1,
                        },
                    }),
                    sample_rate: Some(48000),
                    channels: Some(1),
                },
                sources: vec![],
            },
            100,
        )
        .unwrap();
    let task = media_budget_task(&mut repo, &p);
    let intent = repo
        .prepare_media_operation(
            &task,
            &PrepareMediaOperation {
                provider_id: "configured-audio".into(),
                kind: "audio.transcribe".into(),
                parameters: json!({"audioAssetRevisionId":audio.asset_revision_id}),
                idempotency_key: "transcribe".into(),
            },
            100000,
        )
        .unwrap();
    let op = intent["operation"]["id"].as_str().unwrap();
    assert_eq!(
        intent["operation"]["reservation"]["units"]["audioMilliseconds"],
        1000
    );
    repo.begin_media_operation_outputs(&pid, op, 1, 100000)
        .unwrap();
    let stored = repo
        .store_media_transcript(
            &pid,
            op,
            &MediaTranscriptInput {
                text: "甲乙😀丁戊".into(),
                source_asset_revision_id: audio.asset_revision_id,
            },
            101000,
        )
        .unwrap();
    assert!(repo.media_transcript_page(&pid, op, 0, 2).is_err());
    repo.seal_media_operation(
        &pid,
        op,
        &MediaOperationSeal {
            state: "ready".into(),
            outputs: Some(MediaOperationOutputs {
                asset_revision_ids: vec![],
                artifact_revision_id: Some(stored["artifactRevisionId"].as_str().unwrap().into()),
            }),
            provider_receipt: Some(json!({"id":"transcript-receipt"})),
            error: None,
        },
        102000,
    )
    .unwrap();
    let page = repo.media_transcript_page(&pid, op, 2, 2).unwrap();
    assert_eq!(page["text"], "😀丁");
    assert_eq!(page["nextOffset"], 4);
    assert_eq!(page["totalChars"], 5);
}

#[test]
fn media_interlude_keeps_original_work_and_forbids_document_and_provider_writes() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let created = create(&mut repo, CreativeMedium::Comic, "readonly-interlude");
    let p = project(&created);
    let e = &created.editions[0];
    let thread = "media-question";
    repo.conversation_store()
        .create_conversation(
            &p,
            CreateConversationRequest {
                thread_id: thread.into(),
                scope_id: Some(e.scope_id.clone()),
                title: "漫画问题".into(),
                forked_from_id: None,
                last_turn_id: None,
            },
            100,
        )
        .unwrap();
    let w = repo.multimedia_workspace(&p, &e.id, None).unwrap();
    let request = |id: &str, target: serde_json::Value, text: &str| CreateTaskRequest {
        task_id: id.into(),
        project_id: p.clone(),
        thread_id: thread.into(),
        operation: "advisory".into(),
        input_revision: w.revision,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: json!({"authorIntent":text}).to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 4_000_000_000,
    };
    let root=repo.create_budgeted_root_task(request("root-media",json!({"type":"edition","editionId":e.id,"scopeId":e.scope_id,"branchId":e.branch_id,"revisionId":w.revision_id}),"原漫画任务"),&InitialWorkBudget{authorization_id:"media-control-auth".into(),author_message_id:"media-control-author".into(),expected_base_snapshot_id:w.head_snapshot_id.clone(),expected_source_digest:w.source_digest.clone(),max_requests:Some(10),max_tokens:Some(1000),deadline_at:None},100).unwrap();
    let work = repo.work_task_for_attempt(&p, &root.task_id).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let begin:BeginWorkInterlude=serde_json::from_value(json!({"id":id,"threadId":thread,"expectedControlRevision":0,"parentId":null,"autoReturn":false,"message":{"messageId":id,"sessionId":"host","trigger":{"kind":"interlude","interludeId":id,"workTaskId":work.id},"input":{"text":"只解释画格","focus":null,"connectionId":null,"media":{"scope":{"projectId":p.as_str(),"editionId":e.id,"scopeId":e.scope_id,"branchId":e.branch_id},"expectedRevisionId":w.revision_id,"selection":null,"imageAssetRevisionIds":[]}}}})).unwrap();
    let held = repo.begin_work_interlude(&p, &work.id, begin, 101).unwrap();
    assert_eq!(held.goal, work.goal);
    let step = held
        .steps
        .iter()
        .find(|step| step.id == held.interlude.as_ref().unwrap().step_id)
        .unwrap();
    assert_eq!(step.target["readOnly"], true);
    assert_eq!(step.target["scopeId"], e.scope_id);
    let mut edited = repo
        .conversation_store()
        .conversation_message(&p, thread, &id)
        .unwrap()["input"]
        .clone();
    edited["text"] = json!("只解释画格（二）");
    repo.conversation_store()
        .change_conversation_message(
            &p,
            thread,
            &id,
            serde_json::from_value(json!({"action":"edit","expectedRevision":0,"input":edited}))
                .unwrap(),
        )
        .unwrap();
    let mut wrong = edited.clone();
    wrong["media"]["scope"]["scopeId"] = json!("another-scope");
    assert!(
        repo.conversation_store()
            .change_conversation_message(
                &p,
                thread,
                &id,
                serde_json::from_value(json!({"action":"edit","expectedRevision":1,"input":wrong}))
                    .unwrap()
            )
            .is_err()
    );
    repo.complete_task(&p, &root.task_id, thread, "原步骤结束".into(), None, 102)
        .unwrap();
    repo.charge_work_provider_usage(&p,&work.id,&root.task_id,&json!({"runId":root.task_id,"budgetId":format!("budget-{}",root.task_id),"status":"completed","limits":{"maxRequests":10,"maxTokens":1000,"maxDurationMs":null},"usage":{"requests":1,"tokens":25,"unknownTokens":0,"overrun":false}})).unwrap();
    let message = repo
        .conversation_store()
        .change_conversation_message(
            &p,
            thread,
            &id,
            serde_json::from_value(
                json!({"action":"claim","expectedRevision":1,"sessionId":"host"}),
            )
            .unwrap(),
        )
        .unwrap();
    let mut target = step.target.clone();
    target["resumeStepId"] = json!(step.id);
    target["messageDispatch"] =
        json!({"messageId":id,"sessionId":"host","revision":message["revision"]});
    let question = repo
        .create_funded_task(
            request("media-interlude-attempt", target, "只解释画格（二）"),
            103,
        )
        .unwrap();
    assert_eq!(
        repo.work_task_for_attempt(&p, &question.task_id)
            .unwrap()
            .id,
        work.id
    );
    assert_eq!(
        repo.task_provider_budget(&p, &question.task_id, 103)
            .unwrap()
            .max_tokens,
        Some(975)
    );
    assert!(repo.media_task_workspace(&question).is_ok());
    let action = TaskActionRequest {
        id: "readonly-action".into(),
        source_task_id: question.task_id.clone(),
        operation: "generateBrief".into(),
        target: json!({"type":"artifact"}),
        instruction: "扩展修改".into(),
        input_revision: w.revision,
        intent_revision: 0,
        status: "proposed".into(),
        result_task_id: None,
        error: None,
    };
    assert!(matches!(
        repo.record_task_action(&p, &question.task_id, "readonly-candidate", action),
        Err(RepositoryError::TaskOutputForbidden)
    ));
    let plan:UpdateModelWorkPlan=serde_json::from_value(json!({"expectedPlanVersion":0,"idempotencyKey":"readonly-plan","plan":{"intentMode":"creation","goals":[]}})).unwrap();
    assert!(matches!(
        repo.update_model_work_plan(&question, &plan),
        Err(RepositoryError::TaskOutputForbidden)
    ));
    assert!(matches!(
        repo.propose_generation_handoff(
            &question,
            &GenerationHandoffRequest {
                target_chapters: 1,
                resume_run: None,
                instruction: "扩大到新小说".into(),
                expected_intent_revision: 0,
                idempotency_key: "readonly-no-handoff".into()
            }
        ),
        Err(RepositoryError::TaskOutputForbidden)
    ));
    assert!(matches!(
        repo.save_task_multimedia_document(
            &question,
            &save(document("禁止写入"), None, "interlude-save")
        ),
        Err(RepositoryError::TaskNotWritable)
    ));
    assert!(matches!(
        repo.prepare_media_operation(
            &question,
            &PrepareMediaOperation {
                provider_id: "fixture".into(),
                kind: "image.generate".into(),
                parameters: json!({}),
                idempotency_key: "interlude-generation".into()
            },
            103000
        ),
        Err(RepositoryError::TaskNotWritable)
    ));
    repo.complete_task(
        &p,
        &question.task_id,
        thread,
        "只读问题已回答".into(),
        None,
        104,
    )
    .unwrap();
    repo.charge_work_provider_usage(&p,&work.id,&question.task_id,&json!({"runId":question.task_id,"budgetId":format!("budget-{}",question.task_id),"status":"completed","limits":{"maxRequests":9,"maxTokens":975,"maxDurationMs":null},"usage":{"requests":1,"tokens":25,"unknownTokens":0,"overrun":false}})).unwrap();
    let answered = repo.work_task(&p, &work.id).unwrap();
    assert_eq!(answered.interlude.as_ref().unwrap().phase, "answered");
    let returned = repo
        .return_work_interlude(
            &p,
            &work.id,
            ReturnWorkInterlude {
                command_id: uuid::Uuid::new_v4().to_string(),
                interlude_id: id,
                thread_id: thread.into(),
                expected_control_revision: 0,
                expected_revision: 0,
                automatic: false,
            },
        )
        .unwrap();
    assert!(returned.interlude.is_none());
    assert_eq!(returned.goal, work.goal);
    assert_eq!(
        repo.multimedia_workspace(&p, &e.id, None)
            .unwrap()
            .revision_id,
        w.revision_id
    );
}

use novel_codex_story_core::{GenerationHandoffRepository, InitialWorkBudgetRepository, ModelWorkPlanRepository, MultimediaOperationsRepository, MultimediaRepository, MultimediaSourcesRepository, OrdinaryWorkBudgetRepository, ProjectRevisionRepository, TaskActionsRepository, TaskInputRepository, TaskProviderBudgetRepository, TasksRepository, WorkInterludesRepository, WorkTasksRepository};
