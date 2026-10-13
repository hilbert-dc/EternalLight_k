use novel_codex_story_core::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn create(repo: &mut SqliteStoryRepository, medium: CreativeMedium) -> CreativeProjectCreated {
    repo.create_creative_project(
        &CreateCreativeProjectRequest {
            title: "局部创作".into(),
            logline: "".into(),
            medium,
            idempotency_key: "project".into(),
        },
        100,
    )
    .unwrap()
}
fn pid(p: &CreativeProjectCreated) -> ProjectId {
    ProjectId::parse(p.project.id.clone()).unwrap()
}
fn comic() -> MultimediaDocument {
    serde_json::from_value(json!({"kind":"comic","title":"漫画","format":"pages","pages":[{"id":"page-a","title":"一","width":800,"height":1200,"readingDirection":"rightToLeft","panels":[{"id":"panel-a","description":"雨","rect":{"x":0,"y":0,"width":800,"height":600},"assetRevisionId":null,"referenceAssetRevisionIds":[],"locked":false,"balloons":[{"id":"balloon-a","text":"保留对白","speaker":"甲","rect":{"x":1,"y":1,"width":50,"height":30},"kind":"speech"}]}]},{"id":"page-b","title":"未读页","width":800,"height":1200,"readingDirection":"rightToLeft","panels":[]}]})).unwrap()
}
fn save(
    repo: &mut SqliteStoryRepository,
    p: &CreativeProjectCreated,
    document: MultimediaDocument,
) -> MultimediaWorkspace {
    repo.save_multimedia_document(
        &pid(p),
        &p.active_edition_id,
        None,
        &SaveMultimediaDocument {
            expected_revision_id: None,
            idempotency_key: "initial".into(),
            document,
            read_set: vec![],
            sources: vec![],
        },
    )
    .unwrap()
}
fn patch(revision: Option<String>, key: &str, operations: Value) -> PatchMultimediaDocument {
    serde_json::from_value(json!({"expectedRevisionId":revision,"idempotencyKey":key,"operations":operations,"readSet":[],"sources":[]})).unwrap()
}
fn panel_metadata(description: &str) -> Value {
    json!({"kind":"upsertPanel","pageId":"page-a","panel":{"id":"panel-a","description":description,"rect":{"x":0,"y":0,"width":800,"height":600},"assetRevisionId":null,"referenceAssetRevisionIds":[]}})
}
fn task(
    repo: &mut SqliteStoryRepository,
    p: &CreativeProjectCreated,
    w: &MultimediaWorkspace,
) -> TaskRecord {
    repo.conversation_store().create_conversation(
        &pid(p),
        CreateConversationRequest {
            scope_id: Some(w.edition.scope_id.clone()),
            thread_id: "thread".into(),
            title: "局部修改".into(),
            forked_from_id: None,
            last_turn_id: None,
        },
        100,
    )
    .unwrap();
    repo.create_task(CreateTaskRequest { task_id: "task".into(), project_id: pid(p), thread_id: "thread".into(), operation: "advisory".into(), input_revision: w.revision, model: None, target_json: json!({"type":"edition","editionId":w.edition.id,"scopeId":w.edition.scope_id,"branchId":w.edition.branch_id,"revisionId":w.revision_id}).to_string(), input_manifest_json: json!({"authorIntent":"调整画格"}).to_string(), capability_hash: "a".repeat(64), capability_expires_at:4_000_000_000 }).unwrap()
}

#[test]
fn patch_keeps_unread_children_and_exact_saved_encoding_with_stable_replay() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let first = save(&mut repo, &p, comic());
    let request = patch(
        first.revision_id.clone(),
        "local",
        json!([panel_metadata("晴"),{"kind":"upsertPage","page":{"id":"page-a","title":"新的标题","width":800,"height":1200,"readingDirection":"rightToLeft"}}]),
    );
    let saved = repo
        .patch_multimedia_document(&pid(&p), &p.active_edition_id, None, &request)
        .unwrap();
    let a = serde_json::to_value(&first.document).unwrap();
    let b = serde_json::to_value(&saved.document).unwrap();
    assert_eq!(
        a["pages"][0]["panels"][0]["balloons"],
        b["pages"][0]["panels"][0]["balloons"]
    );
    assert_eq!(a["pages"][1], b["pages"][1]);
    assert_eq!(b["pages"][0]["panels"][0]["description"], "晴");
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(saved.document_encoding.as_ref().unwrap().as_bytes())
        ),
        saved.document_source.as_ref().unwrap().digest
    );
    repo.patch_multimedia_document(
        &pid(&p),
        &p.active_edition_id,
        None,
        &patch(
            saved.revision_id.clone(),
            "another",
            json!([{"kind":"setTitle","title":"再次修改"}]),
        ),
    )
    .unwrap();
    assert_eq!(
        repo.patch_multimedia_document(&pid(&p), &p.active_edition_id, None, &request)
            .unwrap()
            .revision_id,
        saved.revision_id
    );
    let mut conflicting = request;
    conflicting
        .operations
        .push(MediaDocumentOperation::SetTitle {
            title: "换了内容".into(),
        });
    assert!(matches!(
        repo.patch_multimedia_document(&pid(&p), &p.active_edition_id, None, &conflicting),
        Err(RepositoryError::IdempotencyConflict)
    ));
}

