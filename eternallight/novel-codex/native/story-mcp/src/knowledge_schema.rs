use serde_json::{Value, json};

fn evidence() -> Value {
    json!({"$ref":"#/$defs/E"})
}

pub(super) fn definitions() -> Value {
    json!({
        "E":{"type":"object","additionalProperties":false,
            "required":["quote","start","end"],"properties":{
                "quote":{"type":"string","minLength":1,"maxLength":2000},
                "start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":1}}},
        "I":{"type":"string","minLength":1,"maxLength":160},
        "T":{"type":["integer","null"]},
        "R":{"type":"object","additionalProperties":false,
            "required":["sourceBatchId","assertionIndex"],"properties":{
                "sourceBatchId":{"type":["string","null"],"minLength":1,"maxLength":160},
                "assertionIndex":{"type":"integer","minimum":0,"maximum":99}}},
        "L":{"type":"object","additionalProperties":false,
            "required":["sourceBatchId","assertionIndex"],"properties":{
                "sourceBatchId":{"type":"null"},
                "assertionIndex":{"type":"integer","minimum":0,"maximum":99}}}
    })
}

pub(super) fn entities() -> Value {
    json!({"type":"array","maxItems":100,"items":{
        "type":"object","additionalProperties":false,
        "required":["id","kind","displayName","evidence"],"properties":{
        "id":{"$ref":"#/$defs/I"},
        "kind":{"type":"string","enum":["character","organization","location","item","proposition","event"]},
        "displayName":{"type":"string","minLength":1,"maxLength":160},
        "aliases":{"type":"array","maxItems":8,"items":{"type":"string","minLength":1,"maxLength":80}},
        "evidence":evidence()}}})
}

pub(super) fn assertions() -> Value {
    let id = json!({"$ref":"#/$defs/I"});
    let time = json!({"$ref":"#/$defs/T"});
    let common = json!({"type":"object","additionalProperties":false,
        "required":["kind","subjectId","predicate","value","evidence"],
        "properties":{"kind":{"type":"string"},"subjectId":id,
        "predicate":{"type":"string","minLength":1,"maxLength":160},
        "value":{},"evidence":evidence()}});
    let mut general = common.clone();
    general["properties"]["kind"] =
        json!({"type":"string","enum":["assertion","relation","epistemic"]});
    for field in [
        "targetId",
        "relationType",
        "epistemicOwnerId",
        "propositionId",
        "propositionSourceBatchId",
    ] {
        general["properties"][field] = id.clone();
    }
    for field in ["storyTime", "epistemicTime"] {
        general["properties"][field] = time.clone();
    }
    for field in [
        "validFrom",
        "validUntilExclusive",
        "epistemicUntilExclusive",
    ] {
        general["properties"][field] = json!({"type":"integer"});
    }
    for field in ["validUntilEvidence", "epistemicUntilEvidence"] {
        general["properties"][field] = evidence();
    }
    general["properties"]["epistemicState"] =
        json!({"type":"string","enum":["known","suspected","false"]});

    let mut event = common.clone();
    event["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("storyTime"));
    event["properties"]["kind"] = json!({"type":"string","enum":["event"]});
    event["properties"]["predicate"]["maxLength"] = json!(80);
    event["properties"]["storyTime"] = time.clone();
    event["properties"]["value"] = json!({"type":"object","additionalProperties":false,
        "required":["participants","preconditions","effects","untilExclusive"],"properties":{
        "participants":{"type":"array","minItems":1,"maxItems":16,"items":{
            "type":"object","additionalProperties":false,"required":["role","entityId"],"properties":{
            "role":{"type":"string","enum":["actor","recipient","beneficiary","affected","location","instrument","item","witness"]},"entityId":id}}},
        "preconditions":{"type":"array","maxItems":16,"items":{"$ref":"#/$defs/R"}},
        "effects":{"type":"array","maxItems":16,"items":{"$ref":"#/$defs/L"}},
        "untilExclusive":time}});

    let mut state = common;
    state["required"]
        .as_array_mut()
        .unwrap()
        .extend([json!("stateSchema"), json!("validFrom")]);
    state["properties"]["kind"] = json!({"type":"string","enum":["state"]});
    state["properties"]["predicate"]["maxLength"] = json!(80);
    state["properties"]["validFrom"] = time;
    state["properties"]["validUntilExclusive"] = json!({"type":"integer"});
    state["properties"]["validUntilEvidence"] = evidence();
    state["properties"]["value"] = json!({"anyOf":[{"type":"boolean"},{"type":"integer"},
        {"type":"string","minLength":1,"maxLength":512},
        {"type":"object","additionalProperties":false,"required":["entityId"],"properties":{"entityId":id}}]});
    state["properties"]["stateSchema"] = json!({"type":"object","additionalProperties":false,
        "required":["cardinality","valueType"],"properties":{
        "cardinality":{"type":"string","enum":["single","multiple"]},
        "valueType":{"anyOf":[
            {"type":"object","additionalProperties":false,"required":["type"],"properties":{"type":{"type":"string","enum":["boolean","integer","text"]}}},
            {"type":"object","additionalProperties":false,"required":["type","kind"],"properties":{
                "type":{"type":"string","enum":["entity"]},
                "kind":{"type":"string","enum":["character","organization","location","item","proposition","event"]}}}]}}});
    // Shape only: Runtime still validates source anchors, reference identities,
    // state value/schema agreement and temporal relationships transactionally.
    json!({"type":"array","maxItems":100,"items":{"anyOf":[general,event,state]}})
}
