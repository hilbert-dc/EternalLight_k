use super::*;
use crate::event_rule_sources::SourceState;
use pretty_assertions::assert_eq;
use std::collections::HashMap;

fn leaf() -> Value {
    json!({"type":"state","role":"actor","subjectKind":"character","predicate":"alive","quantifier":"all","operator":"eq","operand":{"type":"boolean","value":true}})
}

fn rules(condition: Value) -> PropertyRules {
    PropertyRules { revision_id:Some("rules-adopted".into()),
        definitions:vec![serde_json::from_value(json!({"predicate":"alive","subjectKind":"character","schema":{"cardinality":"single","valueType":{"type":"boolean"}},"description":"生存状态","exceptions":[]})).unwrap()],
        event_rules:vec![serde_json::from_value(json!({"eventType":"departure","description":"行动者状态","exceptions":[],"condition":condition})).unwrap()] }
}

fn event() -> Value {
    json!({"predicate":"departure","storyTime":5,"value":{"participants":[{"role":"actor","entityId":"a"}]}})
}

fn sources(values: &[Value]) -> ConditionSources {
    ConditionSources { states:HashMap::from([(("a".into(),"alive".into()), values.iter().enumerate().map(|(index,value)| SourceState { batch_id:format!("batch-{index}"), assertion_index:0,
        assertion:json!({"kind":"state","subjectId":"a","predicate":"alive","validFrom":1,"value":value}) }).collect())]),
        kinds:HashMap::from([("a".into(),"character".into())]),coverage:json!({"pendingSources":0,"totalSources":values.len(),"scannedSources":values.len(),"omittedSources":0,"sourceBytes":0}),complete:true }
}

fn report(event: &Value, rules: &PropertyRules, sources: &ConditionSources) -> Value {
    assess(
        event,
        rules,
        &IdentityResolution::unresolved(),
        Some(sources),
        &mut 100_000,
    )
    .unwrap()
}

#[test]
fn conflicting_sources_and_negation_preserve_both_independent_truth_bits() {
    for (values, expected) in [
        (vec![json!(true)], "supported"),
        (vec![json!(false)], "refuted"),
        (vec![json!(true), json!(false)], "both"),
        (vec![], "unknown"),
    ] {
        let sources = sources(&values);
        assert_eq!(
            report(&event(), &rules(leaf()), &sources)["result"],
            json!(expected)
        );
        let negated = report(
            &event(),
            &rules(json!({"type":"not","condition":leaf()})),
            &sources,
        );
        assert_eq!(
            negated["result"],
            json!(match expected {
                "supported" => "refuted",
                "refuted" => "supported",
                "both" => "both",
                "unknown" => "unknown",
                _ => unreachable!(),
            })
        );
    }
    let positive = leaf();
    let negative = json!({"type":"not","condition":positive});
    let source = sources(&[json!(true)]);
    assert_eq!(
        report(
            &event(),
            &rules(json!({"type":"all","conditions":[positive,negative]})),
            &source
        )["result"],
        "refuted"
    );
    assert_eq!(
        report(
            &event(),
            &rules(json!({"type":"any","conditions":[positive,negative]})),
            &source
        )["result"],
        "supported"
    );
}

#[test]
fn coverage_exceptions_time_and_resource_gaps_never_certify_satisfaction() {
    let rules = rules(leaf());
    let event = event();
    for start in [json!(null), json!(5), json!(6)] {
        let mut sources = sources(&[json!(true)]);
        sources.states.values_mut().next().unwrap()[0].assertion["validFrom"] = start;
        assert_eq!(report(&event, &rules, &sources)["result"], "unknown");
    }
    let mut sources = sources(&[json!(true)]);
    sources.states.values_mut().next().unwrap()[0].assertion["validUntilExclusive"] = json!(5);
    assert_eq!(report(&event, &rules, &sources)["result"], "supported");
    sources.complete = false;
    let incomplete = report(&event, &rules, &sources);
    assert_eq!(
        (&incomplete["result"], &incomplete["observedResult"]),
        (&json!("unknown"), &json!("supported"))
    );
    sources.complete = true;
    assert_eq!(
        assess(
            &event,
            &rules,
            &IdentityResolution::unresolved(),
            Some(&sources),
            &mut 0
        )
        .unwrap()["result"],
        "unknown"
    );
    let mut rules = rules;
    rules.event_rules[0]
        .exceptions
        .push("预言可能改变限制".into());
    assert_eq!(report(&event, &rules, &sources)["result"], "unknown");
    rules.event_rules[0].exceptions.clear();
    rules.definitions[0].exceptions.push("未知能力".into());
    assert_eq!(report(&event, &rules, &sources)["result"], "unknown");
}

