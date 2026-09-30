use pretty_assertions::assert_eq;
use rusqlite::params;
use serde_json::{Value, json};

use super::proposition_source_visible;
use crate::model_sources::ModelSourceScope;
use crate::{
    BranchId, ProjectId, ProjectRecord, RepositoryError, SnapshotId, SqliteStoryRepository,
};

fn fixture() -> (SqliteStoryRepository, ProjectId, SnapshotId, Value) {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("source-integrity").unwrap();
    let snapshot = SnapshotId::parse("source-capture").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "来源".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("source-start").unwrap(),
        )
        .unwrap();
    let evidence = json!({"quote":"林夕在港口","start":0,"end":5});
    let entities = json!([
        {"id":"linxi","kind":"character","displayName":"林夕","aliases":[],"evidence":evidence},
        {"id":"port","kind":"location","displayName":"港口","aliases":[],"evidence":evidence},
        {"id":"at-port","kind":"proposition","displayName":"林夕在港口","aliases":[],"evidence":evidence}
    ]);
    let assertions = json!([{"kind":"relation","subjectId":"linxi","targetId":"port","propositionId":"at-port",
        "predicate":"locatedAt","relationType":"locatedAt","value":true,"validFrom":1,"evidence":evidence}]);
    repository.connection.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) VALUES('source-revision',?1,'draft-source','draft','test',?2)",
        params![project.as_str(),json!({"body":"林夕在港口","chapterNumber":1}).to_string()]).unwrap();
    repository.connection.execute("INSERT INTO story_object_heads_v2(project_id,object_id,object_kind,candidate_revision_id) VALUES(?1,'draft-source','draft','source-revision')",[project.as_str()]).unwrap();
    repository.connection.execute("INSERT INTO story_modeling_batches_v2(batch_id,project_id,object_id,source_revision_id,input_project_revision,extractor_version,status,assertions_json,entities_json,created_at,published_at) VALUES('source-batch',?1,'draft-source','source-revision',0,'test','ready',?2,?3,1,2)",
        params![project.as_str(),assertions.to_string(),entities.to_string()]).unwrap();
    repository.connection.execute("INSERT INTO story_knowledge_heads_v2(project_id,object_id,batch_id) VALUES(?1,'draft-source','source-batch')",[project.as_str()]).unwrap();
    repository.connection.execute("INSERT INTO story_entity_claims_v2(project_id,batch_id,entity_id,kind,display_name,aliases_json,evidence_json) VALUES(?1,'source-batch','at-port','proposition','林夕在港口','[]',?2)",
        params![project.as_str(),evidence.to_string()]).unwrap();
    repository.connection.execute("INSERT INTO story_snapshots_v2(snapshot_id,project_id,branch_id,parent_snapshot_id) VALUES(?1,?2,'main','source-start')",params![snapshot.as_str(),project.as_str()]).unwrap();
    repository.connection.execute("INSERT INTO story_snapshot_members_v2(snapshot_id,object_id,revision_id) VALUES(?1,'draft-source','source-revision')",[snapshot.as_str()]).unwrap();
    (repository, project, snapshot, entities)
}

#[test]
fn stale_index_cannot_authorize_damaged_source_payload() {
    let (repository, project, snapshot, entities) = fixture();
    let original_revision = repository.project_revision(&project).unwrap();
    for mutation in [
        json!([entities[0], entities[1]]),
        {
            let mut value = entities.clone();
            value[2]["kind"] = json!("item");
            value
        },
        {
            let mut value = entities.clone();
            value.as_array_mut().unwrap().push(entities[2].clone());
            value
        },
        {
            let mut value = entities.clone();
            value[2]["evidence"]["quote"] = json!("伪造");
            value
        },
    ] {
        repository.connection.execute("UPDATE story_modeling_batches_v2 SET entities_json=?1 WHERE batch_id='source-batch'",[mutation.to_string()]).unwrap();
        for scope in [
            ModelSourceScope::Current,
            ModelSourceScope::Snapshot(&snapshot),
        ] {
            assert_eq!(
                proposition_source_visible(
                    &repository.connection,
                    &project,
                    scope,
                    "source-batch",
                    "at-port"
                ),
                Err(RepositoryError::CorruptState)
            );
        }
        assert_eq!(
            repository.project_revision(&project).unwrap(),
            original_revision
        );
    }
}

