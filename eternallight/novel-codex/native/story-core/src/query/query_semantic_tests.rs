use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use crate::query_report::{bound_report, fit_report, step_report};
use crate::{QueryRanking, QueryStep};

fn baseline(count: usize) -> Value {
    let step: QueryStep = serde_json::from_value(
        json!({"id":"state","operation":{"type":"state","entityId":"lamp","perspective":"author"}}),
    )
    .unwrap();
    let rows = (0..count).map(|index|json!({"sourceObjectId":format!("draft-{index}"),"sourceRevisionId":format!("revision-{index}"),
        "confirmationState":"candidate","assertion":{"value":index%2==0,"evidence":{"quote":format!("白塔线索{index}"),"start":0,"end":5}}})).collect::<Vec<_>>();
    let findings = json!([{"leftEntryIndex":0,"rightEntryIndex":1,"type":"polarityConflict"}]);
    let step = step_report(&step,json!({"entries":rows,"findings":findings,"coverage":{"complete":true},"nextCursor":"original-cursor"})).unwrap();
    let mut report = json!({"plan":{"question":"白塔","ranking":{"type":"literal"},"steps":[{"operation":{"type":"state"}}]},
        "steps":[step],"coverage":{"complete":true,"semanticCompleteness":"unknown","responseTruncated":false},"contentHash":""});
    fit_report(&mut report, /*budget*/ 9000).unwrap();
    report
}

fn judgments(report: &Value) -> QueryRanking {
    serde_json::from_value(json!({"type":"modelSemantic","baseReportHash":report["contentHash"],
        "judgments":report["steps"][0]["results"].as_array().unwrap().iter().enumerate().map(|(index,row)|json!({
            "resultId":row["resultId"],"score":index*10,"rationale":"仅为按问题相关性的模型判断"})).collect::<Vec<_>>()})).unwrap()
}

#[test]
fn reranking_preserves_conflicts_evidence_and_confirmation_without_re_normalizing() {
    let mut report = baseline(4);
    let before = report.clone();
    super::apply(&mut report, &judgments(&before)).unwrap();
    bound_report(&mut report, /*budget*/ 9000).unwrap();
    assert_eq!(
        report["steps"][0]["results"][0]["resultId"],
        before["steps"][0]["results"][3]["resultId"]
    );
    assert_eq!(
        report["steps"][0]["findings"],
        before["steps"][0]["findings"]
    );
    assert_eq!(report["evidence"], before["evidence"]);
    assert_eq!(report["coverage"], before["coverage"]);
    for row in report["steps"][0]["results"].as_array().unwrap() {
        let original = before["steps"][0]["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["resultId"] == row["resultId"])
            .unwrap();
        let mut actual = row.clone();
        for key in [
            "literalRelevanceScore",
            "modelRelevanceRationale",
            "relevanceScore",
        ] {
            actual.as_object_mut().unwrap().remove(key);
        }
        let mut expected = original.clone();
        expected.as_object_mut().unwrap().remove("relevanceScore");
        assert_eq!(actual, expected);
    }
}

#[test]
fn model_rationales_consume_the_same_budget_and_clipping_removes_orphan_citations() {
    let mut report = baseline(10);
    let mut ranking = judgments(&report);
    if let QueryRanking::ModelSemantic { judgments, .. } = &mut ranking {
        for judgment in judgments {
            judgment.rationale = "界".repeat(85);
        }
    }
    super::apply(&mut report, &ranking).unwrap();
    bound_report(&mut report, /*budget*/ 4000).unwrap();
    assert!(report.to_string().len() <= 4000);
    assert_eq!(report["coverage"]["complete"], false);
    assert_eq!(report["steps"][0]["findings"], json!([]));
    assert_eq!(report["steps"][0]["nextCursor"], Value::Null);
    assert_eq!(report["steps"][0]["coverage"]["requeryRequired"], true);
    let rows = report["steps"][0]["results"].as_array().unwrap();
    assert!(!rows.is_empty() && rows.len() < 10);
    assert_eq!(report["evidence"].as_object().unwrap().len(), rows.len());
    for row in rows {
        assert!(
            report["evidence"]
                .get(row["assertion"]["evidenceRef"].as_str().unwrap())
                .is_some()
        );
    }
}

#[test]
fn bad_rankings_leave_the_baseline_unchanged() {
    let before = baseline(3);
    let QueryRanking::ModelSemantic {
        base_report_hash,
        mut judgments,
    } = judgments(&before)
    else {
        panic!("semantic test input");
    };
    judgments.pop();
    let mut report = before.clone();
    assert!(
        super::apply(
            &mut report,
            &QueryRanking::ModelSemantic {
                base_report_hash,
                judgments
            }
        )
        .is_err()
    );
    assert_eq!(report, before);
}