#[test]
fn participant_quantifiers_and_multiple_membership_do_not_use_closed_world_absence() {
    let mut event = event();
    event["value"]["participants"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"actor","entityId":"b"}));
    let mut sources = sources(&[json!(true)]);
    sources.kinds.insert("b".into(), "character".into());
    sources.states.insert(
        ("b".into(), "alive".into()),
        vec![SourceState {
            batch_id: "other".into(),
            assertion_index: 0,
            assertion: json!({"validFrom":1,"value":false}),
        }],
    );
    assert_eq!(
        report(&event, &rules(leaf()), &sources)["result"],
        "refuted"
    );
    let mut any = leaf();
    any["quantifier"] = json!("any");
    assert_eq!(report(&event, &rules(any), &sources)["result"], "supported");
    event["value"]["participants"] = json!([]);
    assert_eq!(
        report(&event, &rules(leaf()), &sources)["result"],
        "unknown"
    );
    event["value"]["participants"] = json!([{"role":"actor","entityId":"a"}]);
    let mut rules = rules(leaf());
    rules.definitions[0].schema["cardinality"] = json!("multiple");
    sources
        .states
        .get_mut(&("a".into(), "alive".into()))
        .unwrap()
        .push(SourceState {
            batch_id: "different-member".into(),
            assertion_index: 0,
            assertion: json!({"validFrom":1,"value":false}),
        });
    assert_eq!(report(&event, &rules, &sources)["result"], "supported");
    sources
        .states
        .get_mut(&("a".into(), "alive".into()))
        .unwrap()
        .remove(0);
    assert_eq!(report(&event, &rules, &sources)["result"], "unknown");
}

#[test]
fn entity_comparison_requires_explicit_identity_and_repeated_support_keeps_one_witness() {
    let identity = IdentityResolution::unresolved();
    assert_eq!(
        compare(
            &json!({"entityId":"a"}),
            &json!({"entityId":"alias"}),
            &Operator::Eq,
            &Cardinality::Single,
            &identity
        ),
        Truth::default()
    );
    let mut sources = sources(&[json!(true)]);
    for index in 1..17 {
        sources
            .states
            .values_mut()
            .next()
            .unwrap()
            .push(SourceState {
                batch_id: format!("batch-{index}"),
                assertion_index: 0,
                assertion: json!({"validFrom":1,"value":true}),
            });
    }
    let report = report(&event(), &rules(leaf()), &sources);
    assert_eq!(
        (
            &report["result"],
            &report["observedResult"],
            &report["coverage"]["evidenceTruncated"]
        ),
        (&json!("supported"), &json!("supported"), &json!(false))
    );
    assert_eq!(
        report["sourceClaims"],
        json!([{"sourceBatchId":"batch-0","assertionIndex":0}])
    );
    assert!(report.to_string().len() <= 6144);
}

#[test]
fn late_counterevidence_and_quantified_branches_keep_independent_witnesses() {
    let mut values = vec![json!(true); 32];
    values.push(json!(false));
    let mut sources = sources(&values);
    let mut event = event();
    event["value"]["participants"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"actor","entityId":"b"}));
    sources.kinds.insert("b".into(), "character".into());
    sources.states.insert(
        ("b".into(), "alive".into()),
        vec![SourceState {
            batch_id: "other".into(),
            assertion_index: 0,
            assertion: json!({"validFrom":1,"value":true}),
        }],
    );
    for condition in [
        leaf(),
        json!({"type":"not","condition":leaf()}),
        json!({"type":"any","conditions":[leaf(),leaf()]}),
    ] {
        let report = report(&event, &rules(condition), &sources);
        assert_eq!(
            (
                &report["result"],
                &report["coverage"]["complete"],
                &report["coverage"]["evidenceTruncated"]
            ),
            (&json!("both"), &json!(true), &json!(false))
        );
        assert_eq!(
            report["sourceClaims"],
            json!([
                {"sourceBatchId":"batch-0","assertionIndex":0},
                {"sourceBatchId":"batch-32","assertionIndex":0},
                {"sourceBatchId":"other","assertionIndex":0}
            ])
        );
    }
    // A comparison budget that stops before the counterclaim must remain incomplete.
    let limited = assess(
        &event,
        &rules(leaf()),
        &IdentityResolution::unresolved(),
        Some(&sources),
        &mut 32,
    )
    .unwrap();
    assert_eq!(
        (&limited["result"], &limited["coverage"]["complete"]),
        (&json!("unknown"), &json!(false))
    );
}

