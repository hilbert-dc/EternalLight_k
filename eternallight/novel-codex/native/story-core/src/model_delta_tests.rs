use pretty_assertions::assert_eq;
use rusqlite::params;
use serde_json::{Value, json};

use crate::{ProjectId, RevisionId, SqliteStoryRepository};

fn batch(
    repository: &SqliteStoryRepository,
    project: &ProjectId,
    id: &str,
    source: &str,
    status: &str,
    published_at: Option<i64>,
    assertions: &[Value],
) {
    repository.connection.execute(
        "INSERT INTO story_modeling_batches_v2(batch_id, project_id, object_id, source_revision_id, input_project_revision, extractor_version, status, assertions_json, entities_json, created_at, published_at) VALUES (?1, ?2, 'chapter', ?3, 0, ?1, ?4, ?5, '[]', 1, ?6)",
        params![id, project.as_str(), source, status, json!(assertions).to_string(), published_at],
    ).unwrap();
}

fn revision(repository: &SqliteStoryRepository, project: &ProjectId, id: &str) {
    repository.connection.execute(
        "INSERT INTO story_revisions_v2(revision_id, project_id, object_id, object_kind, content_digest, content_json) VALUES (?1, ?2, 'chapter', 'draft', 'digest', '{\"body\":\"test\"}')",
        params![id, project.as_str()],
    ).unwrap();
}

#[test]
fn event_candidate_delta_includes_all_changed_participant_identities() {
    let repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("event-delta-project").unwrap();
    repository.connection.execute(
        "INSERT INTO story_projects_v2(project_id,title,logline,created_at,updated_at) VALUES(?1,'event','',1,1)",
        [project.as_str()],
    ).unwrap();
    let before = RevisionId::parse("event-before").unwrap();
    let after = RevisionId::parse("event-after").unwrap();
    revision(&repository, &project, before.as_str());
    revision(&repository, &project, after.as_str());
    let old = json!({"kind":"event","subjectId":"delivery","predicate":"transfer","storyTime":1,
        "value":{"participants":[{"role":"actor","entityId":"a"},{"role":"recipient","entityId":"b"},{"role":"item","entityId":"letter"}],
            "preconditions":[],"effects":[],"untilExclusive":null},"evidence":{"quote":"甲交信","start":0,"end":3}});
    let mut new = old.clone();
    new["value"]["participants"][1]["entityId"] = json!("c");
    batch(
        &repository,
        &project,
        "event-old",
        before.as_str(),
        "stale",
        Some(2),
        &[old],
    );
    batch(
        &repository,
        &project,
        "event-new",
        after.as_str(),
        "ready",
        Some(3),
        &[new],
    );
    let entities = [
        ("delivery", "event"),
        ("a", "character"),
        ("b", "character"),
        ("c", "character"),
        ("letter", "item"),
    ]
    .map(|(id, kind)| {
        json!({"id":id,"kind":kind,"displayName":id,"aliases":[],
        "evidence":{"quote":"甲交信","start":0,"end":3}})
    });
    repository
        .connection
        .execute(
            "UPDATE story_revisions_v2 SET content_json=?1 WHERE project_id=?2",
            params![json!({"body":"甲交信"}).to_string(), project.as_str()],
        )
        .unwrap();
    repository
        .connection
        .execute(
            "UPDATE story_modeling_batches_v2 SET entities_json=?1 WHERE project_id=?2",
            params![json!(entities).to_string(), project.as_str()],
        )
        .unwrap();
    let delta = repository
        .draft_model_candidate_delta(&project, "chapter", &before, &after)
        .unwrap();
    assert_eq!(
        delta["candidateChangedEntityIds"],
        json!(["a", "b", "c", "delivery", "letter"])
    );
    assert_eq!(delta["coverage"]["semanticTruthInferred"], false);
    repository.connection.execute("UPDATE story_modeling_batches_v2 SET entities_json=json_set(entities_json,'$[1].kind','item') WHERE batch_id='event-new'",[]).unwrap();
    assert_eq!(
        repository.draft_model_candidate_delta(&project, "chapter", &before, &after),
        Err(crate::RepositoryError::CorruptState)
    );
}