#[test]
fn entire_failed_batch_leaves_no_revision_snapshot_receipt_or_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atomic.sqlite");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let first = save(&mut repo, &p, comic());
    let db = rusqlite::Connection::open(&path).unwrap();
    let counts = || {
        [
            "story_revisions_v2",
            "story_snapshots_v2",
            "story_domain_receipts_v2",
            "story_task_outputs_v2",
        ]
        .map(|table| {
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
        })
    };
    let before = counts();
    let request = patch(
        first.revision_id.clone(),
        "atomic",
        json!([{"kind":"setTitle","title":"不可写入"},{"kind":"removePanel","pageId":"page-a","panelId":"不存在"}]),
    );
    assert!(
        repo.patch_multimedia_document(&pid(&p), &p.active_edition_id, None, &request)
            .is_err()
    );
    assert_eq!(counts(), before);
    assert_eq!(
        repo.multimedia_workspace(&pid(&p), &p.active_edition_id, None)
            .unwrap()
            .revision_id,
        first.revision_id
    );
    let invalid = patch(
        first.revision_id.clone(),
        "invalid-region",
        json!([{"kind":"upsertPage","page":{"id":"page-a","title":"新","width":1,"height":1,"readingDirection":"leftToRight"}}]),
    );
    assert!(
        repo.patch_multimedia_document(&pid(&p), &p.active_edition_id, None, &invalid)
            .is_err()
    );
    assert_eq!(counts(), before);
}

#[test]
fn concurrent_writers_have_one_cas_winner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("race.sqlite");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let first = save(&mut repo, &p, comic());
    drop(repo);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads = (0..2)
        .map(|i| {
            let path = path.clone();
            let barrier = barrier.clone();
            let p = p.clone();
            let revision = first.revision_id.clone();
            std::thread::spawn(move || {
                let mut repo = SqliteStoryRepository::open(&path).unwrap();
                barrier.wait();
                repo.patch_multimedia_document(
                    &pid(&p),
                    &p.active_edition_id,
                    None,
                    &patch(
                        revision,
                        &format!("race-{i}"),
                        json!([{"kind":"setTitle","title":format!("稿{i}")}]),
                    ),
                )
            })
        })
        .collect::<Vec<_>>();
    let results = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results.iter().any(
        |r| matches!(r,Err(RepositoryError::InvalidContent(code)) if code=="media_revision_changed")
    ));
}

#[test]
fn task_preserves_author_locks_and_external_head_changes() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let mut doc = comic();
    if let MultimediaDocument::Comic { pages, .. } = &mut doc {
        pages[0].panels[0].locked = true;
    }
    let first = save(&mut repo, &p, doc);
    let task = task(&mut repo, &p, &first);
    for (key, operations) in [
        ("edit", json!([panel_metadata("更改锁内内容")])),
        (
            "unlock",
            json!([{"kind":"setPanelLock","pageId":"page-a","panelId":"panel-a","locked":false}]),
        ),
        ("remove", json!([{"kind":"removePage","pageId":"page-a"}])),
        (
            "balloon",
            json!([{"kind":"removeBalloon","pageId":"page-a","panelId":"panel-a","balloonId":"balloon-a"}]),
        ),
    ] {
        let result = repo.patch_task_multimedia_document(
            &task,
            &patch(first.revision_id.clone(), key, operations),
        );
        assert!(matches!(
            result,
            Err(RepositoryError::ObjectLocked) | Err(RepositoryError::TaskOutputForbidden)
        ));
        assert_eq!(
            repo.media_task_working_workspace(&task)
                .unwrap()
                .revision_id,
            first.revision_id
        );
    }
    let unlocked=repo.patch_multimedia_document(&pid(&p),&p.active_edition_id,None,&patch(first.revision_id.clone(),"author-unlock",json!([{"kind":"setPanelLock","pageId":"page-a","panelId":"panel-a","locked":false}]))).unwrap();
    assert!(
        repo.patch_task_multimedia_document(
            &task,
            &patch(
                unlocked.revision_id,
                "old-task",
                json!([panel_metadata("旧任务误写")])
            )
        )
        .is_err()
    );
}

