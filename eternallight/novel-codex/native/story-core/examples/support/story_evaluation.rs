use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use novel_codex_story_core::{
    ImpactAnalysisQuery, ImpactAnalysisScope, ProjectId, SearchQuery, SearchScope, SnapshotId,
    SqliteStoryRepository,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Dataset {
    dataset_id: String,
    revision: String,
    project_id: String,
    annotation: Annotation,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Annotation {
    kind: AnnotationKind,
    reviewer: String,
    reference: String,
}

#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum AnnotationKind {
    EngineeringFixture,
    HumanReviewed,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Case {
    id: String,
    category: String,
    scenario: u8,
    query: Query,
}

#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Query {
    Evidence {
        text: String,
        snapshot_id: Option<String>,
        expected: Vec<Anchor>,
    },
    Impact {
        entity_id: String,
        snapshot_id: Option<String>,
        world_time: Option<i64>,
        max_hops: usize,
        expected: Vec<ImpactTarget>,
    },
}

#[derive(Deserialize, serde::Serialize, Clone, Eq, PartialEq, Ord, PartialOrd)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum ImpactTarget {
    Entity { id: String },
    Text { object_id: String },
}

#[derive(Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Anchor {
    object_id: String,
    revision_id: String,
    path: String,
    start: usize,
    end: usize,
    quote: String,
}

pub fn parse_dataset(bytes: &[u8]) -> Result<Dataset> {
    if bytes.len() > 1_048_576 {
        return Err("annotation byte limit exceeded".into());
    }
    let data: Dataset = serde_json::from_slice(bytes)?;
    for label in [
        &data.dataset_id,
        &data.revision,
        &data.project_id,
        &data.annotation.reviewer,
        &data.annotation.reference,
    ] {
        label_valid(label)?;
    }
    if data.cases.is_empty() || data.cases.len() > 256 {
        return Err("invalid case count".into());
    }
    let mut ids = BTreeSet::new();
    for case in &data.cases {
        label_valid(&case.id)?;
        label_valid(&case.category)?;
        if !ids.insert(&case.id) || !(1..=24).contains(&case.scenario) {
            return Err("duplicate case or invalid scenario".into());
        }
        match &case.query {
            Query::Evidence {
                text,
                snapshot_id,
                expected,
            } => {
                if text.trim().is_empty() || text.chars().count() > 240 {
                    return Err("invalid evidence query".into());
                }
                if let Some(snapshot) = snapshot_id {
                    SnapshotId::parse(snapshot)?;
                }
                expected_valid(expected)?;
                for anchor in expected {
                    label_valid(&anchor.object_id)?;
                    label_valid(&anchor.revision_id)?;
                    if !anchor.path.starts_with('/')
                        || anchor.path.len() > 512
                        || anchor.start >= anchor.end
                        || anchor.quote.is_empty()
                        || anchor.quote.len() > 9_000
                        || anchor.quote.chars().count() != anchor.end - anchor.start
                    {
                        return Err("invalid expected anchor".into());
                    }
                }
            }
            Query::Impact {
                entity_id,
                snapshot_id,
                max_hops,
                expected,
                ..
            } => {
                label_valid(entity_id)?;
                if let Some(snapshot) = snapshot_id {
                    SnapshotId::parse(snapshot)?;
                }
                if !(1..=3).contains(max_hops) {
                    return Err("invalid impact hops".into());
                }
                expected_valid(expected)?;
                for target in expected {
                    label_valid(match target {
                        ImpactTarget::Entity { id } => id,
                        ImpactTarget::Text { object_id } => object_id,
                    })?;
                }
            }
        }
    }
    Ok(data)
}

fn label_valid(label: &str) -> Result<()> {
    if label.trim().is_empty() || label.len() > 160 || label.chars().any(char::is_control) {
        return Err("invalid annotation identity".into());
    }
    Ok(())
}

fn expected_valid<T: Ord>(expected: &[T]) -> Result<()> {
    if expected.is_empty()
        || expected.len() > 64
        || expected.iter().collect::<BTreeSet<_>>().len() != expected.len()
    {
        return Err("empty, oversized or duplicate expected set".into());
    }
    Ok(())
}

// Both gold and returned evidence must belong to the requested selected source, not just exist.
fn valid_anchor(
    connection: &Connection,
    project: &str,
    snapshot: Option<&str>,
    anchor: &Anchor,
) -> Result<bool> {
    let content: Option<String> = connection.query_row(
        "SELECT r.content_json FROM story_revisions_v2 r WHERE r.project_id = ?1 AND r.object_id = ?2 AND r.revision_id = ?3 AND r.object_kind = 'draft' AND ((?4 IS NULL AND EXISTS (SELECT 1 FROM story_object_heads_v2 h WHERE h.project_id = r.project_id AND h.object_id = r.object_id AND COALESCE(h.candidate_revision_id, h.accepted_revision_id) = r.revision_id)) OR (?4 IS NOT NULL AND EXISTS (SELECT 1 FROM story_snapshot_members_v2 m JOIN story_snapshots_v2 s ON s.snapshot_id = m.snapshot_id WHERE s.project_id = r.project_id AND s.branch_id = 'main' AND m.snapshot_id = ?4 AND m.object_id = r.object_id AND m.revision_id = r.revision_id)))",
        params![project, anchor.object_id, anchor.revision_id, snapshot], |row| row.get(0),
    ).optional()?;
    let Some(content) = content else {
        return Ok(false);
    };
    let value: Value = serde_json::from_str(&content)?;
    let Some(text) = value.pointer(&anchor.path).and_then(Value::as_str) else {
        return Ok(false);
    };
    Ok(anchor.start < anchor.end
        && anchor.end <= text.chars().count()
        && text
            .chars()
            .skip(anchor.start)
            .take(anchor.end - anchor.start)
            .collect::<String>()
            == anchor.quote)
}

