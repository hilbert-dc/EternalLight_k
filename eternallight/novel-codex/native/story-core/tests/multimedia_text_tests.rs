use novel_codex_story_core::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
fn hash(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
fn create(repo: &mut SqliteStoryRepository, medium: CreativeMedium) -> CreativeProjectCreated {
    repo.create_creative_project(
        &CreateCreativeProjectRequest {
            title: "精确文本".into(),
            logline: "".into(),
            medium,
            idempotency_key: "project".into(),
        },
        1,
    )
    .unwrap()
}
fn pid(p: &CreativeProjectCreated) -> ProjectId {
    ProjectId::parse(p.project.id.clone()).unwrap()
}
fn comic(text: &str) -> MultimediaDocument {
    serde_json::from_value(json!({"kind":"comic","title":"漫画","format":"pages","pages":[{"id":"page","title":"一","width":800,"height":1000,"readingDirection":"leftToRight","panels":[{"id":"panel","description":text,"rect":{"x":0,"y":0,"width":300,"height":300},"assetRevisionId":null,"referenceAssetRevisionIds":[],"locked":false,"balloons":[{"id":"balloon","text":text,"speaker":"甲","rect":{"x":0,"y":0,"width":40,"height":30},"kind":"speech"}]},{"id":"sibling","description":"兄弟节点保持","rect":{"x":400,"y":0,"width":300,"height":300},"assetRevisionId":null,"referenceAssetRevisionIds":[],"locked":false,"balloons":[]}]}]})).unwrap()
}
fn save(
    repo: &mut SqliteStoryRepository,
    p: &CreativeProjectCreated,
    doc: MultimediaDocument,
) -> MultimediaWorkspace {
    repo.save_multimedia_document(
        &pid(p),
        &p.active_edition_id,
        None,
        &SaveMultimediaDocument {
            expected_revision_id: None,
            idempotency_key: "first".into(),
            document: doc,
            read_set: vec![],
            sources: vec![],
        },
    )
    .unwrap()
}
fn balloon() -> MediaTextField {
    MediaTextField::BalloonText {
        page_id: "page".into(),
        panel_id: "panel".into(),
        balloon_id: "balloon".into(),
    }
}
fn page(field: MediaTextField, offset: usize, limit: usize) -> MediaWorkspacePageRequest {
    MediaWorkspacePageRequest {
        section: Some("text".into()),
        text_field: Some(field),
        char_offset: Some(offset),
        char_limit: Some(limit),
        ..Default::default()
    }
}
fn op(field: &MediaTextField, text: &str, start: usize, end: usize, replacement: &str) -> Value {
    json!({"kind":"replaceTextRanges","field":field,"expectedTextDigest":hash(text),"edits":[{"startChar":start,"endChar":end,"expectedSliceDigest":hash(&text.chars().skip(start).take(end-start).collect::<String>()),"replacement":replacement}]})
}
fn patch(w: &MultimediaWorkspace, key: &str, operations: Value) -> PatchMultimediaDocument {
    serde_json::from_value(json!({"expectedRevisionId":w.revision_id,"idempotencyKey":key,"operations":operations,"readSet":[],"sources":[]})).unwrap()
}
fn task(repo: &mut SqliteStoryRepository, w: &MultimediaWorkspace, key: &str) -> TaskRecord {
    let p = ProjectId::parse(w.project.id.clone()).unwrap();
    repo.conversation_store().create_conversation(
        &p,
        CreateConversationRequest {
            scope_id: Some(w.edition.scope_id.clone()),
            thread_id: format!("thread-{key}"),
            title: "改稿".into(),
            forked_from_id: None,
            last_turn_id: None,
        },
        1,
    )
    .unwrap();
    repo.create_task(CreateTaskRequest{task_id:format!("task-{key}"),project_id:p,thread_id:format!("thread-{key}"),operation:"advisory".into(),input_revision:w.revision,model:None,target_json:json!({"type":"edition","editionId":w.edition.id,"scopeId":w.edition.scope_id,"branchId":w.edition.branch_id,"revisionId":w.revision_id}).to_string(),input_manifest_json:json!({"authorIntent":"精确改稿"}).to_string(),capability_hash:"a".repeat(64),capability_expires_at:4_000_000_000}).unwrap()
}
#[test]
fn unicode_pages_are_complete_byte_bounded_and_fixed_after_head_changes() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let text = "中😀e\u{301}\n\t".repeat(3000);
    let first = save(&mut repo, &p, comic(&text));
    let mut offset = 0;
    let mut rebuilt = String::new();
    let mut pages = 0;
    loop {
        let read = first.page(&page(balloon(), offset, 8000)).unwrap();
        assert!(read.to_string().len() <= 65536);
        assert_eq!(read["unit"], "unicodeScalar");
        assert_eq!(read["textDigest"], hash(&text));
        let chunk = read["text"].as_str().unwrap();
        assert_eq!(read["sliceDigest"], hash(chunk));
        assert_eq!(read["returnedSource"]["selector"]["quote"], chunk);
        assert_eq!(
            read["source"]["revisionId"],
            first.revision_id.as_ref().unwrap().as_str()
        );
        assert_eq!(
            read["endChar"].as_u64().unwrap() as usize - offset,
            chunk.chars().count()
        );
        rebuilt.push_str(chunk);
        pages += 1;
        if read["nextCharOffset"].is_null() {
            break;
        }
        offset = read["nextCharOffset"].as_u64().unwrap() as usize;
    }
    assert!(pages > 2);
    assert_eq!(rebuilt, text);
    let directory = first
        .page(&MediaWorkspacePageRequest {
            section: Some("balloons".into()),
            page_id: Some("page".into()),
            panel_id: Some("panel".into()),
            ..Default::default()
        })
        .unwrap();
    assert!(directory["items"][0].get("text").is_none());
    assert_eq!(directory["items"][0]["textFields"][0]["totalChars"], 18000);
    repo.patch_multimedia_document(
        &pid(&p),
        &p.active_edition_id,
        None,
        &patch(&first, "new", json!([op(&balloon(), &text, 0, 1, "新")])),
    )
    .unwrap();
    let historical = repo
        .multimedia_revision_workspace(
            &pid(&p),
            &p.active_edition_id,
            None,
            first.revision_id.as_ref().unwrap(),
        )
        .unwrap();
    assert_eq!(
        historical.page(&page(balloon(), 0, 1)).unwrap()["text"],
        "中"
    );
    for r in [
        page(balloon(), 18001, 1),
        page(balloon(), 0, 0),
        page(balloon(), 0, 8001),
    ] {
        assert!(first.page(&r).is_err());
    }
    let mut mixed = page(balloon(), 0, 1);
    mixed.offset = Some(0);
    assert!(first.page(&mixed).is_err());
}
#[test]
fn scalar_replacement_preserves_geometry_unread_nodes_and_stable_receipts() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let first = save(&mut repo, &p, comic("甲😀乙e\u{301}尾"));
    let request = patch(
        &first,
        "edit",
        json!([{"kind":"replaceTextRanges","field":balloon(),"expectedTextDigest":hash("甲😀乙e\u{301}尾"),"edits":[{"startChar":1,"endChar":2,"expectedSliceDigest":hash("😀"),"replacement":"晴"},{"startChar":3,"endChar":5,"expectedSliceDigest":hash("e\u{301}"),"replacement":"é"}]}]),
    );
    let saved = repo
        .patch_multimedia_document(&pid(&p), &p.active_edition_id, None, &request)
        .unwrap();
    let before = serde_json::to_value(&first.document).unwrap();
    let after = serde_json::to_value(&saved.document).unwrap();
    assert_eq!(
        after["pages"][0]["panels"][0]["balloons"][0]["text"],
        "甲晴乙é尾"
    );
    assert_eq!(
        before["pages"][0]["panels"][0]["description"],
        after["pages"][0]["panels"][0]["description"]
    );
    assert_eq!(
        before["pages"][0]["panels"][1],
        after["pages"][0]["panels"][1]
    );
    assert_eq!(
        before["pages"][0]["panels"][0]["balloons"][0]["rect"],
        after["pages"][0]["panels"][0]["balloons"][0]["rect"]
    );
    assert_eq!(
        repo.patch_multimedia_document(&pid(&p), &p.active_edition_id, None, &request)
            .unwrap()
            .revision_id,
        saved.revision_id
    );
    let mut stale = request.clone();
    stale.idempotency_key = "stale".into();
    assert!(
        repo.patch_multimedia_document(&pid(&p), &p.active_edition_id, None, &stale)
            .is_err()
    );
    let locked = repo
        .patch_multimedia_document(
            &pid(&p),
            &p.active_edition_id,
            None,
            &patch(
                &saved,
                "lock",
                json!([{"kind":"setPanelLock","pageId":"page","panelId":"panel","locked":true}]),
            ),
        )
        .unwrap();
    let t = task(&mut repo, &locked, "locked");
    assert!(matches!(
        repo.patch_task_multimedia_document(
            &t,
            &patch(
                &locked,
                "blocked",
                json!([op(&balloon(), "甲晴乙é尾", 0, 1, "改")])
            )
        ),
        Err(RepositoryError::ObjectLocked)
    ));
}
#[test]
fn bad_text_batches_never_write_and_range_overlap_or_duplicate_fields_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("text.sqlite");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let first = save(&mut repo, &p, comic("甲😀乙"));
    let db = rusqlite::Connection::open(path).unwrap();
    let counts = || {
        [
            "story_revisions_v2",
            "story_snapshots_v2",
            "story_domain_receipts_v2",
        ]
        .map(|table| {
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
        })
    };
    let before = counts();
    let mut wrong = op(&balloon(), "甲😀乙", 0, 1, "晴");
    wrong["edits"][0]["expectedSliceDigest"] = json!(hash("错"));
    let mut overlap = op(&balloon(), "甲😀乙", 0, 2, "晴");
    overlap["edits"].as_array_mut().unwrap().push(
        json!({"startChar":1,"endChar":3,"expectedSliceDigest":hash("😀乙"),"replacement":"另"}),
    );
    let mut many = op(&balloon(), "甲😀乙", 0, 0, "新");
    many["edits"] = json!(vec![many["edits"][0].clone(); 33]);
    let mut digest_bad = op(&balloon(), "甲😀乙", 0, 1, "新");
    digest_bad["expectedTextDigest"] = json!(hash("错"));
    let mut same_start = op(&balloon(), "甲😀乙", 1, 1, "插入");
    same_start["edits"].as_array_mut().unwrap().push(
        json!({"startChar":1,"endChar":2,"expectedSliceDigest":hash("😀"),"replacement":"另"}),
    );
    let mut reversed = op(&balloon(), "甲😀乙", 0, 0, "新");
    reversed["edits"][0]["startChar"] = json!(3);
    reversed["edits"][0]["endChar"] = json!(1);
    for (i, bad) in [
        wrong,
        overlap,
        many,
        digest_bad,
        same_start,
        reversed,
        op(&balloon(), "甲😀乙", 0, 1, &"大".repeat(20000)),
    ]
    .into_iter()
    .enumerate()
    {
        let request = patch(
            &first,
            &format!("bad-{i}"),
            json!([{"kind":"setTitle","title":"不得写入"},bad]),
        );
        assert!(
            repo.patch_multimedia_document(&pid(&p), &p.active_edition_id, None, &request)
                .is_err()
        );
        assert_eq!(counts(), before);
    }
    assert!(
        repo.patch_multimedia_document(
            &pid(&p),
            &p.active_edition_id,
            None,
            &patch(
                &first,
                "repeat",
                json!([
                    op(&balloon(), "甲😀乙", 0, 1, "一"),
                    op(&balloon(), "甲😀乙", 1, 2, "二")
                ])
            )
        )
        .is_err()
    );
    assert_eq!(counts(), before);
    assert!(
        repo.patch_multimedia_document(
            &pid(&p),
            &p.active_edition_id,
            None,
            &patch(
                &first,
                "canon-forged",
                json!([op(
                    &MediaTextField::CanonStatement {
                        entry_id: "rule".into()
                    },
                    "甲",
                    0,
                    1,
                    "乙"
                )])
            )
        )
        .is_err()
    );
    let inserted = repo
        .patch_multimedia_document(
            &pid(&p),
            &p.active_edition_id,
            None,
            &patch(
                &first,
                "insert",
                json!([op(&balloon(), "甲😀乙", 3, 3, "尾")]),
            ),
        )
        .unwrap();
    assert_eq!(
        inserted.page(&page(balloon(), 0, 10)).unwrap()["text"],
        "甲😀乙尾"
    );
}
#[test]
fn own_saved_text_sources_are_rereadable_but_external_heads_are_not_granted() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let first = save(&mut repo, &p, comic("甲😀乙"));
    let t = task(&mut repo, &first, "own");
    let saved = repo
        .patch_task_multimedia_document(
            &t,
            &patch(
                &first,
                "own-save",
                json!([op(&balloon(), "甲😀乙", 1, 2, "晴")]),
            ),
        )
        .unwrap();
    let read = repo
        .media_task_working_workspace(&t)
        .unwrap()
        .page(&page(balloon(), 1, 1))
        .unwrap();
    let source: MediaSourceRef = serde_json::from_value(read["returnedSource"].clone()).unwrap();
    assert_eq!(
        repo.read_task_media_source(&t, &source, &page(balloon(), 0, 8000))
            .unwrap()["text"],
        "晴"
    );
    assert!(
        repo.read_task_media_source(&t, &source, &MediaWorkspacePageRequest::default())
            .is_err()
    );
    let mut next = patch(
        &saved,
        "source-use",
        json!([op(&balloon(), "甲晴乙", 2, 3, "丙")]),
    );
    next.read_set.push(MediaReadDependency {
        source: source.clone(),
        freshness: MediaFreshness::Pinned,
    });
    repo.patch_task_multimedia_document(&t, &next).unwrap();
    let current = repo
        .multimedia_workspace(&pid(&p), &p.active_edition_id, None)
        .unwrap();
    let external = repo
        .patch_multimedia_document(
            &pid(&p),
            &p.active_edition_id,
            None,
            &patch(
                &current,
                "external",
                json!([{"kind":"setTitle","title":"作者新稿"}]),
            ),
        )
        .unwrap();
    assert!(
        repo.read_task_media_source(
            &t,
            external.document_source.as_ref().unwrap(),
            &page(balloon(), 0, 1)
        )
        .is_err()
    );
}
#[test]
fn cross_edition_field_grants_do_not_expand_to_siblings_or_image_regions() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let first = save(&mut repo, &p, comic("甲😀乙丙丁"));
    let mut source = first.document_source.clone().unwrap();
    source.selector = MediaSelector::FieldText {
        field: balloon(),
        start_char: 1,
        end_char: 3,
        quote: "😀乙".into(),
    };
    let e = repo
        .create_creative_edition(
            &pid(&p),
            &CreateEditionRequest {
                title: "改编".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "adapt".into(),
                sources: vec![source.clone()],
            },
            2,
        )
        .unwrap();
    let w = repo.multimedia_workspace(&pid(&p), &e.id, None).unwrap();
    let t = task(&mut repo, &w, "adapt");
    let read = repo
        .read_task_media_source(&t, &source, &page(balloon(), 0, 8000))
        .unwrap();
    assert_eq!(read["text"], "😀乙");
    assert_eq!(read["totalChars"], 2);
    assert_eq!(read["startChar"], 1);
    let mut expanded = source.clone();
    expanded.selector = MediaSelector::Whole;
    assert!(
        repo.read_task_media_source(&t, &expanded, &page(balloon(), 0, 1))
            .is_err()
    );
    assert!(
        repo.read_task_media_source(
            &t,
            &source,
            &page(
                MediaTextField::PanelDescription {
                    page_id: "page".into(),
                    panel_id: "panel".into()
                },
                0,
                1
            )
        )
        .is_err()
    );
    let mut region = first.document_source.unwrap();
    region.selector = MediaSelector::Panel {
        page_id: "page".into(),
        panel_id: "panel".into(),
        region: Some(MediaRect {
            x: 0.,
            y: 0.,
            width: 10.,
            height: 10.,
        }),
    };
    let e = repo
        .create_creative_edition(
            &pid(&p),
            &CreateEditionRequest {
                title: "图像选区".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "region".into(),
                sources: vec![region.clone()],
            },
            3,
        )
        .unwrap();
    let w = repo.multimedia_workspace(&pid(&p), &e.id, None).unwrap();
    let t = task(&mut repo, &w, "region");
    assert!(
        repo.read_task_media_source(&t, &region, &page(balloon(), 0, 1))
            .is_err()
    );
}
#[test]
fn canon_text_reads_frozen_settings_and_never_grants_model_writes() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let first = repo
        .save_canon_document(
            &pid(&p),
            &SaveCanonDocument {
                expected_revision_id: None,
                idempotency_key: "canon-1".into(),
                document: CanonDocument {
                    title: "共同设定".into(),
                    entries: vec![CanonEntry {
                        id: "rule".into(),
                        category: CanonCategory::Rule,
                        statement: "甲😀规则".into(),
                        sources: vec![],
                    }],
                },
            },
        )
        .unwrap();
    let e = repo
        .create_creative_edition(
            &pid(&p),
            &CreateEditionRequest {
                title: "漫画".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "canon-edition".into(),
                sources: vec![],
            },
            2,
        )
        .unwrap();
    let w = repo.multimedia_workspace(&pid(&p), &e.id, None).unwrap();
    let t = task(&mut repo, &w, "canon");
    repo.save_canon_document(
        &pid(&p),
        &SaveCanonDocument {
            expected_revision_id: first.revision_id.clone(),
            idempotency_key: "canon-2".into(),
            document: CanonDocument {
                title: "共同设定".into(),
                entries: vec![CanonEntry {
                    id: "rule".into(),
                    category: CanonCategory::Rule,
                    statement: "作者新规则".into(),
                    sources: vec![],
                }],
            },
        },
    )
    .unwrap();
    let request = page(
        MediaTextField::CanonStatement {
            entry_id: "rule".into(),
        },
        1,
        1,
    );
    let read = repo.media_task_canon_text(&t, &request).unwrap();
    assert_eq!(read["text"], "😀");
    assert_eq!(read["version"], "source");
    assert_eq!(
        read["revisionId"],
        first.revision_id.as_ref().unwrap().as_str()
    );
    let source: MediaSourceRef = serde_json::from_value(read["returnedSource"].clone()).unwrap();
    assert_eq!(
        repo.read_task_media_source(
            &t,
            &source,
            &page(
                MediaTextField::CanonStatement {
                    entry_id: "rule".into()
                },
                0,
                10
            )
        )
        .unwrap()["text"],
        "😀"
    );
    assert!(
        repo.read_task_media_source(&t, &source, &MediaWorkspacePageRequest::default())
            .is_err()
    );
    assert!(
        repo.patch_task_multimedia_document(
            &t,
            &patch(
                &w,
                "model-canon",
                json!([op(
                    &MediaTextField::CanonStatement {
                        entry_id: "rule".into()
                    },
                    "甲😀规则",
                    0,
                    1,
                    "改"
                )])
            )
        )
        .is_err()
    );
}
#[test]
fn shot_description_and_caption_replace_atomically_preserve_timeline_and_shot_locks() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Animation);
    let doc=serde_json::from_value(json!({"kind":"animation","title":"动画","width":800,"height":600,"frameRate":{"numerator":24,"denominator":1},"shots":[{"id":"shot","title":"镜头","description":"镜头😀","assetRevisionId":null,"referenceAssetRevisionIds":[],"locked":false}],"timeline":{"tracks":[{"id":"track","title":"字幕","kind":"subtitle","muted":false,"clips":[{"id":"clip","shotId":null,"assetRevisionId":null,"start":{"value":0,"rate":{"numerator":24,"denominator":1}},"duration":{"value":48,"rate":{"numerator":24,"denominator":1}},"sourceStart":{"value":0,"rate":{"numerator":24,"denominator":1}},"playbackRate":{"numerator":1,"denominator":1},"caption":"字幕😀"}]}]}})).unwrap();
    let first = save(&mut repo, &p, doc);
    let shot = MediaTextField::ShotDescription {
        shot_id: "shot".into(),
    };
    let caption = MediaTextField::ClipCaption {
        track_id: "track".into(),
        clip_id: "clip".into(),
    };
    assert_eq!(first.page(&page(shot.clone(), 2, 1)).unwrap()["text"], "😀");
    let saved = repo
        .patch_multimedia_document(
            &pid(&p),
            &p.active_edition_id,
            None,
            &patch(
                &first,
                "both",
                json!([
                    op(&shot, "镜头😀", 2, 3, "晴"),
                    op(&caption, "字幕😀", 2, 3, "晴")
                ]),
            ),
        )
        .unwrap();
    assert_eq!(
        saved.page(&page(shot.clone(), 0, 10)).unwrap()["text"],
        "镜头晴"
    );
    assert_eq!(saved.page(&page(caption, 0, 10)).unwrap()["text"], "字幕晴");
    let before = serde_json::to_value(&first.document).unwrap();
    let after = serde_json::to_value(&saved.document).unwrap();
    assert_eq!(
        before["timeline"]["tracks"][0]["clips"][0]["duration"],
        after["timeline"]["tracks"][0]["clips"][0]["duration"]
    );
    let locked = repo
        .patch_multimedia_document(
            &pid(&p),
            &p.active_edition_id,
            None,
            &patch(
                &saved,
                "lock-shot",
                json!([{"kind":"setShotLock","shotId":"shot","locked":true}]),
            ),
        )
        .unwrap();
    let t = task(&mut repo, &locked, "shot");
    assert!(matches!(
        repo.patch_task_multimedia_document(
            &t,
            &patch(
                &locked,
                "locked-shot",
                json!([op(&shot, "镜头晴", 2, 3, "雨")])
            )
        ),
        Err(RepositoryError::ObjectLocked)
    ));
}
#[test]
fn canon_catalogue_and_source_pages_survive_large_quotes_with_an_explicit_current_bound() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let text = "\u{1}".repeat(10000);
    let first = save(&mut repo, &p, comic(&text));
    let refs = (0..10)
        .map(|i| {
            let mut s = first.document_source.clone().unwrap();
            s.selector = MediaSelector::FieldText {
                field: balloon(),
                start_char: i,
                end_char: i + 8000,
                quote: "\u{1}".repeat(8000),
            };
            s
        })
        .collect::<Vec<_>>();
    repo.save_canon_document(
        &pid(&p),
        &SaveCanonDocument {
            expected_revision_id: None,
            idempotency_key: "quoted-canon".into(),
            document: CanonDocument {
                title: "共同设定".into(),
                entries: vec![CanonEntry {
                    id: "rule".into(),
                    category: CanonCategory::Rule,
                    statement: "有来源的事实".into(),
                    sources: refs.clone(),
                }],
            },
        },
    )
    .unwrap();
    let e = repo
        .create_creative_edition(
            &pid(&p),
            &CreateEditionRequest {
                title: "改编".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "from-canon".into(),
                sources: vec![],
            },
            2,
        )
        .unwrap();
    let w = repo.multimedia_workspace(&pid(&p), &e.id, None).unwrap();
    let t = task(&mut repo, &w, "quoted-canon");
    let directory = repo.media_task_canon(&t, 0, 100).unwrap();
    assert!(directory.to_string().len() < 65536);
    assert_eq!(directory["items"][0]["sourceCount"], 10);
    assert!(directory["items"][0].get("sources").is_none());
    let mut offset = 0;
    let mut actual = vec![];
    loop {
        let request = MediaWorkspacePageRequest {
            section: Some("canonSources".into()),
            entry_id: Some("rule".into()),
            offset: Some(offset),
            limit: Some(100),
            ..Default::default()
        };
        let read = repo.media_task_canon_sources(&t, &request).unwrap();
        assert!(read.to_string().len() <= 65536);
        actual.extend(read["items"].as_array().unwrap().iter().cloned());
        if read["nextOffset"].is_null() {
            break;
        }
        offset = read["nextOffset"].as_u64().unwrap() as usize;
    }
    assert_eq!(
        actual,
        serde_json::to_value(&refs)
            .unwrap()
            .as_array()
            .unwrap()
            .clone()
    );
    let mut too_big = first.document_source.unwrap();
    too_big.selector = MediaSelector::FieldText {
        field: balloon(),
        start_char: 0,
        end_char: 8001,
        quote: "\u{1}".repeat(8001),
    };
    assert!(
        matches!(repo.create_creative_edition(&pid(&p),&CreateEditionRequest{title:"不得超限".into(),medium:CreativeMedium::Comic,idempotency_key:"large-quote".into(),sources:vec![too_big]},3),Err(RepositoryError::InvalidContent(code)) if code=="media_source_quote_limit")
    );
}
#[test]
fn exact_text_source_impacts_ignore_other_fields_but_detect_changes_and_deletion() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, CreativeMedium::Comic);
    let first = save(&mut repo, &p, comic("甲😀乙"));
    let mut source = first.document_source.clone().unwrap();
    source.selector = MediaSelector::FieldText {
        field: balloon(),
        start_char: 1,
        end_char: 2,
        quote: "😀".into(),
    };
    let e = repo
        .create_creative_edition(
            &pid(&p),
            &CreateEditionRequest {
                title: "精确改编".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "text-impact".into(),
                sources: vec![source.clone()],
            },
            2,
        )
        .unwrap();
    let desc = MediaTextField::PanelDescription {
        page_id: "page".into(),
        panel_id: "panel".into(),
    };
    let sibling = repo
        .patch_multimedia_document(
            &pid(&p),
            &p.active_edition_id,
            None,
            &patch(
                &first,
                "description-only",
                json!([op(&desc, "甲😀乙", 0, 1, "晴")]),
            ),
        )
        .unwrap();
    assert_eq!(
        repo.media_source_impacts(&pid(&p), &e.id, None, 0, 20)
            .unwrap()["items"][0]["status"],
        "current"
    );
    let changed = repo
        .patch_multimedia_document(
            &pid(&p),
            &p.active_edition_id,
            None,
            &patch(
                &sibling,
                "selected-text",
                json!([op(&balloon(), "甲😀乙", 1, 2, "雨")]),
            ),
        )
        .unwrap();
    let report = repo
        .media_source_impacts(&pid(&p), &e.id, None, 0, 20)
        .unwrap();
    assert_eq!(report["items"][0]["status"], "changed");
    assert_eq!(report["items"][0]["latest"]["selector"]["quote"], "雨");
    assert_eq!(
        repo.creative_editions(&pid(&p))
            .unwrap()
            .iter()
            .find(|v| v.id == e.id)
            .unwrap()
            .sources[0],
        source
    );
    repo.patch_multimedia_document(&pid(&p),&p.active_edition_id,None,&patch(&changed,"deleted-balloon",json!([{"kind":"removeBalloon","pageId":"page","panelId":"panel","balloonId":"balloon"}]))).unwrap();
    assert_eq!(
        repo.media_source_impacts(&pid(&p), &e.id, None, 0, 20)
            .unwrap()["items"][0]["status"],
        "selectionMissing"
    );
}