#[test]
fn sources_and_read_dependencies_survive_local_changes_and_require_exact_removal() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let source = save(&mut repo, &p, comic());
    let target = repo
        .create_creative_edition(
            &pid(&p),
            &CreateEditionRequest {
                title: "改编".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "target".into(),
                sources: vec![],
            },
            101,
        )
        .unwrap();
    let source_ref = MediaSourceRef {
        selector: MediaSelector::Panel {
            page_id: "page-a".into(),
            panel_id: "panel-a".into(),
            region: None,
        },
        ..source.document_source.clone().unwrap()
    };
    let link = MediaSourceLink {
        purpose: Some(MediaSourcePurpose::Visual), decision: Some("沿用造型，重新安排叙事顺序".into()),
        relation: MediaRelation::Adapts,
        source: source_ref.clone(),
        target_object_id: target.id.clone(),
        target_selector: MediaSelector::Panel {
            page_id: "page-a".into(),
            panel_id: "panel-a".into(),
            region: None,
        },
    };
    let original = repo
        .save_multimedia_document(
            &pid(&p),
            &target.id,
            None,
            &SaveMultimediaDocument {
                expected_revision_id: None,
                document: comic(),
                idempotency_key: "adapt".into(),
                read_set: vec![MediaReadDependency {
                    source: source_ref.clone(),
                    freshness: MediaFreshness::MustBeCurrent,
                }],
                sources: vec![link.clone()],
            },
        )
        .unwrap();
    let next = repo
        .patch_multimedia_document(
            &pid(&p),
            &target.id,
            None,
            &patch(
                original.revision_id.clone(),
                "local",
                json!([{"kind":"setTitle","title":"只改题目"}]),
            ),
        )
        .unwrap();
    assert_eq!(next.sources, vec![link.clone()]);
    assert!(
        repo.patch_multimedia_document(
            &pid(&p),
            &target.id,
            None,
            &patch(
                next.revision_id.clone(),
                "dangling",
                json!([{"kind":"removePage","pageId":"page-a"}])
            )
        )
        .is_err()
    );
    let removed=repo.patch_multimedia_document(&pid(&p),&target.id,None,&patch(next.revision_id.clone(),"remove-linked",json!([{"kind":"removeSourceLink","link":link},{"kind":"removePage","pageId":"page-a"}]))).unwrap();
    assert!(removed.sources.is_empty());
    repo.patch_multimedia_document(
        &pid(&p),
        &p.active_edition_id,
        None,
        &patch(
            source.revision_id,
            "change-source",
            json!([panel_metadata("来源变化")]),
        ),
    )
    .unwrap();
    assert!(
        matches!(repo.patch_multimedia_document(&pid(&p),&target.id,None,&patch(removed.revision_id,"stale-read",json!([{"kind":"setTitle","title":"无效更新"}]))),Err(RepositoryError::InvalidContent(code)) if code=="media_source_changed")
    );
}

#[test]
fn task_cannot_bind_ungranted_source_or_new_asset_even_in_same_edition() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let first = save(&mut repo, &p, comic());
    let task = task(&mut repo, &p, &first);
    let other = repo
        .create_creative_edition(
            &pid(&p),
            &CreateEditionRequest {
                title: "未授权来源".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "other".into(),
                sources: vec![],
            },
            101,
        )
        .unwrap();
    let source = repo
        .save_multimedia_document(
            &pid(&p),
            &other.id,
            None,
            &SaveMultimediaDocument {
                expected_revision_id: None,
                document: comic(),
                idempotency_key: "other-doc".into(),
                read_set: vec![],
                sources: vec![],
            },
        )
        .unwrap();
    let mut request = patch(
        first.revision_id.clone(),
        "forbidden-source",
        json!([{"kind":"setTitle","title":"范围越界"}]),
    );
    request.read_set.push(MediaReadDependency {
        source: source.document_source.unwrap(),
        freshness: MediaFreshness::Pinned,
    });
    assert!(matches!(
        repo.patch_task_multimedia_document(&task, &request),
        Err(RepositoryError::TaskOutputForbidden)
    ));
    repo.begin_media_import(
        &pid(&p),
        &p.active_edition_id,
        &BeginMediaImport {
            operation_id: "import".into(),
            idempotency_key: "begin".into(),
            name: "图.png".into(),
        },
        102,
    )
    .unwrap();
    let asset = repo
        .register_media_asset(
            &pid(&p),
            &p.active_edition_id,
            &RegisterMediaAsset {
                operation_id: "import".into(),
                idempotency_key: "register".into(),
                asset: MediaAssetInput {
                    name: "图.png".into(),
                    kind: MediaAssetKind::Image,
                    mime_type: "image/png".into(),
                    digest: "a".repeat(64),
                    bytes: 60,
                    width: Some(32),
                    height: Some(32),
                    duration: None,
                    sample_rate: None,
                    channels: None,
                },
                sources: vec![],
            },
            103,
        )
        .unwrap();
    let mut operation = panel_metadata("引用新素材");
    operation["panel"]["assetRevisionId"] = json!(asset.asset_revision_id);
    assert!(matches!(
        repo.patch_task_multimedia_document(
            &task,
            &patch(first.revision_id, "late-asset", json!([operation]))
        ),
        Err(RepositoryError::TaskOutputForbidden)
    ));
}