#[test]
fn distinct_witness_overflow_is_explicit_until_paginated_explanations_exist() {
    let mut sources = sources(&[]);
    let mut participants = Vec::new();
    for index in 0..9 {
        let id = format!("actor-{index}");
        participants.push(json!({"role":"actor","entityId":id}));
        sources.kinds.insert(id.clone(), "character".into());
        sources.states.insert(
            (id.clone(), "alive".into()),
            [true, false]
                .into_iter()
                .enumerate()
                .map(|(offset, value)| SourceState {
                    batch_id: format!("{id}-{offset}"),
                    assertion_index: 0,
                    assertion: json!({"validFrom":1,"value":value}),
                })
                .collect(),
        );
    }
    let mut event = event();
    event["value"]["participants"] = json!(participants);
    let report = report(&event, &rules(leaf()), &sources);
    assert_eq!(
        (
            &report["result"],
            &report["observedResult"],
            &report["coverage"]["evidenceTruncated"]
        ),
        (&json!("unknown"), &json!("both"), &json!(true))
    );
    assert_eq!(report["sourceClaims"].as_array().unwrap().len(), 16);
    let mut progress = ConditionProgress::default();
    let page = assess_with_progress(
        &event,
        &rules(leaf()),
        &IdentityResolution::unresolved(),
        Some(&sources),
        &mut 100_000,
        &mut progress,
    )
    .unwrap();
    let durable = page.into_durable_report("condition-example").unwrap();
    assert_eq!(
        (
            &durable["result"],
            &durable["coverage"]["complete"],
            &durable["sourceClaimsTruncated"]
        ),
        (&json!("both"), &json!(true), &json!(true))
    );
    let explanations = progress.explanations().unwrap();
    assert_eq!(explanations.len(), 9);
    for (index, row) in explanations.iter().enumerate() {
        assert_eq!(
            row,
            &json!({"leafIndex":0,"subjectId":format!("actor-{index}"),"operand":true,"assessment":{"result":"both","support":{"sourceBatchId":format!("actor-{index}-0"),"assertionIndex":0},"refute":{"sourceBatchId":format!("actor-{index}-1"),"assertionIndex":0},"unresolvedChecks":0}})
        );
    }
}

enum PageCompletion {
    Partial,
    Complete,
}