#[test]
fn candidate_delta_requires_both_unique_published_sources() {
    let repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("model-delta-project").unwrap();
    repository.connection.execute(
        "INSERT INTO story_projects_v2(project_id, title, logline, created_at, updated_at) VALUES (?1, 'test', '', 1, 1)",
        [project.as_str()],
    ).unwrap();
    let before = RevisionId::parse("model-delta-before").unwrap();
    let after = RevisionId::parse("model-delta-after").unwrap();
    revision(&repository, &project, before.as_str());
    revision(&repository, &project, after.as_str());
    let old = json!({"kind":"assertion","subjectId":"letter","predicate":"fate",
        "value":"burned","evidence":{"quote":"烧信","start":0,"end":2}});
    let new = json!({"kind":"assertion","subjectId":"letter","predicate":"fate",
        "value":"hidden","evidence":{"quote":"藏信","start":0,"end":2}});
    batch(
        &repository,
        &project,
        "old-model",
        before.as_str(),
        "stale",
        Some(2),
        &[old],
    );
    batch(
        &repository,
        &project,
        "pending-model",
        after.as_str(),
        "queued",
        None,
        &[],
    );
    let pending = repository
        .draft_model_candidate_delta(&project, "chapter", &before, &after)
        .unwrap();
    assert_eq!(pending["status"], "unavailable");
    assert_eq!(pending["candidateChanges"], json!([]));
    assert_eq!(pending["coverage"]["to"], "pendingPublication");

    batch(
        &repository,
        &project,
        "new-model",
        after.as_str(),
        "ready",
        Some(3),
        &[new],
    );
    let result = repository
        .draft_model_candidate_delta(&project, "chapter", &before, &after)
        .unwrap();
    assert_eq!(result["status"], "comparedCandidates");
    assert_eq!(result["coverage"]["complete"], true);
    assert_eq!(result["coverage"]["semanticTruthInferred"], false);
    assert_eq!(result["candidateChangedEntityIds"], json!(["letter"]));
    assert_eq!(result["candidateChanges"].as_array().unwrap().len(), 2);
    assert_eq!(result["candidateChanges"][0]["change"], "noLongerExtracted");
    assert_eq!(
        result["candidateChanges"][0]["sourceRevisionId"],
        before.as_str()
    );
    assert_eq!(result["candidateChanges"][0]["evidence"]["quote"], "烧信");
    assert_eq!(result["candidateChanges"][1]["change"], "newlyExtracted");
    assert_eq!(
        result["candidateChanges"][1]["sourceRevisionId"],
        after.as_str()
    );
    assert_eq!(result["candidateChanges"][1]["evidence"]["quote"], "藏信");

    batch(
        &repository,
        &project,
        "new-model-2",
        after.as_str(),
        "stale",
        Some(4),
        &[],
    );
    let ambiguous = repository
        .draft_model_candidate_delta(&project, "chapter", &before, &after)
        .unwrap();
    assert_eq!(ambiguous["status"], "unavailable");
    assert_eq!(ambiguous["coverage"]["to"], "ambiguousPublishedBatches");
    assert_eq!(ambiguous["candidateChanges"], json!([]));
}

#[test]
fn changed_evidence_does_not_become_a_changed_claim() {
    let repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("model-evidence-project").unwrap();
    repository.connection.execute(
        "INSERT INTO story_projects_v2(project_id, title, logline, created_at, updated_at) VALUES (?1, 'test', '', 1, 1)",
        [project.as_str()],
    ).unwrap();
    let before = RevisionId::parse("model-evidence-before").unwrap();
    let after = RevisionId::parse("model-evidence-after").unwrap();
    revision(&repository, &project, before.as_str());
    revision(&repository, &project, after.as_str());
    let assertion = json!({"kind":"assertion","subjectId":"letter","predicate":"fate",
        "value":"burned","evidence":{"quote":"烧信","start":0,"end":2}});
    let mut reanchored = assertion.clone();
    reanchored["evidence"] = json!({"quote":"信被烧了","start":10,"end":14});
    batch(
        &repository,
        &project,
        "before-evidence",
        before.as_str(),
        "stale",
        Some(2),
        &[assertion],
    );
    batch(
        &repository,
        &project,
        "after-evidence",
        after.as_str(),
        "ready",
        Some(3),
        &[reanchored],
    );
    let result = repository
        .draft_model_candidate_delta(&project, "chapter", &before, &after)
        .unwrap();
    assert_eq!(result["candidateChanges"], json!([]));
    assert_eq!(result["coverage"]["evidenceReanchoredClaimCount"], 1);
}

