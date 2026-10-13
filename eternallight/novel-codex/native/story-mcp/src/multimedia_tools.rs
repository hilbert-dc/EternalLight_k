use serde_json::{Value, json};

fn typed_object(properties: Value) -> Value {
    let required: Vec<String> = properties.as_object().unwrap().keys().cloned().collect();
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn text_field_schema(canon: bool) -> Value {
    let id = json!({"type":"string","minLength":1,"maxLength":160});
    let mut fields = vec![("creativeDesign", json!({})),
        (
            "balloonText",
            json!({"pageId":id,"panelId":id,"balloonId":id}),
        ),
        ("panelDescription", json!({"pageId":id,"panelId":id})),
        ("shotDescription", json!({"shotId":id})),
        ("clipCaption", json!({"trackId":id,"clipId":id})),
    ];
    if canon {
        fields.push(("canonStatement", json!({"entryId":id})));
    }
    json!({"oneOf":fields.into_iter().map(|(kind,mut props)|{props["kind"]=json!({"const":kind});typed_object(props)}).collect::<Vec<_>>()})
}
fn valid_text_field(value: &Value, canon: bool) -> bool {
    let keys: &[&str] = match value["kind"].as_str() {
        Some("creativeDesign") => &[],
        Some("balloonText") => &["pageId", "panelId", "balloonId"],
        Some("panelDescription") => &["pageId", "panelId"],
        Some("shotDescription") => &["shotId"],
        Some("clipCaption") => &["trackId", "clipId"],
        Some("canonStatement") if canon => &["entryId"],
        _ => return false,
    };
    value.as_object().is_some_and(|v| v.len() == keys.len() + 1)
        && keys.iter().all(|key| {
            value[key].as_str().is_some_and(|v| {
                !v.is_empty() && v.len() <= 160 && !v.chars().any(char::is_control)
            })
        })
}
fn operation_schema() -> Value {
    let id = json!({"type":"string","minLength":1,"maxLength":160});
    let nullable_id = json!({"type":["string","null"],"minLength":1,"maxLength":160});
    let title = json!({"type":"string","maxLength":160});
    let text = json!({"type":"string","maxLength":20000});
    let rect = typed_object(
        json!({"x":{"type":"number","minimum":0},"y":{"type":"number","minimum":0},"width":{"type":"number","exclusiveMinimum":0},"height":{"type":"number","exclusiveMinimum":0}}),
    );
    let rate = typed_object(
        json!({"numerator":{"type":"integer","minimum":1,"maximum":1000000},"denominator":{"type":"integer","minimum":1,"maximum":1000000}}),
    );
    let time = typed_object(
        json!({"value":{"type":"integer","minimum":0,"maximum":1000000000000_i64},"rate":rate}),
    );
    let mut production_time = time.clone();
    production_time["properties"]["value"]["minimum"] = json!(1);
    let references = json!({"type":"array","maxItems":100,"items":id});
    let page = typed_object(
        json!({"id":id,"title":title,"width":{"type":"integer","minimum":1,"maximum":100000},"height":{"type":"integer","minimum":1,"maximum":100000},"readingDirection":{"enum":["leftToRight","rightToLeft","vertical"]}}),
    );
    let panel = typed_object(
        json!({"id":id,"description":text,"rect":rect,"assetRevisionId":nullable_id,"referenceAssetRevisionIds":references}),
    );
    let balloon = typed_object(
        json!({"id":id,"text":text,"speaker":title,"rect":rect,"kind":{"enum":["speech","thought","caption","sound"]}}),
    );
    let shot = typed_object(
        json!({"id":id,"title":title,"description":text,"assetRevisionId":nullable_id,"referenceAssetRevisionIds":references}),
    );
    let track = typed_object(
        json!({"id":id,"title":title,"kind":{"enum":["video","audio","subtitle"]},"muted":{"type":"boolean"}}),
    );
    let clip = typed_object(
        json!({"id":id,"shotId":nullable_id,"assetRevisionId":nullable_id,"start":time,"duration":time,"sourceStart":time,"playbackRate":rate,"caption":text}),
    );
    let variants = [
        ("replaceTextRanges",json!({"field":text_field_schema(false),"expectedTextDigest":{"type":"string","pattern":"^[a-f0-9]{64}$"},"edits":{"type":"array","minItems":1,"maxItems":32,"items":typed_object(json!({"startChar":{"type":"integer","minimum":0},"endChar":{"type":"integer","minimum":0},"expectedSliceDigest":{"type":"string","pattern":"^[a-f0-9]{64}$"},"replacement":text}))}})),
        ("setScreenProduction", json!({"production":{"anyOf":[{"type":"null"},typed_object(json!({"form":{"enum":["shortFilm","featureFilm","episode"]},"expression":{"enum":["animation","liveAction","hybrid"]},"deliverable":{"enum":["screenplay","storyboard","animatic","finishedVideo"]},"targetDuration":{"anyOf":[{"type":"null"},production_time]}}))]}})),
        ("setCreativeDesign", json!({"text":{"type":["string","null"]}})),
        ("setTitle", json!({"title":title})),
        ("setComicFormat", json!({"format":{"enum":["pages","strip"]}})),
        ("upsertPage", json!({"page":page})),
        ("removePage", json!({"pageId":id})),
        ("movePage", json!({"pageId":id,"beforeId":nullable_id})),
        ("upsertPanel", json!({"pageId":id,"panel":panel})),
        ("removePanel", json!({"pageId":id,"panelId":id})),
        ("movePanel", json!({"pageId":id,"panelId":id,"beforeId":nullable_id})),
        ("upsertBalloon", json!({"pageId":id,"panelId":id,"balloon":balloon})),
        ("removeBalloon", json!({"pageId":id,"panelId":id,"balloonId":id})),
        ("moveBalloon", json!({"pageId":id,"panelId":id,"balloonId":id,"beforeId":nullable_id})),
        ("setAnimationSettings", json!({"width":{"type":"integer","minimum":1,"maximum":100000},"height":{"type":"integer","minimum":1,"maximum":100000},"frameRate":rate})),
        ("upsertShot", json!({"shot":shot})),
        ("removeShot", json!({"shotId":id})),
        ("moveShot", json!({"shotId":id,"beforeId":nullable_id})),
        ("upsertTrack", json!({"track":track})),
        ("removeTrack", json!({"trackId":id})),
        ("moveTrack", json!({"trackId":id,"beforeId":nullable_id})),
        ("upsertClip", json!({"trackId":id,"clip":clip})),
        ("removeClip", json!({"trackId":id,"clipId":id})),
        ("moveClip", json!({"trackId":id,"clipId":id,"beforeId":nullable_id})),
        ("removeSourceLink", json!({"link":{"type":"object","required":["relation","source","targetObjectId","targetSelector"],"properties":{"relation":{"enum":["adapts","depicts","usesReference","derivedFrom"]},"source":{"type":"object"},"targetObjectId":id,"targetSelector":{"type":"object"},"purpose":{"enum":[null,"plot","character","visual","audio","timing","reference"]},"decision":{"type":["string","null"],"minLength":1,"maxLength":20000}},"additionalProperties":false}})),
    ].into_iter().map(|(kind, mut properties)| {
        properties["kind"] = json!({"const":kind});
        typed_object(properties)
    }).collect::<Vec<_>>();
    json!({"oneOf":variants})
}

pub(crate) fn specs() -> Vec<Value> {
    let mut tools: Vec<Value> = [
        ("read_media_workspace", "Read one bounded view of this task’s frozen comic/animation workspace. Default version=source is the original frozen baseline; version=working reads this task’s own latest saved revision and admitted generated assets, rejecting intervening external edits. Default overview returns version, stable Edition identity, documentHeader and counts. documentHeader.textFields exposes optional creativeDesign (model-owned story/production design, never author approval); read it using section=text and textField={kind:creativeDesign}. section=sourceImpacts (version=source only) compares this task’s exact frozen dependencies to current heads and returns bounded change candidates, never new source access, automatic invalidation or rewriting. Follow nextOffset. Page sections pages/panels/balloons/shots/tracks/clips/assets/sources/editionSources/canon/canonSources using offset/limit and nextOffset until null. canon returns statement textFields and sourceCount; canonSources requires entryId and pages that entry’s exact source refs. Text/FieldText source quotes are limited to 8000 Unicode scalars; cite longer originals in explicit segments. canon always reads this Edition’s frozen author-established shared settings snapshot, including when version=working; it never adopts latest settings. panels needs pageId; balloons needs pageId + panelId; clips needs trackId. pages/tracks/panels are structural summaries; read their children separately. Text-bearing entity sections return textFields descriptors rather than full prose. Read section=text with its exact textField and independent charOffset/charLimit (1–8000 Unicode scalar values), follow nextCharOffset until null. Text pages include the actual fixed revision, full textDigest, sliceDigest and exact returnedSource; serialized output remains under 64 KiB. partialView is never a complete replacement document: use patch_media_document for local changes. Preserve exact IDs and receipts. Metadata is not visual inspection; an unsaved document has null revisionId.", false),
        ("save_media_document", "Save an explicitly requested complete comic/animation document under this active Edition task. Prefer patch_media_document for local changes after bounded reads. A full save requires the full document and all source links/dependencies; never save a partial read view. Preserve unrelated objects and all locked panels/shots. Supply expectedRevisionId from the last actual read/save receipt and reuse idempotencyKey only for this identical save. This saves an editable document; it does not prove a picture/video was generated, rendered, published, or validated. Never use an author endpoint or invent an asset revision.", true),
    ].into_iter().map(|(name,description,save)| {
        let mut properties=json!({
            "projectId":{"type":"string","minLength":1,"maxLength":160},
            "taskId":{"type":"string","minLength":1,"maxLength":160},
            "threadId":{"type":"string","minLength":1,"maxLength":160},
            "expectedRevision":{"type":"integer","minimum":0}
        });
        let mut required=vec!["projectId","taskId","threadId","expectedRevision"];
        if save {
            required.push("request");
            properties["request"]=json!({"type":"object","additionalProperties":false,
                "required":["expectedRevisionId","document","idempotencyKey","readSet","sources"],
                "properties":{
                    "expectedRevisionId":{"type":["string","null"]},
                    "document":{"type":"object","required":["kind","title"],"properties":{"kind":{"enum":["comic","animation"]},"title":{"type":"string"}},"additionalProperties":true},
                    "idempotencyKey":{"type":"string","minLength":1,"maxLength":160},
                    "readSet":{"type":"array","maxItems":500,"items":{"type":"object"}},
                    "sources":{"type":"array","maxItems":1000,"items":{"type":"object"}}
                }});
        }
        if !save {
            properties["version"]=json!({"enum":["source","working"]});
            properties["section"]=json!({"enum":["overview","pages","panels","balloons","shots","tracks","clips","assets","sources","editionSources","canon","canonSources","text","sourceImpacts"]});
            properties["textField"]=text_field_schema(true);
            properties["charOffset"]=json!({"type":"integer","minimum":0});
            properties["charLimit"]=json!({"type":"integer","minimum":1,"maximum":8000});
            for key in ["pageId","panelId","trackId","entryId"] {properties[key]=json!({"type":"string","minLength":1,"maxLength":160});}
            properties["offset"]=json!({"type":"integer","minimum":0});
            properties["limit"]=json!({"type":"integer","minimum":1,"maximum":100});
        }
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
    }).collect();
    let mut patch = tools[1].clone();
    patch["name"] = json!("patch_media_document");
    patch["description"] = json!(
        "Apply one atomic bounded batch of typed local comic/animation changes. setScreenProduction sets or clears an audiovisual production target: narrative form, expression, deliverable and optional rational targetDuration. This is a model-owned plan, not author approval or proof of a finished film. Screenplays belong in creativeDesign; shots/timeline/assets remain actual separate evidence. Finished-video exports with a target duration must match it. setCreativeDesign saves or clears (text=null) an optional reusable story/production design: goals, structure, character arcs, adaptation choices and near-term approach as appropriate. It is model-owned prose, not author approval or a mandatory prerequisite. Read existing design first; prefer replaceTextRanges with field={kind:creativeDesign} for local revisions. Read the relevant sections first, preserve their stable IDs and supply the last actual working expectedRevisionId. Metadata upserts retain unread children, locks and order; new nodes append with empty children. Use replaceTextRanges for local prose edits: field, expectedTextDigest and 1–32 non-overlapping original Unicode scalar ranges, each with expectedSliceDigest and replacement. One text replacement operation per field per batch; never edit Canon. Supply hashes from actual text pages, do not infer UTF-16 indexes or resubmit stale ranges. Use explicit move operations with beforeId (null appends). A nonexistent remove/move target rejects the entire batch. Locked panels/shots cannot be edited, deleted or unlocked by this tool. Changes, appended source links and merged read dependencies commit as one revision or all fail. Source links may record purpose (plot/character/visual/audio/timing/reference) and decision (a model-owned explanation of preserved, combined or changed expression; never author approval). Multiple links can map many sources to one target or a source to many targets. sourceImpacts reports readFreshness for existing read dependencies. When an author has selected a newer baseline, compare the old source, newly granted source and current target before a local edit; retain historical comparison inputs as pinned and declare actual current inputs with mustBeCurrent in readSet. This changes model-declared read dependencies, not author requirements. readSet and sources add/update exact refs without dropping unrelated prior refs; removing a source link requires its exact link. Remove dangling source links/shot references in the same batch. Max 100 operations and 256 KiB request. Reuse idempotencyKey only for the same batch, including after uncertain transport; receipt proves editable document save, not media generation/rendering/publication."
    );
    patch["inputSchema"]["properties"]["request"] = json!({"type":"object","additionalProperties":false,"required":["expectedRevisionId","idempotencyKey","operations","readSet","sources"],"properties":{
        "expectedRevisionId":{"type":["string","null"]},"idempotencyKey":{"type":"string","minLength":1,"maxLength":160},
        "operations":{"type":"array","minItems":1,"maxItems":100,"items":operation_schema()},
        "readSet":{"type":"array","maxItems":500,"items":{"type":"object"}},"sources":{"type":"array","maxItems":1000,"items":{"type":"object"}}
    }});
    tools.push(patch);
    let identity = json!({"projectId":{"type":"string"},"taskId":{"type":"string"},"threadId":{"type":"string"},"expectedRevision":{"type":"integer","minimum":0}});
    let mut read_adaptation = typed_object(identity.clone());
    read_adaptation["properties"]["offset"] = json!({"type":"integer","minimum":0});
    read_adaptation["properties"]["limit"] = json!({"type":"integer","minimum":1,"maximum":20});
    tools.push(json!({"name":"read_adaptation_sources","description":"Read a bounded directory of exact saved sources this advisory task may use for a new comic/animation adaptation. Follow nextOffset. Novel sources are from the task frozen snapshot; media sources include its admitted references and real saved outputs. Read original content before adapting; a directory is not proof of reading. No arbitrary project/edition access.","inputSchema":read_adaptation}));
    let mut adaptation = identity;
    adaptation["request"] = typed_object(json!({"medium":{"enum":["comic","animation"]},"title":{"type":"string","minLength":1,"maxLength":160},"sources":{"type":"array","minItems":1,"maxItems":20,"items":{"type":"object"}},"instruction":{"type":"string","minLength":1,"maxLength":4000},"idempotencyKey":{"type":"string","minLength":1,"maxLength":160}}));
    tools.push(json!({"name":"request_media_adaptation","description":"When the author requests an adaptation, register a new comic/animation Edition and dependent step in this same Work. Use exact real sources from read_adaptation_sources, never future IDs. Include creative direction and required delivery level in instruction; original author intent is separately inherited. Registration does not create artwork: complete the current task to let Host execute the next step. After saving a comic, its task can register the author-requested animation from that real revision. Discussion alone does not authorize creation. Preserve the receipt and retry only with the identical idempotencyKey/request; do not create duplicates after compaction. No new budget, source editing, adoption or publication authority.","inputSchema":typed_object(adaptation)}));

    let mut source = tools[0].clone();
    source["name"] = json!("read_media_source");
    source["description"] = json!(
        "Read one exact frozen source admitted to this Edition task: editionSources, baseline source links/read dependencies, or admitted asset revisions. Supply the real source reference including revisionId,digest,scopeId,branchId,objectId,selector. This cannot query arbitrary project content. Whole comic/animation sources use the same bounded section paging as read_media_workspace; selected panel/shot/clip grants expose only that selection. Novel body text pages use Unicode-character offset/limit. Asset metadata is not visual inspection. For media/Canon prose use section=text, exact textField, charOffset/charLimit and follow nextCharOffset. A returnedSource may be reread only within its exact field/range grant; it never authorizes sibling text. Own task-saved revisions are admitted, arbitrary branch HEAD is not. Follow nextOffset for entity sections and never assume missing content is empty."
    );
    source["inputSchema"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("version");
    source["inputSchema"]["properties"]["section"].as_object_mut().unwrap().get_mut("enum").unwrap()
        .as_array_mut().unwrap().retain(|value| value != "sourceImpacts");
    source["inputSchema"]["properties"]["source"] = json!({"type":"object","additionalProperties":false,"required":["projectId","scopeId","branchId","objectId","revisionId","digest","selector"],"properties":{"projectId":{"type":"string"},"scopeId":{"type":"string"},"branchId":{"type":"string"},"objectId":{"type":"string"},"revisionId":{"type":"string"},"digest":{"type":"string","pattern":"^[a-f0-9]{64}$"},"selector":{"type":"object","required":["kind"]}}});
    source["inputSchema"]["properties"]["limit"]["maximum"] = json!(8000);
    source["inputSchema"]["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("source"));
    tools.push(source);
    for (name, description, action) in [
        (
            "request_media_generation",
            "Request a real image, video, speech or transcription operation for this active Edition task. Choose an available configured service from mediaServices context. parameters must match that service capability. Use one stable idempotencyKey for exactly the same request; never create another key/provider to retry an uncertain paid submission. Runtime freezes source, target, and original work authorization/budget. A submitted job is not a generated or saved result. Retain the returned operation.id and inspect execution/state/seal.",
            "prepare",
        ),
        (
            "read_media_generation",
            "Reconcile a previously registered media operation under the same task and original operationId. This may perform one bounded provider status check; it does not create a replacement request. Domain prepared means not yet sealed; inspect execution.state and the complete seal. An unknown result is not a failure/refund and must not be resubmitted.",
            "read",
        ),
        (
            "wait_media_generation",
            "Perform one bounded reconciliation step for an existing media operation. Respect execution.nextPollAt; do not busy poll. Preserve operationId through timeout, cancellation and restart. Only a ready seal with real asset revisions proves generated media was saved. Saving an editable timeline/document or animatic does not prove full animation.",
            "wait",
        ),
        (
            "cancel_media_generation",
            "Request cancellation of an existing media operation. Retain its original identity and source. This records cancellation intent; a local abort does not prove remote cancellation or refund. Completed remote output may still be collected against the original version. Inspect the actual execution state and seal.",
            "cancel",
        ),
    ] {
        let mut properties = json!({
            "projectId":{"type":"string","minLength":1,"maxLength":160},
            "taskId":{"type":"string","minLength":1,"maxLength":160},
            "threadId":{"type":"string","minLength":1,"maxLength":160},
            "expectedRevision":{"type":"integer","minimum":0}
        });
        let mut required = vec!["projectId", "taskId", "threadId", "expectedRevision"];
        if action == "prepare" {
            required.push("request");
            properties["request"] = json!({"type":"object","additionalProperties":false,
            "required":["providerId","kind","parameters","idempotencyKey"],"properties":{
                "providerId":{"type":"string","minLength":1,"maxLength":160},
                "kind":{"enum":["image.generate","image.edit","video.generate","audio.speech","audio.transcribe"]},
                "parameters":{"type":"object","additionalProperties":true},
                "idempotencyKey":{"type":"string","minLength":1,"maxLength":160}
            }});
        } else {
            required.push("operationId");
            properties["operationId"] = json!({"type":"string","minLength":1,"maxLength":160});
            if action == "read" || action == "wait" {
                properties["transcriptOffset"] = json!({"type":"integer","minimum":0});
                properties["transcriptLimit"] =
                    json!({"type":"integer","minimum":1,"maximum":8000});
            }
        }
        tools.push(json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}));
    }
    tools
}

pub(crate) fn request(
    name: &str,
    arguments: &Value,
) -> Result<(&'static str, String, Option<Value>), String> {
    let get = |key: &str| {
        arguments[key]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 160 && !s.chars().any(char::is_control))
            .ok_or_else(|| format!("{key} must be an exact identifier"))
    };
    let project = get("projectId")?;
    let task = get("taskId")?;
    let thread = get("threadId")?;
    let revision = arguments["expectedRevision"]
        .as_i64()
        .filter(|v| *v >= 0)
        .ok_or("expectedRevision must be nonnegative")?;
    let (action, route) = match name {
        "read_adaptation_sources" => ("adaptationSources", "workspace"),
        "request_media_adaptation" => ("adapt", "workspace"),
        "read_media_workspace" => ("read", "workspace"),
        "read_media_source" => ("source", "workspace"),
        "save_media_document" => ("save", "workspace"),
        "patch_media_document" => ("patch", "workspace"),
        "request_media_generation" => ("prepare", "operations"),
        "read_media_generation" => ("read", "operations"),
        "wait_media_generation" => ("wait", "operations"),
        "cancel_media_generation" => ("cancel", "operations"),
        _ => return Err("unknown media tool".into()),
    };
    let mut body = json!({"threadId":thread,"expectedRevision":revision,"action":action});
    if matches!(action, "save" | "patch" | "prepare" | "adapt") {
        if !arguments["request"].is_object() {
            return Err("request must contain the complete typed document save".into());
        }
        body["request"] = arguments["request"].clone();
    }
    if action == "source" {
        if !arguments["source"].is_object() {
            return Err("source must be an exact frozen media reference".into());
        }
        body["source"] = arguments["source"].clone();
    }
    if arguments["section"] == "sourceImpacts" && (action != "read"
        || arguments.get("version").is_some_and(|v| v != "source")
        || ["pageId", "panelId", "trackId", "entryId", "textField", "charOffset", "charLimit"].iter().any(|key| arguments.get(key).is_some())) {
        return Err("sourceImpacts requires read_media_workspace with version=source and offset/limit only".into());
    }
    if route == "workspace" && matches!(action, "read" | "source" | "adaptationSources") {
        if (arguments["section"] == "canonSources") != arguments.get("entryId").is_some() {
            return Err(
                "canonSources requires entryId; entryId is not valid for other sections".into(),
            );
        }
        if let Some(version) = arguments.get("version") {
            if !version
                .as_str()
                .is_some_and(|v| matches!(v, "source" | "working"))
            {
                return Err("invalid media read version".into());
            }
            body["version"] = version.clone();
        }
        if let Some(section) = arguments.get("section") {
            if !section.as_str().is_some_and(|v| {
                matches!(
                    v,
                    "overview" | "sourceImpacts"
                        | "pages"
                        | "panels"
                        | "balloons"
                        | "shots"
                        | "tracks"
                        | "clips"
                        | "assets"
                        | "sources"
                        | "editionSources"
                        | "canon"
                        | "canonSources"
                        | "text"
                )
            }) {
                return Err("invalid media read section".into());
            }
            body["section"] = section.clone();
        }
        for key in ["pageId", "panelId", "trackId", "entryId"] {
            if arguments.get(key).is_some() {
                body[key] = json!(get(key)?);
            }
        }
        for (key, min, max) in [
            ("offset", 0, i64::MAX),
            ("limit", 1, if action == "source" { 8000 } else { 100 }),
        ] {
            if let Some(value) = arguments.get(key) {
                let n = value
                    .as_i64()
                    .filter(|v| *v >= min && *v <= max)
                    .ok_or_else(|| format!("{key} is outside its admitted page bound"))?;
                body[key] = json!(n);
            }
        }
        if arguments["section"] == "text" {
            if !valid_text_field(&arguments["textField"], true)
                || ["offset", "limit", "pageId", "panelId", "trackId", "entryId"]
                    .iter()
                    .any(|k| arguments.get(k).is_some())
            {
                return Err(
                    "text pages require an exact textField and independent charOffset/charLimit"
                        .into(),
                );
            }
            body["textField"] = arguments["textField"].clone();
            for (key, min, max) in [("charOffset", 0, i64::MAX), ("charLimit", 1, 8000)] {
                if let Some(v) = arguments.get(key) {
                    let n = v
                        .as_i64()
                        .filter(|n| *n >= min && *n <= max)
                        .ok_or("invalid text character page bound")?;
                    body[key] = json!(n);
                }
            }
        } else if ["textField", "charOffset", "charLimit"]
            .iter()
            .any(|k| arguments.get(k).is_some())
        {
            return Err("character page fields require section=text".into());
        }
    }
    if route == "operations" && action != "prepare" {
        body["operationId"] = json!(get("operationId")?);
    }
    if route == "operations" && matches!(action, "read" | "wait") {
        for (key, min, max) in [
            ("transcriptOffset", 0, i64::MAX),
            ("transcriptLimit", 1, 8000),
        ] {
            if let Some(value) = arguments.get(key) {
                let n = value
                    .as_i64()
                    .filter(|v| *v >= min && *v <= max)
                    .ok_or_else(|| format!("{key} is outside its admitted page bound"))?;
                body[key] = json!(n);
            }
        }
    }
    if body.to_string().len() > 1_500_000 {
        return Err("media tool request exceeds transport limit".into());
    }
    if action == "patch" {
        let patch = &arguments["request"];
        if patch.to_string().len() > 256 * 1024 {
            return Err("media patch request exceeds 256 KiB".into());
        }
        let operations = patch["operations"]
            .as_array()
            .filter(|ops| !ops.is_empty() && ops.len() <= 100)
            .ok_or("media patch requires 1 through 100 typed operations")?;
        if operations.iter().any(|op| {
            matches!(
                op["kind"].as_str(),
                Some("setPanelLock" | "setShotLock" | "adoptCanonSnapshot" | "replaceEditionSource")
            )
        }) {
            return Err("models cannot change author locks or adopt shared settings".into());
        }
        for op in operations
            .iter()
            .filter(|op| op["kind"] == "replaceTextRanges")
        {
            if !valid_text_field(&op["field"], false)
                || !op["edits"]
                    .as_array()
                    .is_some_and(|e| !e.is_empty() && e.len() <= 32)
            {
                return Err("text replacement requires a media field and 1 through 32 exact ranges; Canon is author-owned".into());
            }
        }
    }
    Ok((
        "POST",
        format!(
            "/api/projects/{}/tasks/{}/media/{route}",
            super::percent_encode(project),
            super::percent_encode(task)
        ),
        Some(body),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creative_design_uses_real_media_schema_and_text_routes() {
        let schema = operation_schema();
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(validator.is_valid(&json!({"kind":"setCreativeDesign","text":"结构与主线"})));
        assert!(validator.is_valid(&json!({"kind":"setCreativeDesign","text":null})));
        assert!(!validator.is_valid(&json!({"kind":"setCreativeDesign","text":{},"approved":true})));
        let production = json!({"kind":"setScreenProduction","production":{"form":"featureFilm","expression":"hybrid","deliverable":"screenplay","targetDuration":null}});
        assert!(validator.is_valid(&production));
        assert!(validator.is_valid(&json!({"kind":"setScreenProduction","production":null})));
        let mut zero_duration = production.clone();
        zero_duration["production"]["targetDuration"] = json!({"value":0,"rate":{"numerator":1,"denominator":1}});
        assert!(!validator.is_valid(&zero_duration));
        let mut invalid = production.clone(); invalid["production"]["approved"] = json!(true);
        assert!(!validator.is_valid(&invalid));
        let args = json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":3,"section":"text","textField":{"kind":"creativeDesign"},"charOffset":0,"charLimit":100});
        assert_eq!(request("read_media_workspace", &args).unwrap().2.unwrap()["textField"]["kind"], "creativeDesign");
        let mut forged = args; forged["textField"]["entryId"] = json!("rule");
        assert!(request("read_media_workspace", &forged).is_err());
    }
    #[test]
    fn source_impacts_are_task_scoped_read_observations() {
        let args = json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":3,"section":"sourceImpacts","offset":0,"limit":20});
        let (_, path, body) = request("read_media_workspace", &args).unwrap();
        assert_eq!(path, "/api/projects/project/tasks/task/media/workspace");
        assert_eq!(body.unwrap()["section"], "sourceImpacts");
        let mut invalid = args.clone(); invalid["version"] = json!("working");
        assert!(request("read_media_workspace", &invalid).is_err());
        invalid = args.clone(); invalid["pageId"] = json!("other");
        assert!(request("read_media_workspace", &invalid).is_err());
        invalid = args; invalid["source"] = json!({});
        assert!(request("read_media_source", &invalid).is_err());
    }

    #[test]
    fn local_patch_is_available_and_bounded_without_lock_authority() {
        let mut args = json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":3,"request":{"expectedRevisionId":"revision","idempotencyKey":"patch","operations":[{"kind":"setTitle","title":"局部编辑"}],"readSet":[],"sources":[]}});
        let (_, route, body) = request("patch_media_document", &args).unwrap();
        assert_eq!(route, "/api/projects/project/tasks/task/media/workspace");
        assert_eq!(body.unwrap()["action"], "patch");
        args["request"]["operations"] =
            json!([{"kind":"setPanelLock","pageId":"p","panelId":"panel","locked":false}]);
        assert!(request("patch_media_document", &args).is_err());
        args["request"]["operations"] =
            json!([{"kind":"adoptCanonSnapshot","canonSnapshotId":"latest"}]);
        assert!(request("patch_media_document", &args).is_err());
        args["request"]["operations"] = json!([{"kind":"replaceEditionSource","previous":{},"next":{}}]);
        assert!(request("patch_media_document", &args).is_err());
        args["request"]["operations"] = json!([]);
        assert!(request("patch_media_document", &args).is_err());
        let patch = specs()
            .into_iter()
            .find(|v| v["name"] == "patch_media_document")
            .unwrap();
        let variants=patch["inputSchema"]["properties"]["request"]["properties"]["operations"]["items"]["oneOf"].as_array().unwrap();
        assert_eq!(variants.len(), 25);
        assert!(variants.iter().all(|v| !matches!(
            v["properties"]["kind"]["const"].as_str(),
            Some("setPanelLock" | "setShotLock" | "adoptCanonSnapshot" | "replaceEditionSource")
        )));
    }
    #[test]
    fn canon_is_a_fixed_workspace_section() {
        let args = json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":3,"section":"canon","version":"working"});
        assert_eq!(
            request("read_media_workspace", &args).unwrap().2.unwrap()["section"],
            "canon"
        );
        let mut sources = args;
        sources["section"] = json!("canonSources");
        assert!(request("read_media_workspace", &sources).is_err());
        sources["entryId"] = json!("rule");
        assert_eq!(
            request("read_media_workspace", &sources)
                .unwrap()
                .2
                .unwrap()["entryId"],
            "rule"
        );
    }
    #[test]
    fn unicode_text_pages_use_exact_fields_and_separate_character_bounds() {
        let mut args = json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":3,"section":"text","version":"working","textField":{"kind":"balloonText","pageId":"page","panelId":"panel","balloonId":"balloon"},"charOffset":2,"charLimit":8000});
        let body = request("read_media_workspace", &args).unwrap().2.unwrap();
        assert_eq!(body["charOffset"], 2);
        assert_eq!(body["textField"], args["textField"]);
        assert!(body.get("offset").is_none());
        args["offset"] = json!(0);
        assert!(request("read_media_workspace", &args).is_err());
        args.as_object_mut().unwrap().remove("offset");
        args["charLimit"] = json!(8001);
        assert!(request("read_media_workspace", &args).is_err());
        args["charLimit"] = json!(2);
        args["textField"]["untypedField"] = json!("body");
        assert!(request("read_media_workspace", &args).is_err());
        args["textField"] = json!({"kind":"canonStatement","entryId":"rule"});
        assert!(request("read_media_workspace", &args).is_ok());
    }
    #[test]
    fn exact_text_patch_contract_reaches_runtime_but_canon_and_oversized_batches_are_rejected() {
        let mut args = json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":3,"request":{"expectedRevisionId":"revision","idempotencyKey":"patch","operations":[{"kind":"replaceTextRanges","field":{"kind":"shotDescription","shotId":"shot"},"expectedTextDigest":"a".repeat(64),"edits":[{"startChar":1,"endChar":2,"expectedSliceDigest":"b".repeat(64),"replacement":"😀"}]}],"readSet":[],"sources":[]}});
        assert_eq!(
            request("patch_media_document", &args).unwrap().2.unwrap()["request"],
            args["request"]
        );
        args["request"]["operations"][0]["field"] =
            json!({"kind":"canonStatement","entryId":"rule"});
        assert!(request("patch_media_document", &args).is_err());
        args["request"]["operations"][0]["field"] =
            json!({"kind":"shotDescription","shotId":"shot"});
        args["request"]["operations"][0]["edits"] = json!(vec![
                args["request"]["operations"][0]["edits"][0].clone();
                33
            ]);
        assert!(request("patch_media_document", &args).is_err());
        let schema = operation_schema();
        let variant = schema["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["properties"]["kind"]["const"] == "replaceTextRanges")
            .unwrap();
        assert_eq!(variant["properties"]["edits"]["maxItems"], 32);
        assert!(
            variant["properties"]["field"]["oneOf"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v["properties"]["kind"]["const"] != "canonStatement")
        );
    }
    #[test]
    fn exact_bound_route_and_no_author_endpoint() {
        let args =
            json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":3});
        let (_, path, body) = request("read_media_workspace", &args).unwrap();
        assert_eq!(path, "/api/projects/project/tasks/task/media/workspace");
        assert_eq!(body.unwrap()["action"], "read");
        assert!(request("save_media_document", &args).is_err());
    }
    #[test]
    fn media_workspace_pages_are_explicit_and_bounded() {
        let mut args = json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":3,"section":"panels","pageId":"p1","offset":20,"limit":10});
        let (_, _, body) = request("read_media_workspace", &args).unwrap();
        let body = body.unwrap();
        assert_eq!(body["section"], "panels");
        assert_eq!(body["offset"], 20);
        args["limit"] = json!(101);
        assert!(request("read_media_workspace", &args).is_err());
    }
    #[test]
    fn uncertain_operation_reuses_original_id() {
        let args = json!({"projectId":"project","taskId":"task","threadId":"thread","expectedRevision":3,"operationId":"original"});
        for (name, action) in [
            ("read_media_generation", "read"),
            ("wait_media_generation", "wait"),
            ("cancel_media_generation", "cancel"),
        ] {
            let (_, path, body) = request(name, &args).unwrap();
            assert_eq!(path, "/api/projects/project/tasks/task/media/operations");
            let body = body.unwrap();
            assert_eq!(body["operationId"], "original");
            assert_eq!(body["action"], action);
            assert!(body.get("request").is_none());
        }
        assert!(request("request_media_generation", &args).is_err());
    }
}