#[test]
fn creative_design_is_revision_owned_pageable_and_locally_editable_without_new_ui() {
    for medium in [CreativeMedium::Comic, CreativeMedium::Animation] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("design.sqlite");
        let mut repo = SqliteStoryRepository::open(&path).unwrap();
        let p = create(&mut repo, medium);
        let doc = if medium == CreativeMedium::Comic { comic("正文保留") }
            else { MultimediaDocument::empty(medium, "动画".into()).unwrap() };
        let first = save(&mut repo, &p, doc);
        let t = task(&mut repo, &first, "design");
        let design = "甲😀乙：主线、人物变化与阶段安排。\n".repeat(1000);
        let request = patch(&first, "design-save", json!([{"kind":"setCreativeDesign","text":design}]));
        let saved = repo.patch_task_multimedia_document(&t, &request).unwrap();
        assert_eq!(repo.patch_task_multimedia_document(&t, &request).unwrap().revision_id, saved.revision_id);
        assert_eq!(saved.document.creative_design(), Some(design.as_str()));
        let mut stale = request.clone(); stale.idempotency_key = "stale-design".into();
        assert!(repo.patch_task_multimedia_document(&t, &stale).is_err());
        let overview = saved.page(&MediaWorkspacePageRequest::default()).unwrap();
        assert!(overview["documentHeader"].get("creativeDesign").is_none());
        assert_eq!(overview["documentHeader"]["textFields"][0]["field"]["kind"], "creativeDesign");
        let mut restored = String::new(); let mut offset = 0;
        loop {
            let read = saved.page(&page(MediaTextField::CreativeDesign, offset, 8000)).unwrap();
            assert!(read.to_string().len() <= 65536);
            restored.push_str(read["text"].as_str().unwrap());
            if read["nextCharOffset"].is_null() { break; }
            offset = read["nextCharOffset"].as_u64().unwrap() as usize;
        }
        assert_eq!(restored, design);
        let changed = repo.patch_task_multimedia_document(&t, &patch(&saved, "design-edit", json!([op(&MediaTextField::CreativeDesign, &design, 1, 2, "晴")]))).unwrap();
        let mut original = serde_json::to_value(&first.document).unwrap();
        original["creativeDesign"] = json!(format!("甲晴{}", design.chars().skip(2).collect::<String>()));
        assert_eq!(serde_json::to_value(&changed.document).unwrap(), original);
        let history = repo.multimedia_revision_workspace(&pid(&p), &p.active_edition_id, None, saved.revision_id.as_ref().unwrap()).unwrap();
        assert_eq!(history.document.creative_design(), Some(design.as_str()));
        let initial = repo.multimedia_revision_workspace(&pid(&p), &p.active_edition_id, None, first.revision_id.as_ref().unwrap()).unwrap();
        assert!(initial.document.creative_design().is_none());
        let read = changed.page(&page(MediaTextField::CreativeDesign, 0, 3)).unwrap();
        let source: MediaSourceRef = serde_json::from_value(read["returnedSource"].clone()).unwrap();
        let edition = repo.create_creative_edition(&pid(&p), &CreateEditionRequest { title: "按设计改编".into(), medium: CreativeMedium::Animation, idempotency_key: "design-adaptation".into(), sources: vec![source.clone()] }, 2).unwrap();
        let downstream = repo.multimedia_workspace(&pid(&p), &edition.id, None).unwrap();
        let downstream_task = task(&mut repo, &downstream, "downstream");
        assert_eq!(repo.read_task_media_source(&downstream_task, &source, &page(MediaTextField::CreativeDesign, 0, 8000)).unwrap()["text"], "甲晴乙");
        let mut expanded = source.clone(); expanded.selector = MediaSelector::Whole;
        assert!(repo.read_task_media_source(&downstream_task, &expanded, &page(MediaTextField::CreativeDesign, 0, 8000)).is_err());
        assert!(repo.read_task_media_source(&downstream_task, &source, &page(balloon(), 0, 1)).is_err());
        drop(repo);
        let mut reopened = SqliteStoryRepository::open(&path).unwrap();
        let current = reopened.multimedia_workspace(&pid(&p), &p.active_edition_id, None).unwrap();
        assert_eq!(current.document, changed.document);
        let cleared = reopened.patch_multimedia_document(&pid(&p), &p.active_edition_id, None, &patch(&current,"clear-design",json!([{"kind":"setCreativeDesign","text":null}]))).unwrap();
        assert_eq!(cleared.document, first.document);
    }
}

use novel_codex_story_core::{MultimediaCanonRepository, MultimediaImpactRepository, MultimediaRepository, MultimediaSourcesRepository, TasksRepository};
