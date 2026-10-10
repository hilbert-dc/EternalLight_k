use novel_codex_story_core::*;
use serde_json::{Value, json};

fn create(repo: &mut SqliteStoryRepository, key: &str) -> CreativeProjectCreated {
    repo.create_creative_project(
        &CreateCreativeProjectRequest {
            title: "原作".into(),
            logline: String::new(),
            medium: CreativeMedium::Comic,
            idempotency_key: key.into(),
        },
        1,
    )
    .unwrap()
}
fn canon(statement: &str, revision: Option<String>, key: &str) -> SaveCanonDocument {
    SaveCanonDocument {
        expected_revision_id: revision,
        idempotency_key: key.into(),
        document: CanonDocument {
            title: "共同设定".into(),
            entries: vec![CanonEntry {
                id: "rule-light".into(),
                category: CanonCategory::Rule,
                statement: statement.into(),
                sources: vec![],
            }],
        },
    }
}
fn comic(a: &str, b: &str) -> MultimediaDocument {
    serde_json::from_value(json!({"kind":"comic","title":"漫画","format":"pages","pages":[{"id":"page","title":"第一页","width":1000,"height":1000,"readingDirection":"leftToRight","panels":[{"id":"panel-a","description":a,"rect":{"x":0,"y":0,"width":400,"height":400},"assetRevisionId":null,"referenceAssetRevisionIds":[],"balloons":[],"locked":false},{"id":"panel-b","description":b,"rect":{"x":500,"y":0,"width":400,"height":400},"assetRevisionId":null,"referenceAssetRevisionIds":[],"balloons":[],"locked":false}]}]})).unwrap()
}
fn save(doc: MultimediaDocument, revision: Option<String>, key: &str) -> SaveMultimediaDocument {
    SaveMultimediaDocument {
        expected_revision_id: revision,
        idempotency_key: key.into(),
        document: doc,
        read_set: vec![],
        sources: vec![],
    }
}
#[test]
fn author_canon_is_cas_durable_and_editions_retain_exact_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("canon.sqlite");
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let p = create(&mut repo, "p");
    let pid = ProjectId::parse(p.project.id.clone()).unwrap();
    let empty = repo.canon_workspace(&pid, None).unwrap();
    let r = canon("灯不能熄灭", None, "canon-1");
    let first = repo.save_canon_document(&pid, &r).unwrap();
    assert_eq!(
        first.revision_id,
        repo.save_canon_document(&pid, &r).unwrap().revision_id
    );
    assert!(matches!(
        repo.save_canon_document(&pid, &canon("另一个要求", None, "canon-1")),
        Err(RepositoryError::IdempotencyConflict)
    ));
    assert!(
        repo.save_canon_document(&pid, &canon("过期修改", None, "canon-conflict"))
            .is_err()
    );
    let e = repo
        .create_creative_edition(
            &pid,
            &CreateEditionRequest {
                title: "动画".into(),
                medium: CreativeMedium::Animation,
                idempotency_key: "adapt".into(),
                sources: vec![],
            },
            2,
        )
        .unwrap();
    let second = repo
        .save_canon_document(
            &pid,
            &canon("灯可以熄灭", first.revision_id.clone(), "canon-2"),
        )
        .unwrap();
    assert_ne!(first.snapshot_id, second.snapshot_id);
    assert_eq!(
        e.canon_snapshot_id.as_deref(),
        Some(first.snapshot_id.as_str())
    );
    assert!(
        repo.canon_workspace(&pid, Some(&empty.snapshot_id))
            .unwrap()
            .document
            .entries
            .is_empty()
    );
    assert_eq!(
        repo.canon_workspace(&pid, e.canon_snapshot_id.as_deref())
            .unwrap()
            .document
            .entries[0]
            .statement,
        "灯不能熄灭"
    );
    let other = create(&mut repo, "other");
    let other_id = ProjectId::parse(other.project.id).unwrap();
    assert!(
        repo.canon_workspace(&other_id, Some(&first.snapshot_id))
            .is_err()
    );
    let mut invalid = canon("无权来源", second.revision_id.clone(), "invalid");
    invalid.document.entries[0]
        .sources
        .push(first.document_source.clone().unwrap());
    invalid.document.entries[0].sources[0].scope_id = "forged-scope".into();
    assert!(repo.save_canon_document(&pid, &invalid).is_err());
    assert_eq!(
        repo.canon_workspace(&pid, None).unwrap().revision_id,
        second.revision_id
    );
    drop(repo);
    let repo = SqliteStoryRepository::open(path).unwrap();
    assert_eq!(
        repo.canon_workspace(&pid, None).unwrap().document.entries[0].statement,
        "灯可以熄灭"
    );
}
#[test]
fn impact_compares_selected_content_and_never_rebinds_the_adaptation() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = create(&mut repo, "p");
    let pid = ProjectId::parse(p.project.id.clone()).unwrap();
    let origin = &p.editions[0];
    let first = repo
        .save_multimedia_document(
            &pid,
            &origin.id,
            None,
            &save(comic("雨夜", "白天"), None, "s1"),
        )
        .unwrap();
    let mut source = first.document_source.clone().unwrap();
    source.selector = MediaSelector::Panel {
        page_id: "page".into(),
        panel_id: "panel-a".into(),
        region: None,
    };
    let adaptation = repo
        .create_creative_edition(
            &pid,
            &CreateEditionRequest {
                title: "改编漫画".into(),
                medium: CreativeMedium::Comic,
                idempotency_key: "adapt".into(),
                sources: vec![source.clone()],
            },
            2,
        )
        .unwrap();
    let second = repo
        .save_multimedia_document(
            &pid,
            &origin.id,
            None,
            &save(comic("雨夜", "傍晚"), first.revision_id, "s2"),
        )
        .unwrap();
    let report = repo
        .media_source_impacts(&pid, &adaptation.id, None, 0, 20)
        .unwrap();
    assert_eq!(report["items"][0]["status"], "current");
    assert_eq!(
        report["items"][0]["source"]["revisionId"],
        source.revision_id
    );
    let third = repo
        .save_multimedia_document(
            &pid,
            &origin.id,
            None,
            &save(comic("雪夜", "傍晚"), second.revision_id, "s3"),
        )
        .unwrap();
    assert_eq!(
        repo.media_source_impacts(&pid, &adaptation.id, None, 0, 20)
            .unwrap()["items"][0]["status"],
        "changed"
    );
    let mut removed: Value = serde_json::to_value(comic("雪夜", "傍晚")).unwrap();
    removed["pages"][0]["panels"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    repo.save_multimedia_document(
        &pid,
        &origin.id,
        None,
        &save(
            serde_json::from_value(removed).unwrap(),
            third.revision_id,
            "s4",
        ),
    )
    .unwrap();
    assert_eq!(
        repo.media_source_impacts(&pid, &adaptation.id, None, 0, 20)
            .unwrap()["items"][0]["status"],
        "selectionMissing"
    );
    assert_eq!(
        repo.multimedia_workspace(&pid, &adaptation.id, None)
            .unwrap()
            .edition
            .sources,
        vec![source]
    );
    assert!(
        repo.media_source_impacts(&pid, &adaptation.id, None, 1, 0)
            .is_err()
    );
}
