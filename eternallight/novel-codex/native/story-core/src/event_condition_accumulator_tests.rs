use super::*;
use pretty_assertions::assert_eq;

#[test]
fn every_checkpoint_boundary_matches_uninterrupted_comparison() {
    let identity = IdentityResolution::unresolved();
    let kinds = HashMap::new();
    let operand = json!(true);
    let schema = PropertySchema {
        cardinality: Cardinality::Single,
        value_type: ValueType::Boolean,
    };
    let context = ComparisonContext {
        time: Some(5),
        schema: &schema,
        operand: &operand,
        operator: &Operator::Eq,
        identity: &identity,
        kinds: &kinds,
    };
    let mut states = (0..33)
        .map(|index| SourceState {
            batch_id: format!("batch-{index:03}"),
            assertion_index: index,
            assertion: json!({"validFrom":1,"value":index!=32}),
        })
        .collect::<Vec<_>>();
    // Page-local absence must not become a durable unresolved condition.
    states[0].assertion["validFrom"] = json!(8);
    let mut whole = ComparisonAccumulator::default();
    for state in &states {
        whole.observe(state, &context).unwrap();
    }
    assert_eq!(
        whole.truth(),
        Truth {
            support: true,
            refute: true
        }
    );
    assert_eq!(whole.final_unresolved_checks().unwrap(), 0);
    for boundary in 0..=states.len() {
        let mut prefix = ComparisonAccumulator::default();
        for state in &states[..boundary] {
            prefix.observe(state, &context).unwrap();
        }
        let bytes = serde_json::to_vec(&prefix).unwrap();
        assert!(bytes.len() < 1024);
        let mut resumed: ComparisonAccumulator = serde_json::from_slice(&bytes).unwrap();
        for state in &states[boundary..] {
            resumed.observe(state, &context).unwrap();
        }
        assert_eq!(resumed, whole, "checkpoint after {boundary} claims");
    }
    assert_eq!(
        whole.witnesses().cloned().collect::<Vec<_>>(),
        vec![
            Evidence {
                source_batch_id: "batch-001".into(),
                assertion_index: 1
            },
            Evidence {
                source_batch_id: "batch-032".into(),
                assertion_index: 32
            },
        ]
    );
}

#[test]
fn unresolved_evidence_survives_checkpoints_without_inventing_negative_membership() {
    let identity = IdentityResolution::unresolved();
    let kinds = HashMap::new();
    let operand = json!(true);
    for cardinality in [Cardinality::Single, Cardinality::Multiple] {
        let schema = PropertySchema {
            cardinality,
            value_type: ValueType::Boolean,
        };
        let context = ComparisonContext {
            time: Some(5),
            schema: &schema,
            operand: &operand,
            operator: &Operator::Eq,
            identity: &identity,
            kinds: &kinds,
        };
        let mut progress = ComparisonAccumulator::default();
        let claims = [
            json!({"validFrom":null,"value":true}),
            json!({"validFrom":1,"value":false}),
            json!({"validFrom":1,"value":true}),
        ];
        for (index, assertion) in claims.into_iter().enumerate() {
            progress
                .observe(
                    &SourceState {
                        batch_id: format!("batch-{index}"),
                        assertion_index: 0,
                        assertion,
                    },
                    &context,
                )
                .unwrap();
            progress = serde_json::from_slice(&serde_json::to_vec(&progress).unwrap()).unwrap();
        }
        assert_eq!(
            progress.truth(),
            Truth {
                support: true,
                refute: matches!(schema.cardinality, Cardinality::Single)
            }
        );
        assert_eq!(progress.final_unresolved_checks().unwrap(), 1);
    }
}

#[test]
fn unknown_entity_identity_and_type_are_not_repaired_by_later_pages() {
    let identity = IdentityResolution::unresolved();
    let kinds = HashMap::from([
        ("a".into(), "character".into()),
        ("b".into(), "character".into()),
    ]);
    let operand = json!({"entityId":"a"});
    let schema = PropertySchema {
        cardinality: Cardinality::Single,
        value_type: ValueType::Entity {
            kind: "character".into(),
        },
    };
    let context = ComparisonContext {
        time: Some(5),
        schema: &schema,
        operand: &operand,
        operator: &Operator::Eq,
        identity: &identity,
        kinds: &kinds,
    };
    let mut progress = ComparisonAccumulator::default();
    for (index, id) in ["missing-type", "b", "a"].into_iter().enumerate() {
        progress
            .observe(
                &SourceState {
                    batch_id: format!("batch-{index}"),
                    assertion_index: 0,
                    assertion: json!({"validFrom":1,"value":{"entityId":id}}),
                },
                &context,
            )
            .unwrap();
        progress = serde_json::from_slice(&serde_json::to_vec(&progress).unwrap()).unwrap();
    }
    assert_eq!(
        progress.truth(),
        Truth {
            support: true,
            refute: false
        }
    );
    assert_eq!(progress.final_unresolved_checks().unwrap(), 2);
}
