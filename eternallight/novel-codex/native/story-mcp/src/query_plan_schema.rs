use serde_json::{Value, json};

pub(crate) fn query_plan_schema(operation: &str) -> Value {
    let ranking = json!({"oneOf":[
        {"type":"object","additionalProperties":false,"required":["type"],"properties":{"type":{"const":"literal"}}},
        {"type":"object","additionalProperties":false,"required":["type","baseReportHash","judgments"],
         "properties":{"type":{"const":"modelSemantic"},"baseReportHash":{"type":"string","minLength":64,"maxLength":64,"description":"Exact lowercase SHA-256 contentHash of the baseline report; Runtime verifies identity."},
            "judgments":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"object","additionalProperties":false,
                "required":["resultId","score","rationale"],"properties":{"resultId":{"type":"string","minLength":64,"maxLength":64,"description":"Exact resultId from the baseline report; Runtime verifies membership."},
                    "score":{"type":"integer","minimum":0,"maximum":1000},"rationale":{"type":"string","minLength":1,"maxLength":256,"description":"Non-blank rationale, at most 256 UTF-8 bytes, not 256 Unicode characters."}}}}}}
    ]});
    let text = json!({"type":"object","additionalProperties":false,"required":["type","query","limit"],
        "properties":{"type":{"const":"text"},"query":{"type":"string","minLength":1,"maxLength":240,"description":"One contiguous substring, at most 240 Unicode characters."},"limit":{"type":"integer","minimum":1,"maximum":20}}});
    let passages = json!({"type":"object","additionalProperties":false,"required":["type","cursor","maxChars","limit"],
        "properties":{"type":{"const":"passages"},"cursor":{"type":["string","null"],"minLength":1,"maxLength":768,"description":"Copy the returned cursor exactly; at most 768 UTF-8 bytes. JSON null starts the first page."},
            "maxChars":{"type":"integer","minimum":80,"maximum":400},"limit":{"type":"integer","minimum":1,"maximum":4}}});
    let resolve = json!({"type":"object","additionalProperties":false,"required":["type","name"],
        "properties":{"type":{"const":"resolveEntity"},"name":{"type":"string","minLength":1,"maxLength":160,"description":"Exact source name, at most 160 Unicode characters."}}});
    let mut state = json!({"type":"object","additionalProperties":false,"required":["type","entityId","perspective"],
        "properties":{"type":{"const":"state"},"entityId":{"type":"string","minLength":1,"maxLength":160,"description":"Copy the resolved entity ID exactly; at most 160 UTF-8 bytes."},
            "perspective":{"enum":["author","world","character","reader"]},"predicate":{"type":"string","minLength":1,"maxLength":80},
            "ownerId":{"type":"string","minLength":1,"maxLength":160},"readingChapter":{"type":"integer","minimum":0},
            "worldTime":{"type":"integer"},"epistemicTime":{"type":"integer"}}});
    if operation == "generateChapter" {
        state["properties"]["perspective"]["enum"] = json!(["world", "character", "reader"]);
    }
    let relations = json!({"type":"object","additionalProperties":false,"required":["type","entityId","direction","maxHops","limit"],
        "properties":{"type":{"const":"relations"},"entityId":{"type":"string","minLength":1,"maxLength":160,"description":"Copy the resolved entity ID exactly; at most 160 UTF-8 bytes."},
            "relationType":{"type":"string","minLength":1,"maxLength":160},"direction":{"enum":["outgoing","incoming","both"]},
            "worldTime":{"type":"integer"},"maxHops":{"type":"integer","minimum":1,"maximum":3},"limit":{"type":"integer","minimum":1,"maximum":20}}});
    let mut operations = vec![text, passages.clone(), resolve, state, relations];
    if operation == "generateChapter" {
        let mut baseline = passages;
        baseline["properties"]["type"]["const"] = json!("writerBaselinePassages");
        baseline["description"] = json!(
            "Reads only this wholeChapter task's frozen original draft. Copy operationContext.requiredSourceReads.plan; no caller-selected chapter or revision, no prior/future chapter access."
        );
        operations.push(baseline);
    }
    json!({"type":"object","additionalProperties":false,"required":["question","byteBudget","steps","ranking"],
        "description":"Runtime checks the complete typed query plan serialized as JSON against 9000 UTF-8 bytes; field limits alone do not guarantee the total fits.",
        "properties":{"ranking":ranking,"question":{"type":"string","minLength":1,"maxLength":1000,"description":"Non-blank question, at most 1000 UTF-8 bytes, not 1000 Unicode characters."},"byteBudget":{"type":"integer","minimum":4000,"maximum":9000,"description":"Query report budget: 4000 through 9000 bytes inclusive, distinct from read_context.byteBudget. If truncated, use fewer steps or smaller result limits."},
            "steps":{"type":"array","minItems":1,"maxItems":4,"items":{"type":"object","additionalProperties":false,"required":["id","operation"],
                "properties":{"id":{"type":"string","minLength":1,"maxLength":32,"description":"Unique step ID, at most 32 UTF-8 bytes; preserve it when paging."},"operation":{"oneOf":operations}}}}}})
}