fn condition_page(
    all: &ConditionSources,
    range: std::ops::Range<usize>,
    completion: PageCompletion,
) -> ConditionSources {
    let states = all
        .states
        .iter()
        .map(|(key, states)| {
            (
                key.clone(),
                states
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| range.contains(index))
                    .map(|(_, state)| SourceState {
                        batch_id: state.batch_id.clone(),
                        assertion_index: state.assertion_index,
                        assertion: state.assertion.clone(),
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<HashMap<_, _>>();
    let kinds = all
        .kinds
        .iter()
        .filter(|(id, _)| {
            states
                .iter()
                .any(|((subject, _), claims)| subject == *id && !claims.is_empty())
        })
        .map(|(id, kind)| (id.clone(), kind.clone()))
        .collect();
    ConditionSources {
        states,
        kinds,
        coverage: all.coverage.clone(),
        complete: matches!(completion, PageCompletion::Complete),
    }
}

#[test]
fn rule_checkpoints_preserve_quantified_branches_at_every_page_boundary() {
    let mut values = vec![json!(true); 32];
    values.push(json!(false));
    let mut all = sources(&values);
    all.kinds.insert("b".into(), "character".into());
    all.states.insert(
        ("b".into(), "alive".into()),
        vec![
            SourceState {
                batch_id: "b-first".into(),
                assertion_index: 0,
                assertion: json!({"validFrom":1,"value":false}),
            },
            SourceState {
                batch_id: "b-last".into(),
                assertion_index: 0,
                assertion: json!({"validFrom":1,"value":true}),
            },
        ],
    );
    let mut event = event();
    event["value"]["participants"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"actor","entityId":"b"}));
    let mut any = leaf();
    any["quantifier"] = json!("any");
    for condition in [
        leaf(),
        json!({"type":"not","condition":leaf()}),
        json!({"type":"all","conditions":[leaf(),any]}),
        json!({"type":"any","conditions":[leaf(),{"type":"not","condition":leaf()}]}),
    ] {
        let rules = rules(condition);
        let expected = report(&event, &rules, &all);
        for boundary in 0..=values.len() {
            let mut progress = ConditionProgress::default();
            let prefix = condition_page(&all, 0..boundary, PageCompletion::Partial);
            let page = assess_with_progress(
                &event,
                &rules,
                &IdentityResolution::unresolved(),
                Some(&prefix),
                &mut 100_000,
                &mut progress,
            )
            .unwrap();
            assert!(!page.budget_exhausted);
            assert_eq!(page.report["result"], "unknown");
            progress = serde_json::from_slice(&serde_json::to_vec(&progress).unwrap()).unwrap();
            let suffix = condition_page(&all, boundary..values.len(), PageCompletion::Complete);
            let page = assess_with_progress(
                &event,
                &rules,
                &IdentityResolution::unresolved(),
                Some(&suffix),
                &mut 100_000,
                &mut progress,
            )
            .unwrap();
            assert!(!page.budget_exhausted);
            assert_eq!(page.report, expected, "boundary {boundary}");
        }
    }
}

#[test]
fn rule_checkpoint_rejects_changed_inputs_and_signals_uncommittable_partial_pages() {
    let adopted_rules = rules(leaf());
    let event = event();
    let all = sources(&[json!(true), json!(false)]);
    let mut progress = ConditionProgress::default();
    let page = assess_with_progress(
        &event,
        &adopted_rules,
        &IdentityResolution::unresolved(),
        Some(&all),
        &mut 0,
        &mut progress,
    )
    .unwrap();
    assert!(page.budget_exhausted);
    assert_eq!(page.report["result"], "unknown");
    let checkpoint = serde_json::to_vec(&progress).unwrap();
    for change in 0..5 {
        let mut changed_event = event.clone();
        let mut changed_rules = rules(leaf());
        let mut changed_sources = sources(&[json!(true), json!(false)]);
        match change {
            0 => changed_event["storyTime"] = json!(6),
            1 => changed_rules.revision_id = Some("new-rules".into()),
            2 => changed_rules.definitions[0]
                .exceptions
                .push("new-exception".into()),
            3 => changed_sources.coverage["sourceProofId"] = json!("new-proof"),
            4 => changed_rules.event_rules.clear(),
            _ => unreachable!(),
        }
        let mut restored = serde_json::from_slice(&checkpoint).unwrap();
        assert!(matches!(
            assess_with_progress(
                &changed_event,
                &changed_rules,
                &IdentityResolution::unresolved(),
                Some(&changed_sources),
                &mut 100_000,
                &mut restored
            ),
            Err(RepositoryError::TaskBindingInvalid)
        ));
    }
    let mut unbound: Value = serde_json::from_slice(&checkpoint).unwrap();
    unbound["inputHash"] = Value::Null;
    let mut unbound = serde_json::from_value(unbound).unwrap();
    assert!(matches!(
        assess_with_progress(
            &event,
            &adopted_rules,
            &IdentityResolution::unresolved(),
            Some(&all),
            &mut 100_000,
            &mut unbound
        ),
        Err(RepositoryError::CorruptState)
    ));
    let mut changed_type = sources(&[json!(true)]);
    changed_type.kinds.insert("a".into(), "item".into());
    let mut restored = serde_json::from_slice(&checkpoint).unwrap();
    assert!(matches!(
        assess_with_progress(
            &event,
            &adopted_rules,
            &IdentityResolution::unresolved(),
            Some(&changed_type),
            &mut 100_000,
            &mut restored
        ),
        Err(RepositoryError::CorruptState)
    ));
}