#[test]
fn animation_node_updates_keep_clips_and_dangling_shot_removal_is_atomic() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Animation);
    let doc=serde_json::from_value(json!({"kind":"animation","title":"动画","width":1280,"height":720,"frameRate":{"numerator":24,"denominator":1},"shots":[{"id":"shot-a","title":"镜头","description":"雨","assetRevisionId":null,"referenceAssetRevisionIds":[],"locked":false}],"timeline":{"tracks":[{"id":"track-a","title":"视频","kind":"video","muted":false,"clips":[{"id":"clip-a","shotId":"shot-a","assetRevisionId":null,"start":{"value":0,"rate":{"numerator":24,"denominator":1}},"duration":{"value":48,"rate":{"numerator":24,"denominator":1}},"sourceStart":{"value":0,"rate":{"numerator":24,"denominator":1}},"playbackRate":{"numerator":1,"denominator":1},"caption":"旁白"}]}]}})).unwrap();
    let first = save(&mut repo, &p, doc);
    let saved=repo.patch_multimedia_document(&pid(&p),&p.active_edition_id,None,&patch(first.revision_id.clone(),"metadata",json!([{"kind":"upsertTrack","track":{"id":"track-a","title":"改名","kind":"video","muted":true}},{"kind":"upsertShot","shot":{"id":"shot-a","title":"镜头新名","description":"晴","assetRevisionId":null,"referenceAssetRevisionIds":[]}}]))).unwrap();
    assert_eq!(
        serde_json::to_value(&saved.document).unwrap()["timeline"]["tracks"][0]["clips"],
        serde_json::to_value(&first.document).unwrap()["timeline"]["tracks"][0]["clips"]
    );
    assert!(
        repo.patch_multimedia_document(
            &pid(&p),
            &p.active_edition_id,
            None,
            &patch(
                saved.revision_id.clone(),
                "dangling-shot",
                json!([{"kind":"removeShot","shotId":"shot-a"}])
            )
        )
        .is_err()
    );
    let valid=repo.patch_multimedia_document(&pid(&p),&p.active_edition_id,None,&patch(saved.revision_id,"remove-both",json!([{"kind":"removeClip","trackId":"track-a","clipId":"clip-a"},{"kind":"removeShot","shotId":"shot-a"}]))).unwrap();
    assert_eq!(
        serde_json::to_value(valid.document).unwrap()["shots"],
        json!([])
    );
}

#[test]
fn patch_order_is_explicit_and_unbounded_or_untyped_payloads_reject() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let first = save(&mut repo, &p, comic());
    let moved = repo
        .patch_multimedia_document(
            &pid(&p),
            &p.active_edition_id,
            None,
            &patch(
                first.revision_id.clone(),
                "move",
                json!([{"kind":"movePage","pageId":"page-b","beforeId":"page-a"}]),
            ),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&moved.document).unwrap()["pages"][0]["id"],
        "page-b"
    );
    for (key, ops) in [
        ("empty", json!([])),
        (
            "too-many",
            json!(
                (0..101)
                    .map(|_| json!({"kind":"setTitle","title":"标题"}))
                    .collect::<Vec<_>>()
            ),
        ),
        (
            "too-big",
            json!(
                (0..100)
                    .map(|_| json!({"kind":"setTitle","title":"x".repeat(3000)}))
                    .collect::<Vec<_>>()
            ),
        ),
        (
            "move-self",
            json!([{"kind":"movePage","pageId":"page-b","beforeId":"page-b"}]),
        ),
    ] {
        assert!(
            repo.patch_multimedia_document(
                &pid(&p),
                &p.active_edition_id,
                None,
                &patch(moved.revision_id.clone(), key, ops)
            )
            .is_err()
        );
    }
    assert!(serde_json::from_value::<PatchMultimediaDocument>(json!({"expectedRevisionId":null,"idempotencyKey":"bad-shape","readSet":[],"sources":[],"operations":[{"kind":"upsertPage","page":{"id":"p","title":"","width":1,"height":1,"readingDirection":"vertical","panels":[]}}]})).is_err());
}