pub(crate) fn transport_schema(operation: &str) -> Value {
    let perspectives = if operation == "generateChapter" {
        "world|character|reader"
    } else {
        "author|world|character|reader"
    };
    let baseline = if operation == "generateChapter" {
        " writerBaselinePassages uses the same cursor/maxChars/limit fields as passages, only for the task's frozen own wholeChapter draft; copy operationContext.requiredSourceReads.plan. Ordinary POV passages restrictions remain."
    } else {
        ""
    };
    json!({"type":"object","description":format!(
        "Finite query plan, checked by MCP before Runtime execution. passages is sequential original-text scanning, not semantic retrieval; question binds the report but does not select relevant passages. text is literal substring search. modelSemantic only reranks the returned baseline and cannot recover missing candidates. Combine entity/state/relations and source search for a question; expand relevant original scenes through read_source_content before judging sufficiency. Empty results never prove semantic absence. Required: question (nonblank, <=1000 UTF-8 bytes), byteBudget (4000..9000), steps (1..4), ranking. No unknown fields anywhere. Each step: id (unique, 1..32 UTF-8 bytes), operation. Operations: text {{type:'text',query:one contiguous substring (1..240 Unicode characters),limit:1..20}}; passages {{type:'passages',cursor:JSON null for first page or exact returned cursor (1..768 UTF-8 bytes),maxChars:80..400,limit:1..4}};{baseline} resolveEntity {{type:'resolveEntity',name:exact source name (1..160 Unicode characters)}}; state {{type:'state',entityId:1..160 UTF-8 bytes,perspective:{perspectives}}} with optional predicate (1..80 characters), ownerId (1..160 characters), readingChapter (integer >=0), worldTime/epistemicTime (integers); relations {{type:'relations',entityId:1..160 UTF-8 bytes,direction:outgoing|incoming|both,maxHops:1..3,limit:1..20}} with optional relationType (1..160 characters), worldTime (integer). First ranking: {{type:'literal'}}. Semantic rerank: {{type:'modelSemantic',baseReportHash:exact 64-character baseline SHA-256,judgments:[{{resultId:exact 64-character baseline ID,score:integer 0..1000,rationale:nonblank <=256 UTF-8 bytes}}]}}; 1..64 judgments, cover every baseline result. Runtime checks source/cursor/ranking identity and complete typed plan <=9000 UTF-8 bytes. On isError, correct the indicated field without changing frozen binding, source or authorization."
    )})
}

// Supplement the complete MCP schema with existing Runtime UTF-8 field limits.
// The final typed-plan serialization budget remains Runtime-owned.
pub(super) fn validate_utf8_bounds(plan: &Value) -> Result<(), String> {
    let check = |value: &Value, path: &str, maximum: usize| {
        if value.as_str().is_some_and(|text| text.len() > maximum) {
            return Err(format!("{path} exceeds {maximum} UTF-8 bytes"));
        }
        Ok(())
    };
    check(&plan["question"], "plan.question", /*maximum*/ 1000)?;
    for (index, step) in plan["steps"].as_array().into_iter().flatten().enumerate() {
        check(
            &step["id"],
            &format!("plan.steps[{index}].id"),
            /*maximum*/ 32,
        )?;
        let operation = &step["operation"];
        match operation["type"].as_str() {
            Some("passages" | "writerBaselinePassages") => check(
                &operation["cursor"],
                &format!("plan.steps[{index}].operation.cursor"),
                /*maximum*/ 768,
            )?,
            Some("state" | "relations") => check(
                &operation["entityId"],
                &format!("plan.steps[{index}].operation.entityId"),
                /*maximum*/ 160,
            )?,
            _ => {}
        }
    }
    for (index, judgment) in plan["ranking"]["judgments"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        check(
            &judgment["rationale"],
            &format!("plan.ranking.judgments[{index}].rationale"),
            /*maximum*/ 256,
        )?;
    }
    Ok(())
}
