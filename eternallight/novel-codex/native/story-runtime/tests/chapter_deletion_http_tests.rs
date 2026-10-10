use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[path = "support/author_task_fixture.rs"]
mod fixture;
use fixture::AuthorTaskFixture;
const AUTHOR: &str = "chapter-delete-author";
const CANDIDATE: &str = "chapter-delete-candidate";

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    token: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({"rejected":true})),
    )
}
async fn read(app: &Router, path: &str) -> Value {
    let (status, value) = call(app, "GET", path, AUTHOR, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}
async fn create_project(app: &Router) -> String {
    let (status, value) = call(
        app,
        "POST",
        "/api/projects",
        AUTHOR,
        json!({"medium":"novel","idempotencyKey":uuid::Uuid::new_v4().to_string(),"title":"真实删除","logline":"不重排其余章"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    format!("/api/projects/{}", value["project"]["id"].as_str().unwrap())
}
async fn chapter(app: &Router, root: &str, key: &str) -> Value {
    let workspace = read(app, &format!("{root}/creative-workspace")).await;
    let (status, value) = call(
        app,
        "POST",
        &format!("{root}/chapters/materialize-standalone"),
        AUTHOR,
        json!({"title":key,"expectedRevision":workspace["revision"],"idempotencyKey":key}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}
async fn save(
    app: &Router,
    root: &str,
    chapter: &Value,
    key: &str,
    locked: bool,
) -> (Value, Value) {
    let workspace = read(app, &format!("{root}/creative-workspace")).await;
    let input = json!({"payload":{"chapterId":chapter["chapterId"],"chapterNumber":chapter["chapterNumber"],"title":"保存的章名","body":"白塔的灯亮了。","sceneGroups":[{"id":"scene-one","sceneId":null,"body":"白塔的灯亮了。","locked":locked}]},"expectedRevision":workspace["revision"],"expectedCandidateRevisionId":null,"idempotencyKey":key});
    let (status, value) = call(
        app,
        "POST",
        &format!("{root}/drafts/checkpoints"),
        AUTHOR,
        input.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{value}");
    (value, input)
}
fn delete_input(workspace: &Value, key: &str) -> Value {
    json!({"expectedRevision":workspace["revision"],"idempotencyKey":key})
}
fn authoritative(db: &rusqlite::Connection, project: &str) -> Value {
    let mut rows=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'story_%' ORDER BY name").unwrap();
    let names = rows
        .query_map([], |r| r.get::<_, String>(/*idx*/ 0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut counts = serde_json::Map::new();
    for name in names {
        let count: i64 = db
            .query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| {
                r.get(/*idx*/ 0)
            })
            .unwrap();
        counts.insert(name, json!(count));
    }
    json!({"counts":counts,"revision":db.query_row("SELECT revision FROM story_workspaces_v2 WHERE project_id=?1",[project],|r|r.get::<_,i64>(/*idx*/ 0)).unwrap(),"head":db.query_row("SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",[project],|r|r.get::<_,String>(/*idx*/ 0)).unwrap()})
}

#[tokio::test]
async fn deleting_published_chapter_removes_current_authority_and_preserves_exact_history() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("deletion.sqlite3");
    let app = novel_codex_story_runtime::router(fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let root = create_project(&app).await;
    let project = root.rsplit('/').next().unwrap();
    let mut chapters = Vec::new();
    let mut saved = Vec::new();
    let mut inputs = Vec::new();
    for number in 1..=3 {
        let row = chapter(&app, &root, &format!("chapter-{number}")).await;
        let (receipt, input) = save(
            &app,
            &root,
            &row,
            &format!("save-{number}"),
            /*locked*/ false,
        )
        .await;
        chapters.push(row);
        saved.push(receipt);
        inputs.push(input);
    }
    let(status,volume)=call(&app,"POST",&format!("{root}/catalog"),AUTHOR,json!({"expectedRevision":0,"idempotencyKey":"create-volume","operation":{"type":"createVolume","title":"保留卷"}})).await;
    assert_eq!(status, StatusCode::OK, "{volume}");
    let(status,placed)=call(&app,"POST",&format!("{root}/catalog"),AUTHOR,json!({"expectedRevision":1,"idempotencyKey":"place-middle","operation":{"type":"placeChapter","chapterId":chapters[1]["chapterId"],"volumeId":volume["volumeId"]}})).await;
    assert_eq!(status, StatusCode::OK, "{placed}");
    let middle = chapters[1]["chapterId"].as_str().unwrap();
    let object = format!("draft-{middle}");
    let extraction = format!("commitment-extraction-{middle}");
    let(status,review)=call(&app,"POST",&format!("{root}/reviews"),AUTHOR,json!({"artifactKind":"draft","objectId":object,"revisionId":saved[1]["revisionId"],"proposals":[],"selections":{},"idempotencyKey":"review-middle"})).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let workspace = read(&app, &format!("{root}/creative-workspace")).await;
    let(status,acceptance)=call(&app,"POST",&format!("{root}/reviews/{}/accept",review["id"].as_str().unwrap()),AUTHOR,json!({"expectedRevision":workspace["revision"],"selections":{"noConfirmedChanges":true},"idempotencyKey":"publish-middle"})).await;
    assert_eq!(status, StatusCode::OK, "{acceptance}");
    let(status,selection)=call(&app,"PUT",&format!("{root}/working-baselines/draft"),AUTHOR,json!({"objectId":middle,"revisionId":saved[1]["revisionId"],"expectedSelectionRevision":0})).await;
    assert_eq!(status, StatusCode::OK, "{selection}");
    let workspace = read(&app, &format!("{root}/creative-workspace")).await;
    let base = workspace["headSnapshotId"].as_str().unwrap();
    assert_eq!(workspace["workingBaselines"].as_array().unwrap().len(), 1);
    assert_eq!(workspace["workingBaselines"][0]["objectId"], middle);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    // Authoritative fixture sources distinguish fixed snapshot bindings from newer current heads.
    for (index, row) in chapters.iter().enumerate() {
        let obj = format!("draft-{}", row["chapterId"].as_str().unwrap());
        for suffix in ["fixed", "current"] {
            db.execute("INSERT INTO story_modeling_batches_v2(batch_id,project_id,object_id,source_revision_id,input_project_revision,extractor_version,status,created_at,published_at) VALUES(?1,?2,?3,?4,0,?1,'ready',1,1)",rusqlite::params![format!("batch-{index}-{suffix}"),project,obj,saved[index]["revisionId"].as_str().unwrap()]).unwrap();
        }
        db.execute(
            "INSERT INTO story_knowledge_heads_v2(project_id,object_id,batch_id) VALUES(?1,?2,?3)",
            rusqlite::params![project, obj, format!("batch-{index}-current")],
        )
        .unwrap();
        db.execute("UPDATE story_snapshot_knowledge_v2 SET batch_id=?1 WHERE snapshot_id=?2 AND object_id=?3",rusqlite::params![format!("batch-{index}-fixed"),base,obj]).unwrap();
    }
    db.execute("INSERT INTO story_continuation_intents_v2(project_id,target_kind,target_id,working_revision_id,author_intent,status,updated_at) VALUES(?1,'draft',?2,?3,'原指令','ready',1)",rusqlite::params![project,middle,saved[1]["revisionId"].as_str().unwrap()]).unwrap();
    db.execute("INSERT INTO story_chapter_plan_bindings_v2(project_id,plan_revision_id,chapter_plan_id,chapter_id,created_at) VALUES(?1,'frozen-plan','plan-two',?2,1)",rusqlite::params![project,middle]).unwrap();
    db.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) VALUES('extraction-history',?1,?2,'commitmentExtraction','digest',?3)",rusqlite::params![project,extraction,json!({"chapterId":middle,"sourceRevisionId":saved[1]["revisionId"],"candidates":[]}).to_string()]).unwrap();
    db.execute("INSERT INTO story_object_heads_v2(project_id,object_id,object_kind,candidate_revision_id) VALUES(?1,?2,'commitmentExtraction','extraction-history')",rusqlite::params![project,extraction]).unwrap();
    db.execute("INSERT INTO story_snapshot_members_v2(snapshot_id,object_id,revision_id) VALUES(?1,?2,'extraction-history')",rusqlite::params![base,extraction]).unwrap();
    db.execute("INSERT INTO story_facts_v2(fact_id,project_id,payload_json,accepted_by_commit_id,project_revision) VALUES('independent-fact',?1,'{}','author-decision',0)",[project]).unwrap();
    let old_members: Value = {
        let mut s=db.prepare("SELECT object_id,revision_id FROM story_snapshot_members_v2 WHERE snapshot_id=?1 ORDER BY object_id").unwrap();
        json!(
            s.query_map([base], |r| Ok((
                r.get::<_, String>(/*idx*/ 0)?,
                r.get::<_, String>(/*idx*/ 1)?
            )))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
        )
    };
    let input = delete_input(&workspace, "delete-middle");
    let endpoint = format!("{root}/chapters/{middle}/delete");
    let (status, receipt) = call(&app, "POST", &endpoint, AUTHOR, input.clone()).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let after = read(&app, &format!("{root}/creative-workspace")).await;
    assert_eq!(
        after["chapters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (&c["id"], &c["number"]))
            .collect::<Vec<_>>(),
        vec![
            (&chapters[0]["chapterId"], &json!(1)),
            (&chapters[2]["chapterId"], &json!(3))
        ]
    );
    assert_eq!(
        (after["revision"].clone(), after["headSnapshotId"].clone()),
        (
            json!(workspace["revision"].as_i64().unwrap() + 1),
            receipt["resultSnapshotId"].clone()
        )
    );
    let position = read(&app, &format!("{root}/generation-start-position")).await;
    assert_eq!(
        position,
        json!({"completedChapters":3,"firstStage":"coordinate"})
    );
    for table in [
        "story_chapters_v2",
        "story_chapter_plan_bindings_v2",
        "story_chapter_placements_v1",
    ] {
        assert_eq!(
            db.query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE chapter_id=?1"),
                [middle],
                |r| r.get::<_, i64>(/*idx*/ 0)
            )
            .unwrap(),
            0
        );
    }
    for table in ["story_object_heads_v2", "story_knowledge_heads_v2"] {
        assert_eq!(
            db.query_row(
                &format!(
                    "SELECT COUNT(*) FROM {table} WHERE project_id=?1 AND object_id IN(?2,?3)"
                ),
                rusqlite::params![project, object, extraction],
                |r| r.get::<_, i64>(/*idx*/ 0)
            )
            .unwrap(),
            0
        );
    }
    assert_eq!(after["workingBaselines"], json!([]));
    assert_eq!(db.query_row("SELECT COUNT(*) FROM story_working_baselines_v2 WHERE project_id=?1 AND object_kind='draft' AND object_id=?2",rusqlite::params![project,middle],|r|r.get::<_,i64>(/*idx*/ 0)).unwrap(),0);
    assert_eq!(db.query_row("SELECT COUNT(*) FROM story_continuation_intents_v2 WHERE project_id=?1 AND target_kind='draft' AND target_id=?2",rusqlite::params![project,middle],|r|r.get::<_,i64>(/*idx*/ 0)).unwrap(),0);
    let catalog = read(&app, &format!("{root}/catalog")).await;
    assert_eq!(catalog["placements"], json!([]));
    assert_eq!(catalog["volumes"], volume["catalog"]["volumes"]);
    assert_eq!(catalog["revision"], 3);
    let next = receipt["resultSnapshotId"].as_str().unwrap();
    assert_eq!(db.query_row("SELECT COUNT(*) FROM story_snapshot_members_v2 WHERE snapshot_id=?1 AND object_id IN(?2,?3)",rusqlite::params![next,object,extraction],|r|r.get::<_,i64>(/*idx*/ 0)).unwrap(),0);
    assert_eq!(db.query_row("SELECT COUNT(*) FROM story_snapshot_chapters_v1 WHERE snapshot_id=?1 AND chapter_id=?2",rusqlite::params![next,middle],|r|r.get::<_,i64>(/*idx*/ 0)).unwrap(),0);
    let actual: Vec<(String, Option<String>)> = {
        let mut s=db.prepare("SELECT object_id,batch_id FROM story_snapshot_knowledge_v2 WHERE snapshot_id=?1 ORDER BY object_id").unwrap();
        s.query_map([next], |r| Ok((r.get(/*idx*/ 0)?, r.get(/*idx*/ 1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    let mut expected = vec![
        (
            format!("draft-{}", chapters[0]["chapterId"].as_str().unwrap()),
            Some("batch-0-fixed".into()),
        ),
        (
            format!("draft-{}", chapters[2]["chapterId"].as_str().unwrap()),
            Some("batch-2-fixed".into()),
        ),
    ];
    expected.sort();
    assert_eq!(actual, expected);
    let history: Value = {
        let mut s=db.prepare("SELECT object_id,revision_id FROM story_snapshot_members_v2 WHERE snapshot_id=?1 ORDER BY object_id").unwrap();
        json!(
            s.query_map([base], |r| Ok((
                r.get::<_, String>(/*idx*/ 0)?,
                r.get::<_, String>(/*idx*/ 1)?
            )))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
        )
    };
    assert_eq!(history, old_members);
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_facts_v2 WHERE fact_id='independent-fact'",
            [],
            |r| r.get::<_, i64>(/*idx*/ 0)
        )
        .unwrap(),
        1
    );
    let replay = call(&app, "POST", &endpoint, AUTHOR, input.clone()).await;
    assert_eq!(replay, (StatusCode::OK, receipt.clone()));
    let before_bad = authoritative(&db, project);
    let conflict = call(
        &app,
        "POST",
        &format!(
            "{root}/chapters/{}/delete",
            chapters[0]["chapterId"].as_str().unwrap()
        ),
        AUTHOR,
        input,
    )
    .await;
    assert_eq!(conflict.0, StatusCode::CONFLICT);
    let mut resurrection = inputs[1].clone();
    resurrection["expectedRevision"] = after["revision"].clone();
    resurrection["expectedCandidateRevisionId"] = Value::Null;
    resurrection["idempotencyKey"] = json!("resurrect-deleted");
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{root}/drafts/checkpoints"),
            AUTHOR,
            resurrection
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!(
                "{root}/commits/{}/undo",
                acceptance["commitId"].as_str().unwrap()
            ),
            AUTHOR,
            json!({"expectedRevision":after["revision"],"idempotencyKey":"undo-deleted"})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(authoritative(&db, project), before_bad);
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{root}/drafts/checkpoints"),
            AUTHOR,
            inputs[1].clone()
        )
        .await,
        (StatusCode::CREATED, saved[1].clone())
    );
    assert_eq!(authoritative(&db, project), before_bad);
    let fourth = chapter(&app, &root, "after-delete").await;
    assert_eq!(fourth["chapterNumber"], 4);
}

#[tokio::test]
async fn deletion_rejects_foreign_stale_locked_and_active_without_partial_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("guards.sqlite3");
    let app = novel_codex_story_runtime::router(fixture::state(
        path.clone(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let root = create_project(&app).await;
    let other = create_project(&app).await;
    let project = root.rsplit('/').next().unwrap();
    let row = chapter(&app, &root, "protected").await;
    save(&app, &root, &row, "locked", /*locked*/ true).await;
    let id = row["chapterId"].as_str().unwrap();
    let endpoint = format!("{root}/chapters/{id}/delete");
    let workspace = read(&app, &format!("{root}/creative-workspace")).await;
    let db = rusqlite::Connection::open(&path).unwrap();
    let baseline = authoritative(&db, project);
    for (path, token, input, status) in [
        (
            &endpoint,
            CANDIDATE,
            delete_input(&workspace, "candidate"),
            StatusCode::UNAUTHORIZED,
        ),
        (
            &endpoint,
            AUTHOR,
            json!({"expectedRevision":0,"idempotencyKey":"stale"}),
            StatusCode::CONFLICT,
        ),
        (
            &endpoint,
            AUTHOR,
            delete_input(&workspace, "locked"),
            StatusCode::CONFLICT,
        ),
    ] {
        assert_eq!(call(&app, "POST", path, token, input).await.0, status);
        assert_eq!(authoritative(&db, project), baseline);
    }
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{other}/chapters/{id}/delete"),
            AUTHOR,
            json!({"expectedRevision":0,"idempotencyKey":"foreign"})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(authoritative(&db, project), baseline);
    let object = format!("draft-{id}");
    db.execute("UPDATE story_revisions_v2 SET content_json=json_set(content_json,'$.sceneGroups[0].locked',json('false')) WHERE object_id=?1",[&object]).unwrap();
    db.execute(
        "UPDATE story_object_heads_v2 SET locked=1 WHERE object_id=?1",
        [&object],
    )
    .unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            &endpoint,
            AUTHOR,
            delete_input(&workspace, "object-locked")
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(authoritative(&db, project), baseline);
    db.execute(
        "UPDATE story_object_heads_v2 SET locked=0 WHERE object_id=?1",
        [&object],
    )
    .unwrap();
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let(status,task)=call(&app,"POST",&format!("{root}/tasks"),AUTHOR,json!({"operation":"advisory","inputRevision":workspace["revision"],"threadId":"active-root","target":{"type":"project"},"capabilityHash":"a".repeat(64),"capabilityExpiresAt":expires,"authorIntent":"只讨论"})).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    for status in ["queued", "running", "waiting"] {
        db.execute(
            "UPDATE story_tasks_v2 SET status=?1 WHERE task_id=?2",
            rusqlite::params![status, task["id"].as_str().unwrap()],
        )
        .unwrap();
        let before = authoritative(&db, project);
        let (result, error) = call(
            &app,
            "POST",
            &endpoint,
            AUTHOR,
            delete_input(&workspace, &format!("active-{status}")),
        )
        .await;
        assert_eq!(
            (result, error["error"]["code"].clone()),
            (StatusCode::CONFLICT, json!("catalog_task_active"))
        );
        assert_eq!(authoritative(&db, project), before);
    }
    db.execute(
        "UPDATE story_tasks_v2 SET status='cancelled' WHERE task_id=?1",
        [task["id"].as_str().unwrap()],
    )
    .unwrap();
    let (status, receipt) = call(
        &app,
        "POST",
        &endpoint,
        AUTHOR,
        delete_input(&workspace, "finally-delete"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_tasks_v2 WHERE task_id=?1",
            [task["id"].as_str().unwrap()],
            |r| r.get::<_, i64>(/*idx*/ 0)
        )
        .unwrap(),
        1
    );
}