#[test]
fn canon_adoption_is_revision_owned_author_only_and_keeps_historical_task_pins() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let first = save(&mut repo, &p, comic());
    let old_task = task(&mut repo, &p, &first);
    let empty_pin = first.edition.canon_snapshot_id.clone();
    let canon_request = SaveCanonDocument {
        expected_revision_id: None,
        idempotency_key: "canon-1".into(),
        document: CanonDocument {
            title: "规则".into(),
            entries: vec![CanonEntry {
                id: "rule-a".into(),
                category: CanonCategory::Rule,
                statement: "灯不能熄灭".into(),
                sources: vec![],
            }],
        },
    };
    let canon = repo.save_canon_document(&pid(&p), &canon_request).unwrap();
    let adoption = patch(
        first.revision_id.clone(),
        "adopt",
        json!([{"kind":"adoptCanonSnapshot","canonSnapshotId":canon.snapshot_id}]),
    );
    assert!(matches!(
        repo.patch_task_multimedia_document(&old_task, &adoption),
        Err(RepositoryError::TaskOutputForbidden)
    ));
    let adopted = repo
        .patch_multimedia_document(&pid(&p), &p.active_edition_id, None, &adoption)
        .unwrap();
    assert_ne!(adopted.revision_id, first.revision_id);
    assert_eq!(adopted.document_encoding, first.document_encoding);
    assert_eq!(
        adopted.edition.canon_snapshot_id.as_deref(),
        Some(canon.snapshot_id.as_str())
    );
    assert_eq!(
        repo.patch_multimedia_document(&pid(&p), &p.active_edition_id, None, &adoption)
            .unwrap()
            .revision_id,
        adopted.revision_id
    );
    let fixed = repo
        .multimedia_revision_workspace(
            &pid(&p),
            &p.active_edition_id,
            None,
            first.revision_id.as_ref().unwrap(),
        )
        .unwrap();
    assert_eq!(fixed.edition.canon_snapshot_id, empty_pin);
    assert_eq!(
        repo.media_task_workspace(&old_task)
            .unwrap()
            .edition
            .canon_snapshot_id,
        empty_pin
    );
    assert_eq!(repo.media_task_canon(&old_task, 0, 20).unwrap()["total"], 0);
    let new_task=repo.create_task(CreateTaskRequest{task_id:"new-task".into(),project_id:pid(&p),thread_id:"thread".into(),operation:"advisory".into(),input_revision:adopted.revision,model:None,target_json:json!({"type":"edition","editionId":adopted.edition.id,"scopeId":adopted.edition.scope_id,"branchId":adopted.edition.branch_id,"revisionId":adopted.revision_id}).to_string(),input_manifest_json:json!({"authorIntent":"依据已采用规则修改"}).to_string(),capability_hash:"b".repeat(64),capability_expires_at:4_000_000_000}).unwrap();
    let changed = repo
        .save_canon_document(
            &pid(&p),
            &SaveCanonDocument {
                expected_revision_id: canon.revision_id.clone(),
                idempotency_key: "canon-2".into(),
                document: CanonDocument {
                    title: "规则".into(),
                    entries: vec![CanonEntry {
                        id: "rule-a".into(),
                        category: CanonCategory::Rule,
                        statement: "灯可以熄灭".into(),
                        sources: vec![],
                    }],
                },
            },
        )
        .unwrap();
    let read = repo.media_task_canon(&new_task, 0, 20).unwrap();
    assert_eq!(read["snapshotId"], canon.snapshot_id);
    assert!(read["items"][0].get("statement").is_none());
    assert_eq!(
        repo.media_task_canon_text(
            &new_task,
            &MediaWorkspacePageRequest {
                section: Some("text".into()),
                text_field: Some(MediaTextField::CanonStatement {
                    entry_id: "rule-a".into()
                }),
                ..Default::default()
            }
        )
        .unwrap()["text"],
        "灯不能熄灭"
    );
    assert_eq!(
        repo.media_task_scope(&new_task)
            .unwrap()
            .canon_snapshot_id
            .as_deref(),
        Some(canon.snapshot_id.as_str())
    );
    assert!(
        repo.read_task_media_source(
            &new_task,
            canon.document_source.as_ref().unwrap(),
            &MediaWorkspacePageRequest::default()
        )
        .is_ok()
    );
    assert!(matches!(
        repo.read_task_media_source(
            &new_task,
            changed.document_source.as_ref().unwrap(),
            &MediaWorkspacePageRequest::default()
        ),
        Err(RepositoryError::TaskOutputForbidden)
    ));
    let mut request = patch(
        adopted.revision_id.clone(),
        "use-fixed-canon",
        json!([{"kind":"setTitle","title":"遵循固定规则"}]),
    );
    request.read_set.push(MediaReadDependency {
        source: canon.document_source.unwrap(),
        freshness: MediaFreshness::Pinned,
    });
    let saved = repo
        .patch_task_multimedia_document(&new_task, &request)
        .unwrap();
    assert_eq!(
        saved.edition.canon_snapshot_id.as_deref(),
        Some(canon.snapshot_id.as_str())
    );
    let branch = repo
        .create_media_branch(
            &pid(&p),
            &p.active_edition_id,
            &CreateMediaBranch {
                source_branch_id: saved.edition.branch_id.clone(),
                title: "平行版本".into(),
                idempotency_key: "branch".into(),
            },
        )
        .unwrap();
    assert_eq!(branch.canon_snapshot_id, saved.edition.canon_snapshot_id);
    let latest = repo
        .patch_multimedia_document(
            &pid(&p),
            &p.active_edition_id,
            None,
            &patch(
                saved.revision_id,
                "adopt-next",
                json!([{"kind":"adoptCanonSnapshot","canonSnapshotId":changed.snapshot_id}]),
            ),
        )
        .unwrap();
    assert_eq!(
        latest.edition.canon_snapshot_id.as_deref(),
        Some(changed.snapshot_id.as_str())
    );
    assert_eq!(
        repo.multimedia_workspace(&pid(&p), &p.active_edition_id, Some(&branch.branch_id))
            .unwrap()
            .edition
            .canon_snapshot_id
            .as_deref(),
        Some(canon.snapshot_id.as_str())
    );
    assert_eq!(
        repo.creative_editions(&pid(&p)).unwrap()[0].canon_snapshot_id,
        latest.edition.canon_snapshot_id
    );
    assert_eq!(
        repo.multimedia_revision_workspace(
            &pid(&p),
            &p.active_edition_id,
            None,
            adopted.revision_id.as_ref().unwrap()
        )
        .unwrap()
        .edition
        .canon_snapshot_id,
        adopted.edition.canon_snapshot_id
    );
}

#[test]
fn canon_adoption_rejects_foreign_snapshots_and_missing_revision_pin_has_no_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pin.sqlite");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let first = save(&mut repo, &p, comic());
    let other = repo
        .create_creative_project(
            &CreateCreativeProjectRequest {
                title: "其他作品".into(),
                logline: "".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "other-project".into(),
            },
            102,
        )
        .unwrap();
    let foreign = repo.canon_workspace(&pid(&other), None).unwrap();
    assert!(repo.patch_multimedia_document(&pid(&p),&p.active_edition_id,None,&patch(first.revision_id.clone(),"foreign",json!([{"kind":"setTitle","title":"不能写入"},{"kind":"adoptCanonSnapshot","canonSnapshotId":foreign.snapshot_id}]))).is_err());
    assert_eq!(
        repo.multimedia_workspace(&pid(&p), &p.active_edition_id, None)
            .unwrap()
            .revision_id,
        first.revision_id
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "DELETE FROM story_media_revision_canon_v1 WHERE revision_id=?1",
        [first.revision_id.unwrap()],
    )
    .unwrap();
    assert!(
        repo.multimedia_workspace(&pid(&p), &p.active_edition_id, None)
            .is_err()
    );
}

