use novel_codex_story_core::*;
use serde_json::json;

#[test]
fn novel_and_comic_sources_remain_pinned_across_pages_and_combined_adaptation() {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = ProjectId::parse("catalog-project").unwrap();
    repo.create_project(ProjectRecord { project_id:p.clone(),title:"小说".into(),logline:"".into(),created_at:1,updated_at:1 }, BranchId::parse("main").unwrap(), SnapshotId::parse("s0").unwrap()).unwrap();
    let novel = repo.creative_editions(&p).unwrap()[0].clone();
    for n in 1..=3 {
        repo.save_candidate(SaveCandidateRequest {
            project_id:p.clone(),branch_id:BranchId::parse("main").unwrap(),object_id:ObjectId::parse(format!("draft-chapter-{n}")).unwrap(),object_kind:"draft".into(),
            revision_id:RevisionId::parse(format!("rev-{n}")).unwrap(),commit_id:CommitId::parse(format!("commit-{n}")).unwrap(),idempotency_key:IdempotencyKey::parse(format!("save-{n}")).unwrap(),
            request_digest:format!("request-{n}"),content_digest:format!("digest-{n}"),expected_project_revision:0,
            expected_snapshot_id:SnapshotId::parse(format!("s{}",n-1)).unwrap(),next_snapshot_id:SnapshotId::parse(format!("s{n}")).unwrap(),expected_candidate_revision_id:None,
            content_json:json!({"title":format!("第{n}章"),"body":"雨落了下来"}).to_string(),chapter_projection:Some(ChapterProjection{chapter_id:format!("chapter-{n}"),number:n,title:format!("第{n}章")}),task_output:None,linked_impact_ids:vec![],
        }).unwrap();
    }
    let page = repo.media_source_catalog(&p,&novel.id,Some("s2"),0,1).unwrap();
    assert_eq!(page["total"],2); assert_eq!(page["nextOffset"],1);
    let next = repo.media_source_catalog(&p,&novel.id,Some("s2"),1,1).unwrap();
    assert_eq!(next["total"],2); assert!(next["nextOffset"].is_null());
    assert_ne!(page["items"][0]["source"]["objectId"],next["items"][0]["source"]["objectId"]);
    assert_eq!(repo.media_source_catalog(&p,&novel.id,None,0,20).unwrap()["total"],3);
    assert!(repo.media_source_catalog(&p,&novel.id,None,1,20).is_err());
    assert!(repo.media_source_catalog(&p,&novel.id,Some("s2"),3,20).is_err());
    let original:MediaSourceRef = serde_json::from_value(page["items"][0]["source"].clone()).unwrap();
    let comic = repo.create_creative_edition(&p,&CreateEditionRequest{title:"漫画".into(),medium:CreativeMedium::Comic,idempotency_key:"comic".into(),sources:vec![original.clone()]},2).unwrap();
    assert!(repo.media_source_catalog(&p,&comic.id,None,0,20).unwrap()["items"].as_array().unwrap().is_empty());
    assert!(repo.media_source_catalog(&p,&comic.id,Some("s2"),0,20).is_err());
    let saved = repo.save_multimedia_document(&p,&comic.id,None,&SaveMultimediaDocument{expected_revision_id:None,idempotency_key:"comic-save".into(),document:serde_json::from_value(json!({"kind":"comic","title":"漫画","format":"pages","pages":[]})).unwrap(),sources:vec![],read_set:vec![]}).unwrap();
    let comic_catalog = repo.media_source_catalog(&p,&comic.id,None,0,20).unwrap();
    assert_eq!(comic_catalog["items"][0]["source"],json!(saved.document_source.unwrap()));
    let visual:MediaSourceRef=serde_json::from_value(comic_catalog["items"][0]["source"].clone()).unwrap();
    let request=CreateEditionRequest{title:"组合动画".into(),medium:CreativeMedium::Animation,idempotency_key:"animation".into(),sources:vec![original,visual]};
    let result=repo.create_creative_edition(&p,&request,3).unwrap();
    assert_eq!(result.sources,request.sources);
    assert_eq!(repo.create_creative_edition(&p,&request,4).unwrap().id,result.id);
    let other=repo.create_creative_project(&CreateCreativeProjectRequest{title:"其他作品".into(),logline:"".into(),medium:CreativeMedium::Comic,idempotency_key:"other".into()},4).unwrap();
    let other_id=ProjectId::parse(other.project.id).unwrap();
    assert!(repo.media_source_catalog(&other_id,&comic.id,None,0,20).is_err());
    assert!(repo.media_source_catalog(&other_id,&other.editions[0].id,Some("s2"),0,20).is_err());
}

use novel_codex_story_core::{MultimediaCatalogRepository, MultimediaRepository, ProjectRevisionRepository};
