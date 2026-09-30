use serde_json::{Value, json};

use crate::RepositoryError;

fn leaf() -> Value {
    json!({"type":"state","role":"actor","subjectKind":"character","predicate":"alive",
        "quantifier":"all","operator":"eq","operand":{"type":"boolean","value":true}})
}

fn envelope(condition: Value) -> Value {
    json!({"source":"author","version":1,"baseProjectRevision":0,"payload":{
        "definitions":[{"predicate":"alive","subjectKind":"character",
            "schema":{"cardinality":"single","valueType":{"type":"boolean"}},"description":"生存状态","exceptions":[]}],
        "eventRules":[{"eventType":"departure","description":"行动者仍在世","exceptions":[],"condition":condition}]}})
}

fn validate(value: &Value) -> Result<(), RepositoryError> {
    crate::property_rules::validate_content(&value.to_string())
}

#[test]
fn typed_event_operands_and_parameterized_roles_share_declared_property_schemas() {
    for (value_type, operand, operator) in [
        (
            json!({"type":"boolean"}),
            json!({"type":"boolean","value":true}),
            "ne",
        ),
        (
            json!({"type":"integer"}),
            json!({"type":"integer","value":-9_007_199_254_740_991_i64}),
            "lt",
        ),
        (
            json!({"type":"text"}),
            json!({"type":"text","value":"SQL is ordinary text, not executable"}),
            "eq",
        ),
        (
            json!({"type":"entity","kind":"item"}),
            json!({"type":"participant","role":"instrument","quantifier":"any"}),
            "eq",
        ),
    ] {
        let mut value =
            envelope(json!({"type":"all","conditions":[leaf(),{"type":"not","condition":leaf()}]}));
        value["payload"]["definitions"][0]["schema"]["valueType"] = value_type;
        for index in 0..2 {
            let condition = if index == 0 {
                &mut value["payload"]["eventRules"][0]["condition"]["conditions"][0]
            } else {
                &mut value["payload"]["eventRules"][0]["condition"]["conditions"][1]["condition"]
            };
            condition["operator"] = json!(operator);
            condition["operand"] = operand.clone();
        }
        assert!(validate(&value).is_ok(), "{value}");
        value["payload"]["eventRules"][0]["condition"]["conditions"][0]["operand"] =
            json!({"type":"text","value":"wrong type"});
        if operator != "eq"
            || value["payload"]["definitions"][0]["schema"]["valueType"]["type"] != "text"
        {
            assert!(matches!(
                validate(&value),
                Err(RepositoryError::InvalidContent(_))
            ));
        }
    }
}

#[test]
fn event_conditions_reject_undeclared_properties_wrong_roles_and_executable_fields() {
    for (field, replacement) in [
        ("subjectKind", json!("location")),
        ("predicate", json!("undeclared")),
        ("role", json!("location")),
        ("operator", json!("ge")),
        ("quantifier", json!("every")),
        (
            "operand",
            json!({"type":"participant","role":"actor","quantifier":"all"}),
        ),
        ("execute", json!("SELECT 1")),
    ] {
        let mut value = envelope(leaf());
        value["payload"]["eventRules"][0]["condition"][field] = replacement;
        assert!(
            matches!(validate(&value), Err(RepositoryError::InvalidContent(_))),
            "{field}"
        );
    }
    let mut entity = envelope(leaf());
    entity["payload"]["definitions"][0]["schema"]["valueType"] =
        json!({"type":"entity","kind":"item"});
    entity["payload"]["eventRules"][0]["condition"]["operand"] =
        json!({"type":"participant","role":"witness","quantifier":"any"});
    assert!(matches!(
        validate(&entity),
        Err(RepositoryError::InvalidContent(_))
    ));
    entity["payload"]["definitions"][0]["schema"]["valueType"] = json!({"type":"integer"});
    entity["payload"]["eventRules"][0]["condition"]["operand"] =
        json!({"type":"integer","value":i64::MAX});
    assert!(matches!(
        validate(&entity),
        Err(RepositoryError::InvalidContent(_))
    ));
}

#[test]
fn event_expression_depth_node_group_and_serialized_budgets_are_enforced() {
    let chain = |length| {
        (0..length).fold(
            leaf(),
            |condition, _| json!({"type":"not","condition":condition}),
        )
    };
    let valid = envelope(
        json!({"type":"any","conditions":(0..8).map(|index|chain(if index < 6 {3} else {2})).collect::<Vec<_>>()}),
    );
    assert!(validate(&valid).is_ok()); // 31 nodes, depth four.
    for condition in [
        chain(5),
        json!({"type":"all","conditions":[]}),
        json!({"type":"all","conditions":vec![leaf();9]}),
        json!({"type":"any","conditions":vec![chain(3);8]}),
    ] {
        assert!(matches!(
            validate(&envelope(condition)),
            Err(RepositoryError::InvalidContent(_))
        ));
    }
    let mut oversized = envelope(json!({"type":"all","conditions":vec![leaf();8]}));
    oversized["payload"]["definitions"][0]["schema"]["valueType"] = json!({"type":"text"});
    for child in oversized["payload"]["eventRules"][0]["condition"]["conditions"]
        .as_array_mut()
        .unwrap()
    {
        child["operand"] = json!({"type":"text","value":"x".repeat(512)});
    }
    assert!(matches!(
        validate(&oversized),
        Err(RepositoryError::InvalidContent(_))
    ));
    let mut duplicates = envelope(leaf());
    duplicates["payload"]["eventRules"] =
        json!(vec![duplicates["payload"]["eventRules"][0].clone(); 2]);
    assert!(matches!(
        validate(&duplicates),
        Err(RepositoryError::InvalidContent(_))
    ));
    let mut count = envelope(leaf());
    let first = count["payload"]["eventRules"][0].clone();
    count["payload"]["eventRules"] = json!(
        (0..16)
            .map(|index| {
                let mut rule = first.clone();
                rule["eventType"] = json!(format!("event-{index}"));
                rule
            })
            .collect::<Vec<_>>()
    );
    assert!(validate(&count).is_ok());
    count["payload"]["eventRules"]
        .as_array_mut()
        .unwrap()
        .push(first);
    assert!(matches!(
        validate(&count),
        Err(RepositoryError::InvalidContent(_))
    ));
    for (field, replacement) in [
        ("description", json!("汉".repeat(171))),
        ("eventType", json!("x".repeat(81))),
        ("exceptions", json!(vec!["例外"; 9])),
    ] {
        let mut value = envelope(leaf());
        value["payload"]["eventRules"][0][field] = replacement;
        assert!(matches!(
            validate(&value),
            Err(RepositoryError::InvalidContent(_))
        ));
    }
}