#[test]
fn historical_library_pins_exclude_later_imports_and_keep_document_cross_scope_assets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let import = |repo: &mut SqliteStoryRepository, edition: &str, key: &str| {
        repo.begin_media_import(
            &pid(&p),
            edition,
            &BeginMediaImport {
                operation_id: format!("import-{key}"),
                idempotency_key: format!("begin-{key}"),
                name: format!("{key}.png"),
            },
            100,
        )
        .unwrap();
        repo.register_media_asset(
            &pid(&p),
            edition,
            &RegisterMediaAsset {
                operation_id: format!("import-{key}"),
                idempotency_key: format!("register-{key}"),
                asset: MediaAssetInput {
                    name: format!("{key}.png"),
                    kind: MediaAssetKind::Image,
                    mime_type: "image/png".into(),
                    digest: "a".repeat(64),
                    bytes: 60,
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
        .unwrap()
    };
    let asset_a = import(&mut repo, &p.active_edition_id, "a");
    let first = save(&mut repo, &p, comic());
    let frozen_task = task(&mut repo, &p, &first);
    let asset_b = import(&mut repo, &p.active_edition_id, "b");
    assert_eq!(
        repo.multimedia_workspace(&pid(&p), &p.active_edition_id, None)
            .unwrap()
            .assets
            .len(),
        2
    );
    let historic = repo
        .multimedia_revision_workspace(
            &pid(&p),
            &p.active_edition_id,
            None,
            first.revision_id.as_ref().unwrap(),
        )
        .unwrap();
    assert_eq!(historic.assets.len(), 1);
    assert_eq!(
        historic.assets[0].asset_revision_id,
        asset_a.asset_revision_id
    );
    assert_eq!(
        repo.media_task_workspace(&frozen_task)
            .unwrap()
            .assets
            .len(),
        1
    );
    let next = repo
        .patch_multimedia_document(
            &pid(&p),
            &p.active_edition_id,
            None,
            &patch(
                first.revision_id.clone(),
                "new-library",
                json!([{"kind":"setTitle","title":"新库版本"}]),
            ),
        )
        .unwrap();
    assert_eq!(
        repo.multimedia_revision_workspace(
            &pid(&p),
            &p.active_edition_id,
            None,
            next.revision_id.as_ref().unwrap()
        )
        .unwrap()
        .assets
        .len(),
        2
    );
    let other = repo
        .create_creative_edition(
            &pid(&p),
            &CreateEditionRequest {
                title: "引用版".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "other-library".into(),
                sources: vec![],
            },
            102,
        )
        .unwrap();
    let source = MediaSourceRef {
        project_id: p.project.id.clone(),
        scope_id: first.edition.scope_id.clone(),
        branch_id: first.edition.branch_id.clone(),
        object_id: asset_b.id.clone(),
        revision_id: asset_b.asset_revision_id.clone(),
        digest: "a".repeat(64),
        selector: MediaSelector::Whole,
    };
    let mut target = comic();
    if let MultimediaDocument::Comic { pages, .. } = &mut target {
        pages[0].panels[0].asset_revision_id = Some(asset_b.asset_revision_id.clone());
    }
    let target = repo
        .save_multimedia_document(
            &pid(&p),
            &other.id,
            None,
            &SaveMultimediaDocument {
                expected_revision_id: None,
                document: target,
                idempotency_key: "borrowed-library".into(),
                read_set: vec![MediaReadDependency {
                    source,
                    freshness: MediaFreshness::Pinned,
                }],
                sources: vec![],
            },
        )
        .unwrap();
    let fixed = repo
        .multimedia_revision_workspace(
            &pid(&p),
            &other.id,
            None,
            target.revision_id.as_ref().unwrap(),
        )
        .unwrap();
    assert_eq!(fixed.assets.len(), 1);
    assert_eq!(fixed.assets[0].asset_revision_id, asset_b.asset_revision_id);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "DELETE FROM story_media_revision_libraries_v1 WHERE revision_id=?1",
        [first.revision_id.unwrap()],
    )
    .unwrap();
    assert!(
        repo.multimedia_revision_workspace(
            &pid(&p),
            &p.active_edition_id,
            None,
            historic.revision_id.as_ref().unwrap()
        )
        .is_err()
    );
}

#[test]
fn task_source_changes_preserve_baseline_and_do_not_grant_new_source_text() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let origin = save(&mut repo, &p, comic());
    let source = origin.document_source.clone().unwrap();
    let edition = repo.create_creative_edition(&pid(&p), &CreateEditionRequest {
        title: "动画改编".into(), medium: CreativeMedium::Animation,
        idempotency_key: "adaptation".into(), sources: vec![source.clone()],
    }, 100).unwrap();
    let baseline = repo.multimedia_workspace(&pid(&p), &edition.id, None).unwrap();
    let bound = task(&mut repo, &p, &baseline);
    let initial = repo.media_task_source_impacts(&bound, 0, 20).unwrap();
    assert_eq!(initial["items"][0]["status"], "current");
    repo.patch_multimedia_document(&pid(&p), &origin.edition.id, None,
        &patch(origin.revision_id, "upstream", json!([panel_metadata("上游的新内容，不应通过变化提示泄漏")]))).unwrap();
    let changed = repo.media_task_source_impacts(&bound, 0, 20).unwrap();
    assert_eq!(changed["authority"], "changeCandidatesOnly");
    assert_eq!(changed["items"][0]["status"], "changed");
    assert!(changed["items"][0].get("latest").is_none());
    assert!(!changed.to_string().contains("上游的新内容"));
    assert_eq!(changed["items"][0]["source"]["revisionId"], source.revision_id);
    assert_eq!(repo.media_task_workspace(&bound).unwrap().edition.sources, vec![source]);
    assert_eq!(repo.multimedia_workspace(&pid(&p), &edition.id, None).unwrap().revision_id, baseline.revision_id);
    assert!(repo.media_task_source_impacts(&bound, 0, 0).is_err());
    assert!(repo.media_task_source_impacts(&bound, 2, 20).is_err());
    let mut foreign = bound.clone(); foreign.thread_id = "other-thread".into();
    assert!(repo.media_task_source_impacts(&foreign, 0, 20).is_err());
}

