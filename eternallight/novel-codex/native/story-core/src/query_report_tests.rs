use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::{fit_report, step_report};
use crate::query_plan::QueryStep;

enum Polarity {
    Positive,
    Negative,
}

fn claim(revision: &str, text: &str, polarity: Polarity) -> Value {
    let value = matches!(polarity, Polarity::Positive);
    json!({"sourceObjectId":"chapter-1","sourceRevisionId":revision,"confirmationState":"candidate",
        "assertion":{"kind":"assertion","value":value,"evidence":{"quote":text,"start":0,"end":text.chars().count()}}})
}

fn step(id: &str, rows: Vec<Value>, findings: Vec<Value>) -> Value {
    let definition: QueryStep = serde_json::from_value(
        json!({"id":id,"operation":{"type":"state","entityId":"person-0","perspective":"author"}}),
    )
    .unwrap();
    step_report(
        &definition,
        json!({"entries":rows,"findings":findings,"coverage":{"complete":true}}),
    )
    .unwrap()
}

fn report(steps: Vec<Value>) -> Value {
    let plan_steps = steps
        .iter()
        .map(|step| json!({"id":step["stepId"],"operation":{"type":"state"}}))
        .collect::<Vec<_>>();
    json!({"plan":{"question":"白塔秘密","steps":plan_steps},"steps":steps,
        "coverage":{"complete":true,"semanticCompleteness":"unknown","responseTruncated":false},"contentHash":""})
}

#[test]
fn shared_sources_do_not_merge_conflicting_claims_or_distinct_revisions() {
    let mut report = report(vec![
        step(
            "world",
            vec![
                claim("r1", "白塔秘密", Polarity::Positive),
                claim("r1", "白塔秘密", Polarity::Negative),
            ],
            vec![],
        ),
        step(
            "other",
            vec![
                claim("r1", "白塔秘密", Polarity::Positive),
                claim("r2", "白塔秘密", Polarity::Positive),
            ],
            vec![],
        ),
    ]);
    fit_report(&mut report, /*budget*/ 9000).unwrap();
    assert_eq!(report["evidence"].as_object().unwrap().len(), 2);
    let left = &report["steps"][0]["results"];
    assert_eq!(
        left[0]["assertion"]["evidenceRef"],
        left[1]["assertion"]["evidenceRef"]
    );
    assert_eq!(
        (
            left[0]["assertion"]["value"].clone(),
            left[1]["assertion"]["value"].clone()
        ),
        (json!(true), json!(false))
    );
    assert_eq!(left[0]["confirmationState"], "candidate");
    assert_eq!(left[0]["assertion"].get("evidence"), None);
    assert_ne!(
        left[0]["assertion"]["evidenceRef"],
        report["steps"][1]["results"][1]["assertion"]["evidenceRef"]
    );
    assert_eq!(report["coverage"]["complete"], true);
}

#[test]
fn effect_assessments_share_the_query_budget_and_oversized_rows_are_omitted_whole() {
    let event = json!({"value":{"effects":(0..16).map(|index| json!({"sourceBatchId":null,"assertionIndex":index})).collect::<Vec<_>>()}});
    let timings = json!({"effects":(0..16).map(|index| json!({"referenceIndex":index,"timing":"atEvent"})).collect::<Vec<_>>()});
    let confirmations = (0..16)
        .map(|index| {
            (
                ("batch".into(), index),
                format!("{index:03}{}", "f".repeat(157)),
            )
        })
        .collect();
    let assessment = crate::event_evolution::assess(
        &event,
        "batch",
        &timings,
        &json!({"status":"interpreted","result":"both"}),
        &confirmations,
    )
    .unwrap();
    let mut large = claim("event-revision", &"🌊".repeat(1600), Polarity::Positive);
    large["assertion"]["kind"] = json!("event");
    large["assertion"]["value"] = event["value"].clone();
    large["eventEvolutionAssessment"] = assessment;
    let mut report = report(vec![step(
        "events",
        vec![
            large,
            claim("small-revision", "白塔秘密", Polarity::Positive),
        ],
        vec![],
    )]);
    fit_report(&mut report, /*budget*/ 9000).unwrap();
    assert!(report.to_string().len() <= 9000);
    assert_eq!(report["steps"][0]["results"].as_array().unwrap().len(), 1);
    assert_eq!(
        report["steps"][0]["results"][0]["sourceRevisionId"],
        "small-revision"
    );
    assert_eq!(report["steps"][0]["coverage"]["requeryRequired"], true);
    assert_eq!(report["coverage"]["complete"], false);
    assert_eq!(report["coverage"]["responseTruncated"], true);
    let remaining = report["evidence"]
        .as_object()
        .unwrap()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        remaining,
        vec![
            json!({"sourceObjectId":"chapter-1","sourceRevisionId":"small-revision","sourcePath":"/body","quote":"白塔秘密","start":0,"end":4})
        ]
    );
}

