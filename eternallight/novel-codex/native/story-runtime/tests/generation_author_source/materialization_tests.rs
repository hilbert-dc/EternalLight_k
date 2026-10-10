use super::{AUTHOR, CANDIDATE, CAPABILITY, call, setup};
use axum::http::StatusCode;
use eternallight_engine_core::PersistentBudget;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[tokio::test]
async fn metered_generation_saves_plan_and_materializes_without_adopting_or_a_deadline() {
    materialized().await;
}

async fn materialized() -> (tempfile::TempDir, std::path::PathBuf, String, Value, Value) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("metered-materialization.sqlite3");
    let (base, mut request) = setup(&path).await;
    let (status, outline) = call(&path, "POST", &format!("{base}/artifacts/outline"),
        json!({"payload":{"nodes":[]},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"outline"}), AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{outline}");
    let (_, workspace) = call(
        &path,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    request["expectedBaseSnapshotId"] = workspace["headSnapshotId"].clone();
    request["expectedSourceDigest"] = workspace["sourceDigest"].clone();
    for field in [
        "maxTaskAttempts",
        "maxProviderRequests",
        "maxProviderTokens",
        "deadlineAt",
    ] {
        request[field] = Value::Null;
    }
    let (status, run) = call(
        &path,
        "POST",
        &format!("{base}/generation-runs"),
        request,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    let uri = format!("{base}/generation-runs/{}", run["id"].as_str().unwrap());
    let action = super::prepared_action(&path, &base, &run, "generatePlan", "plan-thread").await;
    assert_eq!(action["operation"], "generatePlan");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (status, task) = call(&path, "POST", &format!("{base}/tasks"),
        json!({"operation":"generatePlan","inputRevision":0,"threadId":"plan-thread",
            "target":action["target"],"authorIntent":"本次只写一章",
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600}), AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{task}");
    let task_id = task["id"].as_str().unwrap();
    let plan_chapters: Vec<_> = (1..=3).map(|number| json!({
        "chapterPlanId":format!("cp-{number}"),"chapterNumber":number,
        "publicTitle":"受潮的一页","goal":"找到签名","conflict":"销毁截止",
        "scenes":[{"id":format!("scene-{number}"),"goal":"保留证据","conflict":"来不及",
            "beats":[],"expectedEffects":[{"id":format!("effect-{number}"),"dimension":"knowledge","description":"发现签名","commitmentRefs":[]}]}]
    })).collect();
    let (status, plan) = call(
        &path,
        "POST",
        &format!("{base}/artifacts/plan"),
        json!({"payload":{"chapters":plan_chapters},
            "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"plan",
            "taskId":task_id,"threadId":"plan-thread"}),
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{plan}");
    let (status, completed) = call(
        &path,
        "POST",
        &format!("{base}/tasks/{task_id}/complete"),
        json!({"threadId":"plan-thread","summary":"章纲已保存"}),
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    // No provider is dispatched by this local HTTP test; its terminal zero receipt is real.
    let mut budget = PersistentBudget::new(
        &path.with_extension("budget.sqlite3"),
        task_id,
        json!({"options":{"id":format!("budget-{task_id}")},"now":0}),
    )
    .unwrap();
    budget.complete("completed").unwrap();
    let (status, charged) = call(
        &path,
        "POST",
        &format!("{uri}/provider-usage"),
        json!({"taskId":task_id}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{charged}");
    let (status, ready) = call(
        &path,
        "POST",
        &format!("{uri}/advance-task"),
        json!({"expectedVersion":charged["version"],"taskId":task_id}),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{ready}");
    assert_eq!(
        (ready["stage"].clone(), ready["deadlineAt"].clone()),
        (json!("coordinate"), Value::Null)
    );
    let selected =
        super::prepared_action(&path, &base, &ready, "writeNextChapter", "plan-thread").await;
    assert_eq!(selected["operation"], "generateChapter");
    let (status, draft) = call(&path, "GET", &uri, Value::Null, AUTHOR).await;
    assert_eq!(status, StatusCode::OK, "{draft}");
    assert_eq!(
        (
            draft["stage"].clone(),
            draft["currentChapterNumber"].clone()
        ),
        (json!("coordinate"), json!(1))
    );
    // Selecting the next action is a real, separately metered coordinator task.
    // No provider ran here: totals and the previous ledger remain unchanged.
    let before = ready["providerBudget"]["charges"].as_array().unwrap();
    let after = draft["providerBudget"]["charges"].as_array().unwrap();
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(&after[..before.len()], before);
    let coordinator_id = after.last().unwrap()["taskId"].as_str().unwrap();
    let (status, coordinator) = call(
        &path,
        "GET",
        &format!("{base}/tasks/{coordinator_id}"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{coordinator}");
    assert_eq!(coordinator["operation"], "advisory");
    assert_eq!(coordinator["status"], "completed");
    assert_eq!(coordinator["target"]["runId"], run["id"]);
    assert_eq!(
        after.last().unwrap(),
        &json!({"taskId":coordinator_id,
        "budgetId":format!("budget-{coordinator_id}"),"status":"completed",
        "requests":0,"tokens":0,"unknownTokens":0,"overrun":false})
    );
    let mut expected_budget = ready["providerBudget"].clone();
    expected_budget["charges"] = draft["providerBudget"]["charges"].clone();
    assert_eq!(draft["providerBudget"], expected_budget);
    let (_, saved) = call(
        &path,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(saved["chapters"].as_array().unwrap().len(), 1);
    assert_eq!(saved["chapters"][0]["id"], draft["currentChapterId"]);
    assert_eq!(saved["chapters"][0]["title"], "受潮的一页");
    assert_eq!(saved["chapters"][0]["candidate"], Value::Null);
    assert_eq!(
        saved["artifacts"]["plan"]["candidate"]["id"],
        plan["revisionId"]
    );
    assert_eq!(saved["artifacts"]["plan"]["accepted"], Value::Null);
    assert_eq!(saved["revision"], 1);
    assert_eq!(
        saved["chapterPlanBindings"][0]["plan_revision_id"],
        plan["revisionId"]
    );
    (directory, path, base, plan, draft)
}

#[tokio::test]
async fn canonical_scene_body_survives_linking_before_extract_assess_and_rolling_projection() {
    scene_context_source(
        "",
        &["extractCommitments", "assessSceneOutcome", "generatePlan"],
    )
    .await;
}

#[tokio::test]
async fn assessment_pages_a_source_larger_than_its_context_byte_budget() {
    scene_context_source(&" ".repeat(/*n*/ 6000), &["assessSceneOutcome"]).await;
}

/// Read the same immutable, capability-bound source pages the agent sees.
async fn read_context_source(
    path: &std::path::Path,
    base: &str,
    task: &Value,
    pack: &Value,
    section: &str,
) -> Value {
    assert_eq!(pack["taskSources"]["readTool"], "read_source_content");
    assert_eq!(pack["taskSources"]["textDelivered"], false);
    let entry = pack["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["sectionPath"] == section)
        .expect("frozen source projection");
    assert_eq!(
        pack.pointer(section).unwrap(),
        &json!({"sourceId":entry["sourceId"],"textDelivered":false})
    );
    let mut cursor = Value::Null;
    let mut text = String::new();
    loop {
        let suffix = cursor
            .as_str()
            .map(|cursor| format!("&cursor={cursor}"))
            .unwrap_or_default();
        let (status, page) = call(
            path,
            "GET",
            &format!(
                "{base}/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}{suffix}",
                task["id"].as_str().unwrap(),
                task["threadId"].as_str().unwrap(),
                task["inputRevision"],
                entry["sourceId"].as_str().unwrap()
            ),
            Value::Null,
            CANDIDATE,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["sourceId"], entry["sourceId"]);
        assert_eq!(page["sectionPath"], section);
        assert_eq!(page["contentHash"], entry["contentHash"]);
        assert_eq!(page["byteRange"]["start"], text.len());
        let part = page["payloadPart"].as_str().unwrap();
        assert!(!part.is_empty() && part.len() <= 2000);
        assert!(page.to_string().len() <= 6500);
        text.push_str(part);
        assert_eq!(page["byteRange"]["end"], text.len());
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            assert_eq!(page["endOfInputFromCursor"], true);
            assert_eq!(page["byteRange"]["total"], text.len());
            break;
        }
        assert_eq!(page["endOfInputFromCursor"], false);
        assert!(text.len() < entry["payloadBytes"].as_u64().unwrap() as usize);
    }
    assert_eq!(entry["payloadBytes"], text.len());
    assert_eq!(
        format!("{:x}", Sha256::digest(text.as_bytes())),
        entry["contentHash"].as_str().unwrap()
    );
    serde_json::from_str(&text).unwrap()
}

async fn scene_context_source(padding: &str, operations: &[&str]) {
    let (_directory, path, base, plan, run) = materialized().await;
    let chapter = run["currentChapterId"].as_str().unwrap();
    // Internal whitespace tests byte accounting without expanding the unrelated text consumers' token budget.
    let repetitions = if padding.is_empty() { 100 } else { 1000 };
    let first = format!("{}{padding}end", "林夕拾起密函。🌊".repeat(repetitions));
    let body = format!("{first}\n\n---\n\n她听见远处的钟声。");
    let groups = json!([
        {"id":"opening","sceneId":"scene-1","body":format!(" {first} "),"locked":false},
        {"id":"ending","sceneId":"scene-1","body":"她听见远处的钟声。","locked":false}
    ]);
    let payload = json!({"chapterId":chapter,"chapterNumber":1,"title":"受潮的一页","body":body,"sceneGroups":groups,
        "sourcePlanRefs":{"chapterPlanId":"cp-1","planRevisionId":plan["revisionId"],"localOnly":false,"lineageStatus":"current","outlineRevisionId":null}});
    let (status, saved) = call(&path,"POST",&format!("{base}/drafts/checkpoints"),
        json!({"payload":payload,"expectedRevision":1,"expectedCandidateRevisionId":null,"idempotencyKey":"scene-source"}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let (status,batch)=call(&path,"POST",&format!("{base}/modeling-batches"),
        json!({"objectId":format!("draft-{chapter}"),"sourceRevisionId":saved["revisionId"],"expectedRevision":1,"extractorVersion":"context-order"}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{batch}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let create = |operation: &str, target: Value| {
        json!({"operation":operation,"target":target,"inputRevision":1,"threadId":operation,
        "authorIntent":"按绑定章节进行只读检查","capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600})
    };
    let (status, model) = call(
        &path,
        "POST",
        &format!("{base}/tasks"),
        create("modelKnowledge", json!({"batchId":batch["id"]})),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{model}");
    // An event in the second group proves that links use full canonical Unicode offsets.
    let quote = "钟声";
    let start = body[..body.find(quote).unwrap()].chars().count();
    let evidence = json!({"quote":quote,"start":start,"end":start+quote.chars().count()});
    let (status,published)=call(&path,"POST",&format!("{base}/modeling-batches/{}/publish",batch["id"].as_str().unwrap()),
        json!({"taskId":model["id"],"threadId":"modelKnowledge","expectedRevision":1,
            "entities":[{"id":"bell","kind":"event","displayName":"钟声","aliases":[],"evidence":evidence},
                {"id":"linxi","kind":"character","displayName":"林夕","aliases":[],"evidence":{"quote":"林夕","start":0,"end":2}}],
            "assertions":[{"kind":"event","subjectId":"bell","predicate":"rang","storyTime":null,
                "value":{"participants":[{"role":"witness","entityId":"linxi"}],"preconditions":[],"effects":[],"untilExclusive":null},"evidence":evidence}]}),CANDIDATE).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let (status, complete) = call(
        &path,
        "POST",
        &format!("{base}/tasks/{}/complete", model["id"].as_str().unwrap()),
        json!({"threadId":"modelKnowledge","summary":"已建立原文事件证据"}),
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{complete}");
    let (_, workspace) = call(
        &path,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    let mut outline_head = workspace["artifacts"]["outline"]["candidate"]["id"].clone();
    for (operation,purpose,target) in [
        ("extractCommitments","critic",json!({"type":"commitmentExtraction","chapterId":chapter,"chapterNumber":1,"sourceRevisionId":saved["revisionId"]})),
        ("assessSceneOutcome","critic",json!({"chapterId":chapter,"chapterNumber":1,"draftRevisionId":saved["revisionId"],"planRevisionId":plan["revisionId"],"knowledgeBatchId":batch["id"]})),
        ("generatePlan","planner",json!({"planFromChapter":2}))
    ].into_iter().filter(|(operation, _, _)| operations.contains(operation)) {
        if operation != "extractCommitments" {
            let nodes: Vec<_> = if operation == "assessSceneOutcome" {
                (0..10).map(|position| json!({"id":format!("volume-{position}"),"kind":"volume","position":position,
                    "title":format!("第{position}卷"),"purpose":"后续独立剧情",
                    "summary":"尚待作者决定的后卷因果安排。".repeat(/*n*/60)})).collect()
            } else { Vec::new() };
            let (status,outline)=call(&path,"POST",&format!("{base}/artifacts/outline"),
                json!({"payload":{"nodes":nodes},"expectedRevision":1,"expectedCandidateRevisionId":outline_head,"idempotencyKey":operation}),AUTHOR).await;
            assert_eq!(status,StatusCode::CREATED,"{outline}");
            outline_head=outline["revisionId"].clone();
        }
        let (status,task)=call(&path,"POST",&format!("{base}/tasks"),create(operation,target),AUTHOR).await;
        assert_eq!(status,StatusCode::CREATED,"{operation}: {task}");
        let uri=format!("{base}/context?taskId={}&threadId={operation}&projectRevision=1&purpose={purpose}&budget=24&byteBudget=60000",task["id"].as_str().unwrap());
        let (status,pack)=call(&path,"GET",&uri,Value::Null,CANDIDATE).await;
        assert_eq!(status,StatusCode::OK,"{operation}: {pack}");
        if operation == "assessSceneOutcome" {
            assert_eq!((pack["narrativeGoals"]["brief"].clone(),pack["narrativeGoals"]["outline"].clone(),pack["plannedEffects"].clone()),
                (Value::Null,Value::Null,json!([])));
            let result=json!({"content":[{"type":"text","text":pack.to_string()}]});
            let item=json!({"type":"function_call_output","call_id":"x".repeat(/*n*/160),"output":result.to_string()});
            assert!(eternallight_engine_core::dispatch("tokens.count",json!(item.to_string())).unwrap().as_u64().unwrap()<=9000);
            if !padding.is_empty() {
                let context_byte_budget = 24000;
                assert!(body.len()>context_byte_budget);
                let (status,reference_pack)=call(&path,"GET",&uri.replace("byteBudget=60000",&format!("byteBudget={context_byte_budget}")),Value::Null,CANDIDATE).await;
                assert_eq!(status,StatusCode::OK,"long source reference: {reference_pack}");
                assert!(reference_pack.to_string().len()<=context_byte_budget);
                assert!(reference_pack["budgetsUsed"]["byteSize"].as_u64().unwrap()<=context_byte_budget as u64);
                let reference_result=json!({"content":[{"type":"text","text":reference_pack.to_string()}]});
                let reference_item=json!({"type":"function_call_output","call_id":"x".repeat(160),"output":reference_result.to_string()});
                assert!(eternallight_engine_core::dispatch("tokens.count",json!(reference_item.to_string())).unwrap().as_u64().unwrap()<=9000);
            }
            let (status,small)=call(&path,"GET",&uri.replace("byteBudget=60000","byteBudget=4096"),Value::Null,CANDIDATE).await;
            assert_eq!((status,small["error"]["code"].clone()),(StatusCode::UNPROCESSABLE_ENTITY,json!("context_budget_exceeded")));
        }
        let relevant_text = read_context_source(&path,&base,&task,&pack,"/relevantText").await;
        let source=relevant_text.as_array().unwrap().iter().find(|entry|entry["sourceRevisionId"]==saved["revisionId"]).unwrap();
        // Frozen task sources retain complete scene groups; the lightweight pack
        // carries their handles rather than a second body-stripped representation.
        assert_eq!((source["textRepresentation"].clone(),source["content"]["sceneGroups"].clone()),(json!("sceneGroups"),groups.clone()));
        let restored_body = source["content"]["sceneGroups"].as_array().unwrap().iter()
            .map(|group|group["body"].as_str().unwrap().trim()).collect::<Vec<_>>().join("\n\n---\n\n");
        assert_eq!(restored_body,body);
        assert_eq!(source["sourceRevisionId"],saved["revisionId"]);
        if operation == "assessSceneOutcome" {
            assert!(pack["operationContext"].get("requiredSourceReads").is_none());
            assert_eq!(pack["coverage"]["complete"],false);
            assert!(pack["coverage"]["missingScope"].as_array().unwrap().contains(&json!("taskSourceContents")));
            // The agent may additionally choose exact passage evidence. No host
            // script supplies this plan: it is a real, bounded query chosen here.
            let mut request=json!({"threadId":operation,"expectedRevision":1,"plan":{
                "question":"核对冻结章节正文的完整证据和字符位置","byteBudget":9000,"ranking":{"type":"literal"},
                "steps":[{"id":"assessment-source","operation":{"type":"passages","cursor":null,"maxChars":400,"limit":2}}]}});
            for limit in [1,2,4] {
                let mut sample=request.clone();
                sample["plan"]["steps"][0]["operation"]["limit"]=json!(limit);
                let (status,report)=call(&path,"POST",&format!("{base}/tasks/{}/query",task["id"].as_str().unwrap()),sample,CANDIDATE).await;
                assert_eq!(status,StatusCode::OK,"{report}");
                let result=json!({"content":[{"type":"text","text":report.to_string()}]});
                let item=json!({"type":"function_call_output","call_id":"x".repeat(/*n*/160),"output":result.to_string()});
                let tokens=eternallight_engine_core::dispatch("tokens.count",json!(item.to_string())).unwrap();
                eprintln!("page sample limit={limit} bytes={} escapedTokens={tokens} truncated={} cursorWithheld={}",report.to_string().len(),report["coverage"]["responseTruncated"],report["steps"][0]["coverage"]["cursorWithheldAfterBudget"]);
            }
            let mut covered=vec![false;body.chars().count()];
            let mut pages=0;
            loop {
                let (status,report)=call(&path,"POST",&format!("{base}/tasks/{}/query",task["id"].as_str().unwrap()),request.clone(),CANDIDATE).await;
                assert_eq!(status,StatusCode::OK,"{report}");
                assert_ne!(report["coverage"]["responseTruncated"],true,"{report}");
                assert!(report.to_string().len()<=9000,"passage page exceeded its declared byte budget");
                pages+=1;
                for row in report["steps"][0]["results"].as_array().unwrap() {
                    let evidence=&report["evidence"][row["evidenceRef"].as_str().unwrap()];
                    assert_eq!((evidence["sourceRevisionId"].clone(),evidence["sourcePath"].clone()),(saved["revisionId"].clone(),json!("/body")));
                    let start=evidence["start"].as_u64().unwrap() as usize;
                    let end=evidence["end"].as_u64().unwrap() as usize;
                    assert_eq!(evidence["quote"],body.chars().skip(start).take(end-start).collect::<String>());
                    covered[start..end].fill(/*value*/ true);
                }
                let cursor=report["steps"][0]["nextCursor"].clone();
                if cursor.is_null() { break; }
                assert!(pages<20,"bounded pagination did not finish");
                request["plan"]["steps"][0]["operation"]["cursor"]=cursor;
            }
            assert!(pages>1);
            assert!(covered.into_iter().all(|covered|covered));
        }
        assert!(source["content"].get("body").is_none(),"duplicate body reached frozen task source");
        let scene_links = read_context_source(&path,&base,&task,&pack,"/operationContext/scenePlanDraftLinks").await;
        let links=scene_links.as_array().unwrap();
        let linked=links.iter().find(|entry|entry["sceneId"]=="scene-1").unwrap();
        assert_eq!((linked["draftRevisionId"].clone(),linked["actualSceneGroupIds"].clone()),(saved["revisionId"].clone(),json!(["opening","ending"])));
        assert_eq!(linked["actualEventRefs"][0]["predicate"],"rang");
    }
    let (status,frozen)=call(&path,"POST",&format!("{base}/tasks"),create("assessSceneOutcome",
        json!({"chapterId":chapter,"chapterNumber":1,"draftRevisionId":saved["revisionId"],"planRevisionId":plan["revisionId"],"knowledgeBatchId":batch["id"]})),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{frozen}");
    let mut changed = payload;
    changed["body"] = json!("作者更新了本章正文。");
    changed["sceneGroups"] = json!([]);
    let (status,updated)=call(&path,"POST",&format!("{base}/drafts/checkpoints"),
        json!({"payload":changed,"expectedRevision":1,"expectedCandidateRevisionId":saved["revisionId"],"idempotencyKey":"author-update"}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{updated}");
    let (status,stale)=call(&path,"GET",&format!("{base}/context?taskId={}&threadId=assessSceneOutcome&projectRevision=1&purpose=critic&budget=24&byteBudget=60000",frozen["id"].as_str().unwrap()),Value::Null,CANDIDATE).await;
    assert_eq!(status, StatusCode::CONFLICT, "{stale}");
}

#[tokio::test]
async fn agent_can_draft_without_a_plan_and_cannot_invent_a_bound_position() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("direct-author-intent.sqlite3");
    let (base, mut request) = setup(&path).await;
    request["maxTaskAttempts"] = json!(3);
    request["maxProviderRequests"] = json!(3);
    let (status, run) = call(
        &path,
        "POST",
        &format!("{base}/generation-runs"),
        request,
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["maxTaskAttempts"], 3);
    let action =
        super::prepared_action(&path, &base, &run, "writeNextChapter", "direct-writer").await;
    assert_eq!(action["operation"], "generateChapter");
    assert_eq!(action["target"]["povCharacterId"], Value::Null);
    assert_eq!(action["target"]["storyTime"], Value::Null);
    let (_, workspace) = call(
        &path,
        "GET",
        &format!("{base}/creative-workspace"),
        Value::Null,
        AUTHOR,
    )
    .await;
    assert_eq!(workspace["chapterPlanBindings"], json!([]));
    assert_eq!(workspace["artifacts"]["plan"]["candidate"], Value::Null);
    assert_eq!(workspace["chapters"].as_array().unwrap().len(), 1);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let make_task = |target: Value| {
        json!({"operation":"generateChapter","inputRevision":workspace["revision"],
        "threadId":"direct-writer","target":target,"authorIntent":"忽略这个非权威包装文本",
        "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes())),"capabilityExpiresAt":now+600})
    };
    let mut forged = action["target"].clone();
    forged["povCharacterId"] = json!("invented-plan-character");
    let (status, rejected) = call(
        &path,
        "POST",
        &format!("{base}/tasks"),
        make_task(forged),
        AUTHOR,
    )
    .await;
    assert!(!status.is_success(), "{rejected}");
    let (status, writer) = call(
        &path,
        "POST",
        &format!("{base}/tasks"),
        make_task(action["target"].clone()),
        AUTHOR,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{writer}");
    let id = writer["id"].as_str().unwrap();
    let chapter = action["target"]["chapterId"].as_str().unwrap();
    let (status, context) = call(
        &path,
        "GET",
        &format!(
            "{base}/context?taskId={id}&threadId=direct-writer&projectRevision={}&purpose=writer",
            workspace["revision"]
        ),
        Value::Null,
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(context["plannedEffects"], json!([]));
    let (status, saved) = call(&path, "POST", &format!("{base}/drafts/checkpoints"), json!({
        "taskId":id,"threadId":"direct-writer","expectedRevision":workspace["revision"],
        "expectedCandidateRevisionId":null,"idempotencyKey":"direct-draft","completeness":"chapterComplete",
        "payload":{"chapterId":chapter,"chapterNumber":1,"title":"白塔来信","body":"雨中的白塔收到一封没有署名的信。","sceneGroups":[],
            "sourcePlanRefs":{"planRevisionId":"invented-plan","chapterPlanId":"invented-entry","localOnly":false,"lineageStatus":"current"}}}), CANDIDATE).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let stored: String = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE revision_id=?1",
            [saved["revisionId"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    let stored: Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(
        stored["sourcePlanRefs"],
        json!({"localOnly":true,"lineageStatus":"historical"})
    );
    let (status, completed) = call(
        &path,
        "POST",
        &format!("{base}/tasks/{id}/complete"),
        json!({"threadId":"direct-writer","summary":"依照作者意图起草，未伪造章规划"}),
        CANDIDATE,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    let project =
        novel_codex_story_core::ProjectId::parse(base.trim_start_matches("/api/projects/"))
            .unwrap();
    assert!(
        novel_codex_story_core::SqliteStoryRepository::open(&path)
            .unwrap()
            .validate_generation_chapter_checkpoint(&project, chapter, 1)
            .is_ok()
    );
    assert_eq!(
        novel_codex_story_core::SqliteStoryRepository::open(&path)
            .unwrap()
            .generation_start_position(&project)
            .unwrap()
            .completed_chapters,
        1
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM story_chapter_plan_bindings_v2",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[tokio::test]
async fn optional_planning_does_not_allow_a_stale_bound_plan_at_delivery() {
    let (_directory, path, base, plan, run) = materialized().await;
    let chapter = run["currentChapterId"].as_str().unwrap();
    let (status, saved) = call(&path,"POST",&format!("{base}/drafts/checkpoints"),
        json!({"payload":{"chapterId":chapter,"chapterNumber":1,"title":"来信","body":"雨中的白塔收到一封信。","sceneGroups":[]},
            "expectedRevision":1,"expectedCandidateRevisionId":null,"idempotencyKey":"planned-draft"}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let project =
        novel_codex_story_core::ProjectId::parse(base.trim_start_matches("/api/projects/"))
            .unwrap();
    let repository = novel_codex_story_core::SqliteStoryRepository::open(&path).unwrap();
    assert!(
        repository
            .validate_generation_chapter_checkpoint(&project, chapter, 1)
            .is_ok()
    );
    // Inspect the exact immutable fixture revision, not a UI projection.
    let db = rusqlite::Connection::open(&path).unwrap();
    let encoded: String = db
        .query_row(
            "SELECT content_json FROM story_revisions_v2 WHERE revision_id=?1",
            [plan["revisionId"].as_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    let mut payload = serde_json::from_str::<Value>(&encoded).unwrap()["payload"].clone();
    payload["chapters"][0]["goal"] = json!("作者调整了本章目标");
    let (status, replacement) = call(&path,"POST",&format!("{base}/artifacts/plan"),
        json!({"payload":payload,"expectedRevision":1,"expectedCandidateRevisionId":plan["revisionId"],"idempotencyKey":"new-plan"}),AUTHOR).await;
    assert_eq!(status, StatusCode::CREATED, "{replacement}");
    assert!(
        repository
            .validate_generation_chapter_checkpoint(&project, chapter, 1)
            .is_err()
    );
}