#[test]
fn author_source_replacement_versions_baselines_without_rewriting_or_expanding_old_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source-baselines.sqlite");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let origin = save(&mut repo, &p, comic());
    let previous = origin.document_source.clone().unwrap();
    let adapted = repo.create_creative_edition(&pid(&p), &CreateEditionRequest { title:"改编".into(), medium:CreativeMedium::Comic, idempotency_key:"adapt".into(), sources:vec![previous.clone(),previous.clone()] }, 100).unwrap();
    let baseline = repo.save_multimedia_document(&pid(&p), &adapted.id, None, &SaveMultimediaDocument {
        expected_revision_id:None,idempotency_key:"adapt-save".into(),document:comic(),read_set:vec![MediaReadDependency{source:previous.clone(),freshness:MediaFreshness::MustBeCurrent}],
        sources:vec![MediaSourceLink{source:previous.clone(),target_object_id:adapted.id.clone(),target_selector:MediaSelector::Whole,relation:MediaRelation::Adapts,purpose:None,decision:None}],
    }).unwrap();
    let frozen = task(&mut repo,&p,&baseline);
    let historical_branch=repo.create_media_branch(&pid(&p),&adapted.id,&CreateMediaBranch{source_branch_id:adapted.branch_id.clone(),title:"旧来源分支".into(),idempotency_key:"source-branch".into()}).unwrap();
    let updated = repo.patch_multimedia_document(&pid(&p),&origin.edition.id,None,&patch(origin.revision_id.clone(),"upstream-update",json!([panel_metadata("雪夜的新原作")]))).unwrap();
    let next = updated.document_source.clone().unwrap();
    let request = patch(baseline.revision_id.clone(),"replace-baseline",json!([{"kind":"replaceEditionSource","previous":previous,"next":next}]));
    assert!(repo.patch_task_multimedia_document(&frozen,&request).is_err());
    let mut mixed=request.clone();mixed.idempotency_key="mixed-content".into();
    mixed.operations.push(MediaDocumentOperation::SetTitle{title:"混入内容修改".into()});
    assert!(repo.patch_multimedia_document(&pid(&p),&adapted.id,None,&mixed).is_err());
    let mut invalid = request.clone();
    invalid.idempotency_key="invalid-source".into();
    let mut bad_next=serde_json::to_value(&next).unwrap(); bad_next["digest"]=json!("wrong");
    invalid.operations=serde_json::from_value(json!([{"kind":"replaceEditionSource","previous":previous,"next":bad_next}])).unwrap();
    assert!(repo.patch_multimedia_document(&pid(&p),&adapted.id,None,&invalid).is_err());
    let changed = repo.patch_multimedia_document(&pid(&p),&adapted.id,None,&request).unwrap();
    assert_eq!(changed.edition.sources,vec![next.clone()]);
    assert_eq!(changed.document,baseline.document);
    assert_eq!(changed.sources,baseline.sources);
    assert_eq!(repo.multimedia_workspace(&pid(&p),&adapted.id,Some(&historical_branch.branch_id)).unwrap().edition.sources,baseline.edition.sources);
    assert!(repo.patch_task_multimedia_document(&frozen,&patch(changed.revision_id.clone(),"old-task-write",json!([{"kind":"setTitle","title":"越过来源变化"}]))).is_err());
    assert_ne!(changed.revision_id,baseline.revision_id);
    assert_eq!(repo.patch_multimedia_document(&pid(&p),&adapted.id,None,&request).unwrap().revision_id,changed.revision_id);
    let mut stale=request.clone();stale.idempotency_key="stale".into();
    assert!(repo.patch_multimedia_document(&pid(&p),&adapted.id,None,&stale).is_err());
    assert_eq!(repo.media_task_workspace(&frozen).unwrap().edition.sources,baseline.edition.sources);
    assert!(repo.read_task_media_source(&frozen,&next,&MediaWorkspacePageRequest::default()).is_err());
    assert!(repo.read_task_media_source(&frozen,&previous,&MediaWorkspacePageRequest::default()).is_ok());
    assert_eq!(repo.multimedia_revision_workspace(&pid(&p),&adapted.id,None,baseline.revision_id.as_ref().unwrap()).unwrap().edition.sources,baseline.edition.sources);
    drop(repo);
    let mut repo=SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(repo.multimedia_workspace(&pid(&p),&adapted.id,None).unwrap().edition.sources,vec![next.clone()]);
    repo.conversation_store().create_conversation(&pid(&p),CreateConversationRequest{scope_id:Some(adapted.scope_id.clone()),thread_id:"new-thread".into(),title:"新版改编".into(),forked_from_id:None,last_turn_id:None},200).unwrap();
    let new_task=repo.create_task(CreateTaskRequest{task_id:"new-task".into(),project_id:pid(&p),thread_id:"new-thread".into(),operation:"advisory".into(),input_revision:changed.revision,model:None,target_json:json!({"type":"edition","editionId":adapted.id,"scopeId":adapted.scope_id,"branchId":adapted.branch_id,"revisionId":changed.revision_id}).to_string(),input_manifest_json:json!({"authorIntent":"比较新旧原作，局部更新"}).to_string(),capability_hash:"b".repeat(64),capability_expires_at:4_000_000_000}).unwrap();
    assert!(repo.read_task_media_source(&new_task,&next,&MediaWorkspacePageRequest::default()).is_ok());
    assert!(repo.read_task_media_source(&new_task,&previous,&MediaWorkspacePageRequest::default()).is_ok());
    let impacts=repo.media_task_source_impacts(&new_task,0,20).unwrap();
    assert!(impacts["items"].as_array().unwrap().iter().any(|i|i["source"]["revisionId"]==previous.revision_id && i["status"]=="changed" && i["readFreshness"]=="mustBeCurrent"));
    let mut local=patch(changed.revision_id.clone(),"local-after-compare",json!([panel_metadata("根据新原作修改这一格")]));
    assert!(repo.patch_task_multimedia_document(&new_task,&local).is_err(),"Old mustBeCurrent dependency must still block content edits");
    local.read_set=vec![MediaReadDependency{source:previous.clone(),freshness:MediaFreshness::Pinned},MediaReadDependency{source:next.clone(),freshness:MediaFreshness::MustBeCurrent}];
    let local_saved=repo.patch_task_multimedia_document(&new_task,&local).unwrap();
    let original_doc=serde_json::to_value(&baseline.document).unwrap();
    let local_doc=serde_json::to_value(&local_saved.document).unwrap();
    assert_eq!(local_doc["pages"][1],original_doc["pages"][1]);
    assert_eq!(local_doc["pages"][0]["panels"][0]["balloons"],original_doc["pages"][0]["panels"][0]["balloons"]);
    assert_eq!(local_doc["pages"][0]["panels"][0]["description"],"根据新原作修改这一格");
    // Missing mandatory revision provenance must not fall back to the creation baseline.
    let db=rusqlite::Connection::open(&path).unwrap();
    db.execute("DELETE FROM story_media_revision_sources_v1 WHERE revision_id=?1",[local_saved.revision_id.as_ref().unwrap()]).unwrap();
    assert!(repo.multimedia_workspace(&pid(&p),&adapted.id,None).is_err());
}