#[test]
fn sorting_rebinds_findings_and_keeps_end_anchors_separate() {
    let mut first = claim("r1", "无关", Polarity::Positive);
    first["assertion"]["epistemicUntilEvidence"] = json!({"quote":"忘记","start":4,"end":6});
    let second = claim("r2", "白塔秘密", Polarity::Negative);
    let finding = json!({"leftEntryIndex":0,"rightEntryIndex":1,"leftEvidence":first["assertion"]["evidence"],
        "rightEvidence":second["assertion"]["evidence"],"type":"polarityConflict","confirmationState":"candidateConflict"});
    let original = step("state", vec![first, second], vec![finding]);
    let ids = (
        original["results"][0]["resultId"].clone(),
        original["results"][1]["resultId"].clone(),
    );
    let mut report = report(vec![original]);
    fit_report(&mut report, /*budget*/ 9000).unwrap();
    let step = &report["steps"][0];
    assert_eq!(step["results"][0]["resultId"], ids.1);
    assert_eq!(
        (
            step["findings"][0]["leftResultId"].clone(),
            step["findings"][0]["rightResultId"].clone()
        ),
        ids
    );
    assert_eq!(
        step["findings"][0]["leftEvidenceRef"],
        step["results"][1]["assertion"]["evidenceRef"]
    );
    assert_eq!(report["evidence"].as_object().unwrap().len(), 3);
    let end = step["results"][1]["assertion"]["epistemicUntilEvidenceRef"]
        .as_str()
        .unwrap();
    assert_eq!(report["evidence"][end]["quote"], "忘记");
    assert_eq!(step["findings"][0].get("leftEntryIndex"), None);
}

#[test]
fn budget_retains_later_channels_and_removes_orphan_references() {
    let text = "无关".repeat(100);
    let mut many = (0..8)
        .map(|index| claim(&format!("r{index}"), &text, Polarity::Positive))
        .collect::<Vec<_>>();
    many.push(claim("relevant", "白塔秘密", Polarity::Positive));
    let finding = json!({"leftEntryIndex":0,"rightEntryIndex":8,"leftEvidence":many[0]["assertion"]["evidence"],
        "rightEvidence":many[8]["assertion"]["evidence"],"type":"polarityConflict"});
    let mut report = report(vec![
        step("early", many, vec![finding]),
        step(
            "late",
            vec![claim("later", "白塔秘密", Polarity::Negative)],
            vec![],
        ),
    ]);
    report["steps"][0]["nextCursor"] = json!("backend-after-nine");
    fit_report(&mut report, /*budget*/ 4000).unwrap();
    assert!(report.to_string().len() <= 4000);
    assert_eq!(report["steps"][1]["returnedItems"], 1);
    assert_eq!(
        report["steps"][0]["results"][0]["sourceRevisionId"],
        "relevant"
    );
    assert_eq!(report["steps"][0]["nextCursor"], Value::Null);
    assert_eq!(report["steps"][0]["coverage"]["requeryRequired"], true);
    assert_eq!(report["coverage"]["complete"], false);
    assert_eq!(report["steps"][0]["findings"], json!([]));
    let dictionary = report["evidence"].as_object().unwrap();
    let references = report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|step| step["results"].as_array().unwrap())
        .map(|row| row["assertion"]["evidenceRef"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        dictionary
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        references
    );
}

#[test]
fn fragment_cap_and_hash_are_deterministic() {
    let rows = (0..45)
        .map(|index| claim(&format!("r{index}"), "白塔秘密", Polarity::Positive))
        .collect();
    let original = report(vec![step("state", rows, vec![])]);
    let mut report = original.clone();
    fit_report(&mut report, /*budget*/ 100_000).unwrap();
    assert_eq!(report["evidence"].as_object().unwrap().len(), 40);
    assert_eq!(report["steps"][0]["returnedItems"], 40);
    assert_eq!(report["coverage"]["responseTruncated"], true);
    let mut repeat = original;
    fit_report(&mut repeat, /*budget*/ 100_000).unwrap();
    assert_eq!(repeat, report);
    let digest = report["contentHash"].clone();
    report["contentHash"] = json!("");
    use sha2::{Digest, Sha256};
    assert_eq!(
        digest,
        format!("{:x}", Sha256::digest(report.to_string().as_bytes()))
    );
}

#[test]
fn fields_and_positions_are_part_of_source_identity() {
    let mut other_field = claim("r1", "白塔", Polarity::Positive);
    other_field["sourcePath"] = json!("/scenes/0/body");
    let mut other_position = claim("r1", "白塔", Polarity::Positive);
    other_position["assertion"]["evidence"]["start"] = json!(5);
    other_position["assertion"]["evidence"]["end"] = json!(7);
    let mut other_chapter = claim("r1", "白塔", Polarity::Positive);
    other_chapter["sourceObjectId"] = json!("chapter-2");
    let mut report = report(vec![step(
        "state",
        vec![
            claim("r1", "白塔", Polarity::Positive),
            other_field,
            other_position,
            other_chapter,
        ],
        vec![],
    )]);
    fit_report(&mut report, /*budget*/ 9000).unwrap();
    assert_eq!(report["evidence"].as_object().unwrap().len(), 4);
}
