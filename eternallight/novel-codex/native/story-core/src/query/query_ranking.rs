use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::RepositoryError;

pub(crate) fn rank(report: &mut Value) -> Result<(), RepositoryError> {
    let question = report["plan"]["question"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?;
    let mut queries = Vec::new();
    for step in report["plan"]["steps"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?
    {
        let operation = &step["operation"];
        let terms = [
            question,
            operation["query"].as_str().unwrap_or_default(),
            operation["name"].as_str().unwrap_or_default(),
        ]
        .into_iter()
        .flat_map(literal_terms)
        .collect::<BTreeSet<_>>();
        queries.push(terms);
    }
    for (step, terms) in report["steps"]
        .as_array_mut()
        .ok_or(RepositoryError::CorruptState)?
        .iter_mut()
        .zip(queries)
    {
        let rows = step["results"]
            .as_array_mut()
            .ok_or(RepositoryError::CorruptState)?;
        for row in rows.iter_mut() {
            let mut text = String::new();
            collect_content(row, &mut text);
            let tokens = literal_terms(&text);
            row["relevanceScore"] = json!(terms.intersection(&tokens).count());
        }
        // Stable sorting preserves backend order for equal literal overlap.
        rows.sort_by_key(|row| {
            std::cmp::Reverse(row["relevanceScore"].as_u64().unwrap_or_default())
        });
    }
    report["ranking"] = json!({"method":"literalOverlap","semantic":false,"selection":"channelFloorThenLowestScore"});
    Ok(())
}

fn collect_content(value: &Value, text: &mut String) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if matches!(
                    key.as_str(),
                    "quote"
                        | "snippet"
                        | "title"
                        | "displayName"
                        | "aliases"
                        | "predicate"
                        | "relationType"
                        | "value"
                ) {
                    text.push_str(&value.to_string());
                    text.push(' ');
                } else if matches!(key.as_str(), "evidence" | "assertion" | "claims") {
                    collect_content(value, text);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_content(item, text);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn literal_terms(text: &str) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    let mut word = String::new();
    let mut cjk = Vec::new();
    for character in text.to_lowercase().chars().chain(std::iter::once(' ')) {
        if ('\u{3400}'..='\u{9fff}').contains(&character) {
            cjk.push(character);
        } else if !cjk.is_empty() {
            if cjk.len() == 1 {
                terms.insert(cjk[0].to_string());
            } else {
                terms.extend(cjk.windows(2).map(|pair| pair.iter().collect::<String>()));
            }
            cjk.clear();
        }
        if character.is_ascii_alphanumeric() {
            word.push(character);
        } else if !word.is_empty() {
            terms.insert(std::mem::take(&mut word));
        }
    }
    terms
}