#[test]
fn film_production_target_is_versioned_without_claiming_delivery_or_mutating_story() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Animation);
    let document = MultimediaDocument::empty(CreativeMedium::Animation, "电影方案".into()).unwrap();
    let original = save(&mut repo, &p, document);
    let t = task(&mut repo, &p, &original);
    let production = json!({"form":"featureFilm","expression":"hybrid","deliverable":"screenplay","targetDuration":{"value":5400,"rate":{"numerator":1,"denominator":1}}});
    let request = patch(original.revision_id.clone(), "film-target", json!([
        {"kind":"setScreenProduction","production":production},
        {"kind":"setCreativeDesign","text":"雨夜收到密函，林夕决定寻找失踪的父亲。"}
    ]));
    let saved = repo.patch_task_multimedia_document(&t, &request).unwrap();
    let value = serde_json::to_value(&saved.document).unwrap();
    assert_eq!(value["production"], production);
    assert_eq!(value["shots"], json!([]));
    assert_eq!(value["timeline"]["tracks"], json!([]));
    assert!(saved.revision_id.is_some());
    let page = saved.page(&MediaWorkspacePageRequest::default()).unwrap();
    assert_eq!(page["documentHeader"]["production"], production);
    assert_eq!(repo.patch_task_multimedia_document(&t, &request).unwrap().revision_id, saved.revision_id);
    let invalid = patch(saved.revision_id.clone(), "bad-duration", json!([
        {"kind":"setScreenProduction","production":{"form":"featureFilm","expression":"hybrid","deliverable":"finishedVideo","targetDuration":{"value":0,"rate":{"numerator":1,"denominator":1}}}}
    ]));
    assert!(repo.patch_task_multimedia_document(&t, &invalid).is_err());
    assert_eq!(repo.multimedia_workspace(&pid(&p), &p.active_edition_id, None).unwrap().revision_id,saved.revision_id);
}

use novel_codex_story_core::{MultimediaCanonRepository, MultimediaImpactRepository, MultimediaRepository, MultimediaSourcesRepository, TasksRepository};