#[test]
fn source_visibility_uses_authority_not_a_missing_entity_index_and_keeps_history_frozen() {
    let (repository, project, snapshot, _) = fixture();
    repository
        .connection
        .execute(
            "DELETE FROM story_entity_claims_v2 WHERE batch_id='source-batch'",
            [],
        )
        .unwrap();
    assert_eq!(
        proposition_source_visible(
            &repository.connection,
            &project,
            ModelSourceScope::Current,
            "source-batch",
            "at-port"
        ),
        Ok(true)
    );
    repository
        .connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET status='stale' WHERE batch_id='source-batch'",
            [],
        )
        .unwrap();
    assert_eq!(
        proposition_source_visible(
            &repository.connection,
            &project,
            ModelSourceScope::Current,
            "source-batch",
            "at-port"
        ),
        Ok(false)
    );
    assert_eq!(
        proposition_source_visible(
            &repository.connection,
            &project,
            ModelSourceScope::Snapshot(&snapshot),
            "source-batch",
            "at-port"
        ),
        Ok(true)
    );
    assert_eq!(
        proposition_source_visible(
            &repository.connection,
            &project,
            ModelSourceScope::Snapshot(&snapshot),
            "source-batch",
            "unknown"
        ),
        Ok(false)
    );
    let other = ProjectId::parse("other-source-project").unwrap();
    assert_eq!(
        proposition_source_visible(
            &repository.connection,
            &other,
            ModelSourceScope::Snapshot(&snapshot),
            "source-batch",
            "at-port"
        ),
        Ok(false)
    );
}

#[test]
fn current_source_mismatch_cannot_replace_a_frozen_or_published_scope() {
    let (repository, project, snapshot, _) = fixture();
    repository.connection.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) VALUES('new-source',?1,'draft-source','draft','new',?2)",
        params![project.as_str(),json!({"body":"新稿没有这条命题"}).to_string()]).unwrap();
    repository.connection.execute("UPDATE story_object_heads_v2 SET candidate_revision_id='new-source' WHERE object_id='draft-source'",[]).unwrap();
    for (scope, expected) in [
        (ModelSourceScope::Current, false),
        (ModelSourceScope::Snapshot(&snapshot), true),
        (ModelSourceScope::Published, true),
        (
            ModelSourceScope::Snapshot(&SnapshotId::parse("source-start").unwrap()),
            false,
        ),
    ] {
        assert_eq!(
            proposition_source_visible(
                &repository.connection,
                &project,
                scope,
                "source-batch",
                "at-port"
            ),
            Ok(expected)
        );
    }
}

#[test]
fn source_assertion_schema_and_unicode_evidence_are_checked_without_repair() {
    let (repository, project, snapshot, _) = fixture();
    let evidence = json!({"quote":"林夕在港口","start":0,"end":5});
    for claims in [
        json!([{"kind":"state","subjectId":"linxi","predicate":"alive","value":"not a boolean",
            "stateSchema":{"cardinality":"single","valueType":{"type":"boolean"}},"validFrom":1,"evidence":evidence}]),
        json!([{"kind":"assertion","subjectId":"linxi","predicate":"present","value":true,
            "evidence":{"quote":"林夕在港口","start":0,"end":4}}]),
    ] {
        repository.connection.execute("UPDATE story_modeling_batches_v2 SET assertions_json=?1 WHERE batch_id='source-batch'",[claims.to_string()]).unwrap();
        for scope in [
            ModelSourceScope::Current,
            ModelSourceScope::Snapshot(&snapshot),
            ModelSourceScope::Published,
        ] {
            assert_eq!(
                proposition_source_visible(
                    &repository.connection,
                    &project,
                    scope,
                    "source-batch",
                    "at-port"
                ),
                Err(RepositoryError::CorruptState)
            );
        }
        let unchanged: String = repository.connection.query_row("SELECT assertions_json FROM story_modeling_batches_v2 WHERE batch_id='source-batch'",[],|row| row.get(0)).unwrap();
        assert_eq!(unchanged, claims.to_string());
    }
}