#[test]
fn duplicate_claims_keep_the_correct_removed_evidence() {
    let repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("model-duplicate-project").unwrap();
    repository.connection.execute(
        "INSERT INTO story_projects_v2(project_id, title, logline, created_at, updated_at) VALUES (?1, 'test', '', 1, 1)",
        [project.as_str()],
    ).unwrap();
    let before = RevisionId::parse("model-duplicate-before").unwrap();
    let after = RevisionId::parse("model-duplicate-after").unwrap();
    revision(&repository, &project, before.as_str());
    revision(&repository, &project, after.as_str());
    let first = json!({"kind":"assertion","subjectId":"letter","predicate":"fate",
        "value":"burned","evidence":{"quote":"first","start":0,"end":5}});
    let mut second = first.clone();
    second["evidence"] = json!({"quote":"second","start":7,"end":13});
    batch(
        &repository,
        &project,
        "before-duplicate",
        before.as_str(),
        "stale",
        Some(2),
        &[first, second.clone()],
    );
    batch(
        &repository,
        &project,
        "after-duplicate",
        after.as_str(),
        "ready",
        Some(3),
        &[second],
    );
    let result = repository
        .draft_model_candidate_delta(&project, "chapter", &before, &after)
        .unwrap();
    assert_eq!(result["candidateChanges"].as_array().unwrap().len(), 1);
    assert_eq!(result["candidateChanges"][0]["evidence"]["quote"], "first");
    assert_eq!(result["coverage"]["evidenceReanchoredClaimCount"], 0);
}

#[test]
fn candidate_delta_reports_truncation_without_claiming_complete_coverage() {
    let repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("model-budget-project").unwrap();
    repository.connection.execute(
        "INSERT INTO story_projects_v2(project_id, title, logline, created_at, updated_at) VALUES (?1, 'test', '', 1, 1)",
        [project.as_str()],
    ).unwrap();
    let before = RevisionId::parse("model-budget-before").unwrap();
    let after = RevisionId::parse("model-budget-after").unwrap();
    revision(&repository, &project, before.as_str());
    revision(&repository, &project, after.as_str());
    let assertions = (0..31)
        .map(|index| {
            json!({
                "kind":"assertion", "subjectId":format!("event-{index}"),
                "predicate":"occurred", "value":true,
                "evidence":{"quote":"event","start":0,"end":5}
            })
        })
        .collect::<Vec<_>>();
    batch(
        &repository,
        &project,
        "before-budget",
        before.as_str(),
        "stale",
        Some(2),
        &assertions,
    );
    batch(
        &repository,
        &project,
        "after-budget",
        after.as_str(),
        "ready",
        Some(3),
        &[],
    );
    let result = repository
        .draft_model_candidate_delta(&project, "chapter", &before, &after)
        .unwrap();
    assert_eq!(result["candidateChanges"].as_array().unwrap().len(), 30);
    assert_eq!(result["coverage"]["totalCandidateChanges"], 31);
    assert_eq!(result["coverage"]["truncated"], true);
    assert_eq!(result["coverage"]["complete"], false);
}

#[test]
fn property_delta_keeps_typed_values_and_reports_both_entity_endpoints() {
    let repository = SqliteStoryRepository::in_memory().unwrap();
    let project = ProjectId::parse("property-delta").unwrap();
    repository.connection.execute("INSERT INTO story_projects_v2(project_id,title,logline,created_at,updated_at) VALUES (?1,'test','',1,1)",[project.as_str()]).unwrap();
    let before = RevisionId::parse("property-before").unwrap();
    let after = RevisionId::parse("property-after").unwrap();
    revision(&repository, &project, before.as_str());
    revision(&repository, &project, after.as_str());
    let old = json!({"kind":"state","subjectId":"linxi","predicate":"carried","value":{"entityId":"key"},
        "stateSchema":{"cardinality":"multiple","valueType":{"type":"entity","kind":"item"}},"validFrom":1,
        "evidence":{"quote":"钥匙","start":0,"end":2}});
    let mut new = old.clone();
    new["value"] = json!({"entityId":"book"});
    batch(
        &repository,
        &project,
        "property-old",
        before.as_str(),
        "stale",
        Some(2),
        &[old],
    );
    batch(
        &repository,
        &project,
        "property-new",
        after.as_str(),
        "ready",
        Some(3),
        &[new.clone()],
    );
    let delta = repository
        .draft_model_candidate_delta(&project, "chapter", &before, &after)
        .unwrap();
    assert_eq!(
        delta["candidateChangedEntityIds"],
        json!(["book", "key", "linxi"])
    );
    assert_eq!(delta["candidateChanges"].as_array().unwrap().len(), 2);
    assert_eq!(
        delta["candidateChanges"][0]["claim"]["stateSchema"],
        new["stateSchema"]
    );
    assert_eq!(delta["coverage"]["semanticTruthInferred"], false);
}