fn returned_anchor(hit: &Value, object_field: &str, path: Option<&str>) -> Result<Anchor> {
    Ok(Anchor {
        object_id: hit[object_field]
            .as_str()
            .ok_or("missing source object")?
            .into(),
        revision_id: hit["sourceRevisionId"]
            .as_str()
            .ok_or("missing source revision")?
            .into(),
        path: path.ok_or("missing source path")?.into(),
        start: usize::try_from(hit["evidence"]["start"].as_u64().ok_or("missing start")?)?,
        end: usize::try_from(hit["evidence"]["end"].as_u64().ok_or("missing end")?)?,
        quote: hit["evidence"]["quote"]
            .as_str()
            .ok_or("missing quote")?
            .into(),
    })
}

fn covers(actual: &Anchor, expected: &Anchor) -> bool {
    actual.object_id == expected.object_id
        && actual.revision_id == expected.revision_id
        && actual.path == expected.path
        && actual.start <= expected.start
        && actual.end >= expected.end
}

// Coverage is a numeric/boolean measurement, never a passthrough for names or prose.
fn coverage_metrics(value: &Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(_, value)| {
                    matches!(
                        value,
                        Value::Object(_) | Value::Bool(_) | Value::Number(_) | Value::Null
                    )
                })
                .map(|(key, value)| (key.clone(), coverage_metrics(value)))
                .collect(),
        ),
        Value::Bool(_) | Value::Number(_) | Value::Null => value.clone(),
        Value::Array(_) | Value::String(_) => Value::Null,
    }
}

#[derive(Default)]
struct Counts {
    expected: usize,
    recalled: usize,
    queries: usize,
    errors: usize,
    invalid_anchors: usize,
}

impl Counts {
    fn report(&self) -> Value {
        let recall = (self.expected > 0).then(|| self.recalled as f64 / self.expected as f64);
        json!({"expected":self.expected,"recalled":self.recalled,"queries":self.queries,"queryErrors":self.errors,
            "invalidReturnedAnchors":self.invalid_anchors,"recall":recall,
            "thresholdMet":recall.map(|value| value >= 0.9 && self.errors == 0 && self.invalid_anchors == 0)})
    }
}

