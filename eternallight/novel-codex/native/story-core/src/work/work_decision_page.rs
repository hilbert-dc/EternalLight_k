//! Pack the complete decision envelope, not isolated arrays, within the wire budget.
use crate::RepositoryError;
use serde_json::{Value, json};

pub(crate) fn pack(
    mut base: Value,
    requested: usize,
    cursor: impl Fn(usize) -> String,
) -> Result<Value, RepositoryError> {
    let streams = [
        base["requirements"].take(),
        base["readyActions"].take(),
        base["receipts"]["items"].take(),
    ];
    let streams: Vec<&Vec<Value>> = streams
        .iter()
        .map(|v| v.as_array().ok_or(RepositoryError::CorruptState))
        .collect::<Result<_, _>>()?;
    let totals: Vec<usize> = streams.iter().map(|s| s.len()).collect();
    // Interleave independent streams while preserving each stream's order.
    let mut entries = Vec::new();
    for index in 0..totals.iter().copied().max().unwrap_or(0) {
        for (stream, values) in streams.iter().enumerate() {
            if let Some(value) = values.get(index) {
                entries.push((stream, value));
            }
        }
    }
    let mut offset = [0usize; 3];
    let mut position = 0;
    for page in 0..=requested {
        let start = offset;
        let mut items: [Vec<Value>; 3] = std::array::from_fn(|_| Vec::new());
        let render = |items: &[Vec<Value>; 3], more: bool| {
            let mut value = base.clone();
            value["requirements"] = json!(items[0]);
            value["readyActions"] = json!(items[1]);
            value["receipts"]["items"] = json!(items[2]);
            for (field, data) in [
                (
                    "complete",
                    json!(items[0].len() == totals[0] && items[1].len() == totals[1]),
                ),
                ("requirementsOffset", json!(start[0])),
                ("requirementsComplete", json!(items[0].len() == totals[0])),
                ("actionsOffset", json!(start[1])),
                ("actionsComplete", json!(items[1].len() == totals[1])),
            ] {
                value["coverage"][field] = data;
            }
            value["receipts"]["coverage"] = json!({"total":totals[2],"returned":items[2].len(),"offset":start[2],"complete":items[2].len()==totals[2]});
            value["pagination"] =
                json!({"page":page,"hasMore":more,"nextCursor":more.then(||cursor(page+1))});
            crate::work_decision::bounded(value)
        };
        let mut response = render(&items, position < entries.len())?;
        while let Some(&(stream, value)) = entries.get(position) {
            items[stream].push(value.clone());
            match render(&items, position + 1 < entries.len()) {
                Ok(value) => {
                    response = value;
                    offset[stream] += 1;
                    position += 1;
                }
                Err(error) => {
                    items[stream].pop();
                    if offset == start {
                        return Err(error);
                    }
                    break;
                }
            }
        }
        if page == requested {
            return Ok(response);
        }
        if position == entries.len() {
            return Err(RepositoryError::InvalidContent(
                "Work status cursor is out of range; restart read_work_status with cursor null"
                    .into(),
            ));
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variable_width_rows_preserve_all_streams_and_full_envelope() {
        let original = json!({"requirements":[{"id":"a","text":"验".repeat(800)},{"id":"b"}],
            "readyActions":[{"key":"c","text":"动".repeat(700)},{"key":"d"}],
            "receipts":{"items":[{"id":"e"},{"id":"f","text":"稿".repeat(500)}]},
            "coverage":{},"chapterAcceptance":{"text":"标".repeat(1000)},"binding":"fixed"});
        let mut got: [Vec<Value>; 3] = std::array::from_fn(|_| Vec::new());
        let mut page = 0;
        loop {
            let value = pack(original.clone(), page, |n| {
                format!("{n}:{}", "a".repeat(64))
            })
            .unwrap();
            assert!(value.to_string().len() <= 6500);
            assert_eq!(value["chapterAcceptance"], original["chapterAcceptance"]);
            assert_eq!(value["binding"], "fixed");
            assert_eq!(value["coverage"]["requirementsOffset"], got[0].len());
            assert_eq!(value["coverage"]["actionsOffset"], got[1].len());
            assert_eq!(value["receipts"]["coverage"]["offset"], got[2].len());
            for (i, rows) in [
                &value["requirements"],
                &value["readyActions"],
                &value["receipts"]["items"],
            ]
            .into_iter()
            .enumerate()
            {
                got[i].extend(rows.as_array().unwrap().iter().cloned());
            }
            page += 1;
            if value["pagination"]["nextCursor"].is_null() {
                break;
            }
        }
        assert!(page > 1);
        assert_eq!(json!(got[0]), original["requirements"]);
        assert_eq!(json!(got[1]), original["readyActions"]);
        assert_eq!(json!(got[2]), original["receipts"]["items"]);
        assert!(pack(original, page, |n| n.to_string()).is_err());
    }
}
