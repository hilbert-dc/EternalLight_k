use rusqlite::params;
use serde_json::json;

use super::{ImpactAnalysisQuery, ImpactAnalysisScope, ImpactRevisionPair};
use crate::{
    BranchId, ObjectId, ProjectId, ProjectRecord, RepositoryError, RevisionId, RevisionRecord,
    SnapshotId, SqliteStoryRepository,
};

#[test]
fn impact_paths_preserve_independent_evidence_and_do_not_cross_snapshot() {
    let mut repository = SqliteStoryRepository::in_memory().unwrap();
    let project_id = ProjectId::parse("impact-analysis-project").unwrap();
    let initial_snapshot = SnapshotId::parse("impact-initial-snapshot").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: project_id.clone(),
                title: "港城".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            initial_snapshot.clone(),
        )
        .unwrap();
    let relations = [
        ("event-a", "event-b", "causes", "p1"),
        ("event-a", "event-b", "enables", "p2"),
        ("event-b", "event-c", "reveals", "p3"),
        ("event-a", "place-l", "locatedAt", "p4"),
    ];
    let assertions = relations
        .iter()
        .map(|(from, to, kind, proposition)| {
            json!({
                "kind": "relation", "subjectId": from, "targetId": to,
                "relationType": kind, "propositionId": proposition,
                "value": true, "validFrom": 1,
                "evidence": {"quote": format!("{from} {kind} {to}"), "start": 0, "end": 3}
            })
        })
        .collect::<Vec<_>>();
    let entities = ["event-a", "event-b", "event-c", "place-l", "p1", "p2", "p3", "p4"]
        .iter()
        .map(|id| {
            json!({"id": id, "kind": if id.starts_with("event") { "event" } else if id.starts_with("place") { "location" } else { "proposition" },
                "displayName": id, "aliases": [], "evidence": {"quote": id, "start": 0, "end": 3}})
        })
        .collect::<Vec<_>>();
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES ('impact-revision', ?1, 'draft-impact', 'draft', 'digest', ?2)",
        params![project_id.as_str(), json!({"body":"event-a causes event-b"}).to_string()],
    ).unwrap();
    crate::search_index::index_revision(
        &repository.connection,
        &RevisionRecord {
            project_id: project_id.clone(),
            object_id: ObjectId::parse("draft-impact").unwrap(),
            object_kind: "draft".into(),
            revision_id: RevisionId::parse("impact-revision").unwrap(),
            parent_revision_id: None,
            content_digest: "digest".into(),
            content_json: json!({"body":"event-a causes event-b"}).to_string(),
        },
    )
    .unwrap();
    repository.connection.execute(
        "INSERT INTO story_object_heads_v2(project_id, object_id, object_kind, candidate_revision_id) VALUES (?1, 'draft-impact', 'draft', 'impact-revision')",
        [project_id.as_str()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, assertions_json, entities_json, created_at, published_at) VALUES ('impact-batch', ?1, 'draft-impact', 'impact-revision', 0, 'test', 'ready', ?2, ?3, 1, 2)",
        params![project_id.as_str(), json!(assertions).to_string(), json!(entities).to_string()],
    ).unwrap();
    repository.connection.execute(
        "INSERT INTO story_knowledge_heads_v2(project_id, object_id, batch_id) VALUES (?1, 'draft-impact', 'impact-batch')",
        [project_id.as_str()],
    ).unwrap();
    for entity in &entities {
        repository.connection.execute(
            "INSERT INTO story_entity_claims_v2(project_id, batch_id, entity_id, kind, display_name, aliases_json, evidence_json) VALUES (?1, 'impact-batch', ?2, ?3, ?4, '[]', ?5)",
            params![project_id.as_str(), entity["id"].as_str(), entity["kind"].as_str(),
                entity["displayName"].as_str(), entity["evidence"].to_string()],
        ).unwrap();
    }
    repository.connection.execute(
        "INSERT INTO story_entity_names_v2(project_id, batch_id, entity_id, name) VALUES (?1, 'impact-batch', 'event-a', 'event-a')",
        [project_id.as_str()],
    ).unwrap();
    for (index, (from, to, kind, _)) in relations.iter().enumerate() {
        repository.connection.execute(
            "INSERT INTO story_relation_claims_v1(project_id, batch_id, assertion_index, subject_id, target_id, relation_type, assertion_json) VALUES (?1, 'impact-batch', ?2, ?3, ?4, ?5, ?6)",
            params![project_id.as_str(), index as i64, from, to, kind, assertions[index].to_string()],
        ).unwrap();
    }
    let report = repository
        .analyze_explicit_impact(ImpactAnalysisQuery {
            project_id: &project_id,
            scope: ImpactAnalysisScope::Current,
            changed_entity_id: "event-a",
            world_time: Some(2),
            max_hops: 2,
            limit: 20,
            revision_pair: None,
        })
        .unwrap();
    let findings = report["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 4);
    assert_eq!(
        findings
            .iter()
            .filter(|finding| finding["affectedEntityId"] == "event-b")
            .count(),
        2
    );
    assert_eq!(
        findings
            .iter()
            .filter(|finding| finding["affectedEntityId"] == "event-c")
            .count(),
        2
    );
    assert!(
        findings
            .iter()
            .all(|finding| finding["classification"] == "possibleImpact")
    );
    assert!(
        findings
            .iter()
            .all(|finding| finding["affectedEntityId"] != "place-l")
    );
    assert_eq!(report["coverage"]["explicitGraphComplete"], true);
    assert_eq!(report["coverage"]["globalImpactComplete"], false);
    assert_eq!(
        report["literalTextCandidates"][0]["sourceObjectId"],
        "draft-impact"
    );
    assert_eq!(
        report["literalTextCandidates"][0]["evidence"]["quote"],
        "event-a"
    );
    assert_eq!(
        report["coverage"]["literalSearch"]["literalNameSearchComplete"],
        true
    );
    let capped = repository
        .analyze_explicit_impact(ImpactAnalysisQuery {
            project_id: &project_id,
            scope: ImpactAnalysisScope::Current,
            changed_entity_id: "event-a",
            world_time: Some(2),
            max_hops: 2,
            limit: 2,
            revision_pair: None,
        })
        .unwrap();
    assert_eq!(capped["coverage"]["truncatedPaths"], true);
    assert_eq!(capped["coverage"]["explicitGraphComplete"], false);
    let unlinked_content =
        json!({"body":"event-a returned to harbor without any modeled relation"});
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES ('unlinked-revision', ?1, 'draft-unlinked', 'draft', 'unlinked-digest', ?2)",
        params![project_id.as_str(), unlinked_content.to_string()],
    ).unwrap();
    crate::search_index::index_revision(
        &repository.connection,
        &RevisionRecord {
            project_id: project_id.clone(),
            object_id: ObjectId::parse("draft-unlinked").unwrap(),
            object_kind: "draft".into(),
            revision_id: RevisionId::parse("unlinked-revision").unwrap(),
            parent_revision_id: None,
            content_digest: "unlinked-digest".into(),
            content_json: unlinked_content.to_string(),
        },
    )
    .unwrap();
    repository.connection.execute(
        "INSERT INTO story_object_heads_v2(project_id, object_id, object_kind, candidate_revision_id) VALUES (?1, 'draft-unlinked', 'draft', 'unlinked-revision')",
        [project_id.as_str()],
    ).unwrap();
    let unlinked = repository
        .analyze_explicit_impact(ImpactAnalysisQuery {
            project_id: &project_id,
            scope: ImpactAnalysisScope::Current,
            changed_entity_id: "event-a",
            world_time: Some(2),
            max_hops: 2,
            limit: 20,
            revision_pair: None,
        })
        .unwrap();
    assert!(
        unlinked["literalTextCandidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|candidate| candidate["sourceObjectId"] == "draft-unlinked")
    );
    assert_eq!(unlinked["coverage"]["explicitGraphComplete"], false);
    assert_eq!(
        unlinked["coverage"]["graphCoverage"]["pendingSourceCount"],
        1
    );
    let frozen = repository
        .analyze_explicit_impact(ImpactAnalysisQuery {
            project_id: &project_id,
            scope: ImpactAnalysisScope::Snapshot(&initial_snapshot),
            changed_entity_id: "event-a",
            world_time: Some(2),
            max_hops: 2,
            limit: 20,
            revision_pair: None,
        })
        .unwrap();
    assert_eq!(frozen["findings"], json!([]));
    assert_eq!(frozen["literalTextCandidates"], json!([]));
    assert_eq!(frozen["baseSnapshotId"], initial_snapshot.as_str());
    let other_snapshot = SnapshotId::parse("other-project-snapshot").unwrap();
    repository
        .create_project(
            ProjectRecord {
                project_id: ProjectId::parse("other-impact-project").unwrap(),
                title: "另一作品".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            other_snapshot.clone(),
        )
        .unwrap();
    assert!(matches!(
        repository.analyze_explicit_impact(ImpactAnalysisQuery {
            project_id: &project_id,
            scope: ImpactAnalysisScope::Snapshot(&other_snapshot),
            changed_entity_id: "event-a",
            world_time: Some(2),
            max_hops: 2,
            limit: 20,
            revision_pair: None,
        }),
        Err(RepositoryError::CandidateNotFound)
    ));
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES ('impact-revised', ?1, 'draft-impact', 'draft', 'new-digest', ?2)",
        params![project_id.as_str(), json!({"body":"different story"}).to_string()],
    ).unwrap();
    repository.connection.execute(
        "UPDATE story_object_heads_v2 SET candidate_revision_id = 'impact-revised' WHERE project_id = ?1 AND object_id = 'draft-impact'",
        [project_id.as_str()],
    ).unwrap();
    let stale = repository
        .analyze_explicit_impact(ImpactAnalysisQuery {
            project_id: &project_id,
            scope: ImpactAnalysisScope::Current,
            changed_entity_id: "event-a",
            world_time: Some(2),
            max_hops: 2,
            limit: 20,
            revision_pair: None,
        })
        .unwrap();
    assert_eq!(stale["findings"], json!([]));
    assert_eq!(stale["literalTextCandidates"], json!([]));
    assert_eq!(stale["coverage"]["explicitGraphComplete"], false);
    let from_revision_id = RevisionId::parse("impact-revision").unwrap();
    let to_revision_id = RevisionId::parse("impact-revised").unwrap();
    let changed = repository
        .analyze_explicit_impact(ImpactAnalysisQuery {
            project_id: &project_id,
            scope: ImpactAnalysisScope::Current,
            changed_entity_id: "event-a",
            world_time: Some(2),
            max_hops: 2,
            limit: 20,
            revision_pair: Some(ImpactRevisionPair {
                from_revision_id: &from_revision_id,
                to_revision_id: &to_revision_id,
            }),
        })
        .unwrap();
    assert_eq!(changed["revisionDelta"]["objectId"], "draft-impact");
    assert_eq!(changed["revisionDelta"]["bodyChanged"], true);
    assert_eq!(
        changed["revisionDelta"]["scopeBinding"],
        "currentToRevision"
    );
    assert_eq!(
        changed["revisionDelta"]["coverage"]["semanticChangeInferred"],
        false
    );
    assert_eq!(
        changed["revisionDelta"]["modelCandidateDelta"]["status"],
        "unavailable"
    );
    assert_eq!(
        changed["revisionDelta"]["modelCandidateDelta"]["coverage"]["to"],
        "missingPublishedBatch"
    );
    assert!(matches!(
        repository.analyze_explicit_impact(ImpactAnalysisQuery {
            project_id: &project_id,
            scope: ImpactAnalysisScope::Current,
            changed_entity_id: "event-a",
            world_time: Some(2),
            max_hops: 2,
            limit: 20,
            revision_pair: Some(ImpactRevisionPair {
                from_revision_id: &to_revision_id,
                to_revision_id: &from_revision_id,
            }),
        }),
        Err(RepositoryError::InvalidContent(_))
    ));
}

use crate::{ImpactAnalysisRepository, ProjectRevisionRepository};