pub fn measure(
    repository: &SqliteStoryRepository,
    captured: &Connection,
    data: &Dataset,
    bytes: &[u8],
    corpus_digest: &str,
) -> Result<Value> {
    let project = ProjectId::parse(&data.project_id)?;
    if repository.project(&project)?.is_none() {
        return Err("annotation project not found".into());
    }
    let mut totals = BTreeMap::from([
        ("evidence", Counts::default()),
        ("impact", Counts::default()),
    ]);
    let mut categories: BTreeMap<&str, BTreeMap<&str, Counts>> = BTreeMap::new();
    let mut cases = Vec::new();
    for case in &data.cases {
        let started = Instant::now();
        let mut invalid_anchors = 0;
        let (channel, expected_count, missed, response) = match &case.query {
            Query::Evidence {
                text,
                snapshot_id,
                expected,
            } => {
                for anchor in expected {
                    if !valid_anchor(captured, &data.project_id, snapshot_id.as_deref(), anchor)? {
                        return Err("gold anchor is invalid or outside its scope".into());
                    }
                }
                let snapshot = snapshot_id.as_ref().map(SnapshotId::parse).transpose()?;
                let response = repository.search_documents(SearchQuery {
                    project_id: &project,
                    snapshot_id: snapshot.as_ref(),
                    text,
                    kind: Some("draft"),
                    scope: SearchScope::All,
                    limit: 20,
                    cursor: None,
                });
                let mut found = BTreeSet::new();
                if let Ok(report) = &response {
                    for hit in report["results"]
                        .as_array()
                        .ok_or("missing evidence results")?
                    {
                        let anchor = returned_anchor(hit, "id", hit["sourcePath"].as_str());
                        match anchor {
                            Ok(anchor)
                                if valid_anchor(
                                    captured,
                                    &data.project_id,
                                    snapshot_id.as_deref(),
                                    &anchor,
                                )? =>
                            {
                                found.insert(anchor);
                            }
                            _ => invalid_anchors += 1,
                        }
                    }
                }
                let missed = expected.iter().filter(|gold| !found.iter().any(|hit| covers(hit, gold)))
                    .map(|anchor| json!({"objectId":anchor.object_id,"revisionId":anchor.revision_id,"path":anchor.path,"start":anchor.start,"end":anchor.end})).collect::<Vec<_>>();
                ("evidence", expected.len(), missed, response)
            }
            Query::Impact {
                entity_id,
                snapshot_id,
                world_time,
                max_hops,
                expected,
            } => {
                let snapshot = snapshot_id.as_ref().map(SnapshotId::parse).transpose()?;
                let response = repository.analyze_explicit_impact(ImpactAnalysisQuery {
                    project_id: &project,
                    scope: snapshot
                        .as_ref()
                        .map_or(ImpactAnalysisScope::Current, ImpactAnalysisScope::Snapshot),
                    changed_entity_id: entity_id,
                    world_time: *world_time,
                    max_hops: *max_hops,
                    limit: 50,
                    revision_pair: None,
                });
                let mut found = BTreeSet::new();
                if let Ok(report) = &response {
                    for finding in report["findings"]
                        .as_array()
                        .ok_or("missing impact findings")?
                    {
                        let edges = finding["evidencePath"]
                            .as_array()
                            .ok_or("missing impact path")?;
                        let mut valid = !edges.is_empty();
                        for edge in edges {
                            let anchor = returned_anchor(edge, "sourceObjectId", Some("/body"));
                            match anchor {
                                Ok(anchor)
                                    if valid_anchor(
                                        captured,
                                        &data.project_id,
                                        snapshot_id.as_deref(),
                                        &anchor,
                                    )? => {}
                                _ => {
                                    invalid_anchors += 1;
                                    valid = false;
                                }
                            }
                        }
                        if valid {
                            found.insert(ImpactTarget::Entity {
                                id: finding["affectedEntityId"]
                                    .as_str()
                                    .ok_or("missing impact target")?
                                    .into(),
                            });
                        }
                    }
                    for hit in report["literalTextCandidates"]
                        .as_array()
                        .ok_or("missing impact text")?
                    {
                        let anchor =
                            returned_anchor(hit, "sourceObjectId", hit["sourcePath"].as_str());
                        match anchor {
                            Ok(anchor)
                                if valid_anchor(
                                    captured,
                                    &data.project_id,
                                    snapshot_id.as_deref(),
                                    &anchor,
                                )? =>
                            {
                                found.insert(ImpactTarget::Text {
                                    object_id: anchor.object_id,
                                });
                            }
                            _ => invalid_anchors += 1,
                        }
                    }
                }
                let missed = expected
                    .iter()
                    .filter(|target| !found.contains(*target))
                    .map(|target| serde_json::to_value(target).expect("serializable target"))
                    .collect();
                ("impact", expected.len(), missed, response)
            }
        };
        let recalled = expected_count - missed.len();
        let failed = usize::from(response.is_err());
        for count in [
            totals.get_mut(channel).expect("known channel"),
            categories
                .entry(&case.category)
                .or_default()
                .entry(channel)
                .or_default(),
        ] {
            count.expected += expected_count;
            count.recalled += recalled;
            count.queries += 1;
            count.errors += failed;
            count.invalid_anchors += invalid_anchors;
        }
        cases.push(json!({"id":case.id,"category":case.category,"scenario":case.scenario,"channel":channel,
            "expected":expected_count,"recalled":recalled,"missed":missed,"queryFailed":failed > 0,"invalidReturnedAnchors":invalid_anchors,
            "elapsedMs":started.elapsed().as_secs_f64()*1000.0,"coverage":response.ok().map(|report| coverage_metrics(&report["coverage"]))}));
    }
    let category_reports = categories.into_iter().map(|(category, counts)| json!({"category":category,
        "channels":counts.into_iter().map(|(channel,count)| (channel,count.report())).collect::<BTreeMap<_,_>>()})).collect::<Vec<_>>();
    let thresholds_met = totals
        .values()
        .all(|count| count.report()["thresholdMet"] == true)
        && category_reports.iter().all(|category| {
            category["channels"]
                .as_object()
                .expect("channels object")
                .values()
                .all(|count| count["thresholdMet"] == true)
        });
    Ok(
        json!({"datasetId":data.dataset_id,"datasetRevision":data.revision,"datasetSha256":format!("{:x}",Sha256::digest(bytes)),
        "corpusCaptureSha256":corpus_digest,"projectId":data.project_id,
        "annotationClaim":{"kind":data.annotation.kind,"reviewer":data.annotation.reviewer,"reference":data.annotation.reference,"independentlyVerified":false},
        "method":{"evidence":"productionLiteralSearchAt20","impact":"productionExplicitPathsAndLiteralMentions","threshold":0.9},
        "channels":totals.into_iter().map(|(channel,count)| (channel,count.report())).collect::<BTreeMap<_,_>>(),
        "categories":category_reports,"cases":cases,"referencedScenarios":data.cases.iter().map(|case| case.scenario).collect::<BTreeSet<_>>(),
        "measuredThresholdsMet":thresholds_met,"fullArchitectureAcceptanceVerified":false}),
    )
}

use novel_codex_story_core::{ImpactAnalysisRepository, ProjectRevisionRepository, SearchIndexRepository};
