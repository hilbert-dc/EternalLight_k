//! Real Runtime/SQLite issue references; scripted judgments are not literary evidence.
use serde_json::{Value, json};
#[path = "support/continuous_recovery.rs"]
mod support;
use support::{BODY, CANDIDATE, Runtime};
fn work(root: &str, run: &Value) -> String {
    format!(
        "{root}/work-tasks/work-{}/issues",
        run["id"].as_str().unwrap()
    )
}
async fn other_work_problem(r: &Runtime, root: &str, uri: &str) -> String {
    let issue = "other-work-problem";
    other_work_problems(r,root,uri,json!([{"issue":{"issueId":issue,"newIssue":{"summary":"第二任务自己的未完成要求"},"resolutionOf":[]},
        "observationId":null,"reason":"此问题属于第二个实际 Work"}])).await;
    issue.into()
}
async fn other_work_problems(r: &Runtime, root: &str, uri: &str, unresolved: Value) -> Value {
    let workspace = r.get(&format!("{root}/creative-workspace")).await;
    let task=r.post(&format!("{root}/tasks"),json!({
        "operation":"advisory","target":{"type":"project"},"inputRevision":workspace["revision"],
        "threadId":"other-problem-work","authorIntent":"核对第二个任务的未完成要求",
        "capabilityHash":format!("{:x}",<sha2::Sha256 as sha2::Digest>::digest(support::CAPABILITY.as_bytes())),
        "capabilityExpiresAt":r.get(uri).await["deadlineAt"],
        "initialBudget":{"authorizationId":"other-problem-budget","authorMessageId":"other-problem-author",
            "expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],
            "maxRequests":4,"maxTokens":4000,"deadlineAt":r.get(uri).await["deadlineAt"]}
    })).await;
    r.read_author_requirements(root, &task).await;
    r.candidate(&format!("{root}/tasks/{}/work-plan",task["id"].as_str().unwrap()),json!({
        "taskId":task["id"],"threadId":task["threadId"],"expectedRevision":task["inputRevision"],
        "expectedPlanVersion":0,"idempotencyKey":"other-work-execution",
        "plan":{"intentMode":"executeGoals","goals":[]}
    })).await;
    let read = r.decision_status(root, &task).await;
    r.candidate(&format!("{root}/tasks/{}/complete",task["id"].as_str().unwrap()),json!({
        "threadId":task["threadId"],"summary":"如实记录第二任务未完成要求",
        "deliveryReview":{"expectedWorkVersion":read["version"],"expectedSourceDigest":read["sourceDigest"],
            "goalStatus":"partial","goalAssessment":"该要求仍未解决","checkedScope":"完整原作者来源与真实当前任务",
            "unresolved":unresolved,
            "issueResolutions":[],"authorDecisions":[],"evidence":[]}
    })).await;
    task
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stable_issue_requires_explicit_current_recheck_and_preserves_dispute_and_history() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let (writer, _) = r.saved_chapter(&root, &uri, BODY).await;
    let negative = r.assess_chapter(&root, &uri, "unmet").await;
    let endpoint = work(&root, &r.get(&uri).await);
    let list = r.get(&endpoint).await;
    assert_eq!(list["totalCount"], 1);
    assert_eq!(list["entries"][0]["status"], "open");
    assert_eq!(list["entries"][0]["observations"], json!([]));
    let id = list["entries"][0]["issueId"].as_str().unwrap();
    let views = r
        .get(&format!("{endpoint}?issueId={id}&cursor=0&limit=16"))
        .await;
    assert_eq!(views["entries"][0]["taskId"], negative["id"]);
    assert_eq!(views["entries"][0]["current"], true);
    let first = views["entries"][0]["observationId"].clone();
    // A positive report without explicit reconciliation cannot erase the same
    // current negative opinion. A still-running critic cannot certify resolution.
    let positive = r
        .assess_chapter_options(
            &root,
            &uri,
            "supported",
            "另一评审提出正面意见但未调和原问题",
            &[],
            None,
            false,
            false,
        )
        .await;
    let report = r
        .get(&format!(
            "{root}/revision-outcomes/{}",
            writer["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(report["issues"][0]["status"], "open");
    let pending = r.get(&format!("{endpoint}?issueId={id}")).await;
    assert_eq!(pending["entries"][0]["current"], false);
    assert_eq!(pending["entries"][0]["recheckRequired"], true);
    r.complete_and_advance(&root, &uri, &positive).await;
    assert_eq!(r.get(&endpoint).await["entries"][0]["status"], "disputed");
    let recheck = r
        .assess_chapter_options(
            &root,
            &uri,
            "supported",
            "实际读取当前稿，显式重审同一问题的两个观点",
            &[],
            None,
            true,
            true,
        )
        .await;
    let list = r.get(&endpoint).await;
    assert_eq!(list["totalCount"], 1);
    assert_eq!(list["entries"][0]["status"], "resolved");
    let one = r.get(&format!("{endpoint}?issueId={id}&limit=1")).await;
    assert_eq!(one["entries"].as_array().unwrap().len(), 1);
    assert_eq!(one["entries"][0]["taskId"], recheck["id"]);
    assert!(
        one["entries"][0]["resolutionOf"]
            .as_array()
            .unwrap()
            .contains(&first)
    );
    let next = one["nextCursor"].as_u64().unwrap();
    let rest = r
        .get(&format!("{endpoint}?issueId={id}&cursor={next}&limit=16"))
        .await;
    assert_eq!(rest["entries"].as_array().unwrap().len(), 2);
    assert!(
        rest["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["superseded"] == true)
    );
    assert_eq!(
        rest["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["observationId"] == first)
            .unwrap()["evidence"][0]["quote"],
        BODY
    );
    let coordinator = r.next_task(&root, &uri, "finish").await;
    let current = r.decision_status(&root, &coordinator).await;
    let target = &current["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == "finishChapter")
        .unwrap();
    let mut request = json!({"taskId":coordinator["id"],"threadId":coordinator["threadId"],"expectedRevision":coordinator["inputRevision"],"expectedWorkVersion":current["version"],"expectedSourceDigest":current["sourceDigest"],"actionKey":target["actionKey"],"instruction":"引用当前独立结论","idempotencyKey":"forged-problem-ref","chapterReview":{"sourceRevisionId":target["sourceRevisionId"],"assessmentRevisionId":"invented-report","checks":[{"criterionId":"whole-chapter","issueId":id}],"length":current["chapterAcceptance"]["length"]}});
    let (code, _) = r
        .call(
            "POST",
            &format!(
                "{root}/tasks/{}/work-decision/actions",
                coordinator["id"].as_str().unwrap()
            ),
            CANDIDATE,
            Some(request.take()),
        )
        .await;
    assert!(!code.is_success());
    let receipt = r.choose(&root, &coordinator, "finishChapter").await;
    assert_eq!(
        receipt["chapterReviewReceipt"]["assessmentRevisionId"],
        r.get(&format!(
            "{root}/revision-outcomes/{}",
            writer["id"].as_str().unwrap()
        ))
        .await["revisionId"]
    );
    let done = r.complete_and_advance(&root, &uri, &coordinator).await;
    assert_eq!(done["status"], "completed");
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn new_prose_does_not_resolve_old_problem_and_simple_supported_path_adds_no_issue() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    r.saved_chapter(&root, &uri, BODY).await;
    r.assess_chapter(&root, &uri, "unmet").await;
    let endpoint = work(&root, &r.get(&uri).await);
    let old = r.get(&endpoint).await["entries"][0]["issueId"].clone();
    let decision = r.next_task(&root, &uri, "repair").await;
    r.choose(&root, &decision, "generateChapter").await;
    r.complete_and_advance(&root, &uri, &decision).await;
    let writer = r.next_task(&root, &uri, "repair-writer").await;
    for suffix in [String::new(), format!("&issueId={}", old.as_str().unwrap())] {
        let (code, actual) = r
            .call(
                "GET",
                &format!(
                    "{root}/tasks/{}/work-issues?threadId={}&expectedRevision={}{}",
                    writer["id"].as_str().unwrap(),
                    writer["threadId"].as_str().unwrap(),
                    writer["inputRevision"],
                    suffix
                ),
                CANDIDATE,
                None,
            )
            .await;
        assert!(
            code.is_success(),
            "a genuinely whole-chapter Writer may read the same chapter's actual problems: {actual}"
        );
    }
    r.save_writer(
        &root,
        &writer,
        "林夕重新核对灯光来源并记录调查证据。",
        "actual-repair",
    )
    .await;
    r.complete_and_advance(&root, &uri, &writer).await;
    assert_eq!(r.get(&endpoint).await["entries"][0]["status"], "stale");
    r.assess_chapter_options(
        &root,
        &uri,
        "supported",
        "当前稿正评仍没明确重审旧问题",
        &[],
        None,
        false,
        true,
    )
    .await;
    assert_eq!(r.get(&endpoint).await["entries"][0]["status"], "stale");
    r.assess_chapter_options(
        &root,
        &uri,
        "supported",
        "当前实际稿补读并重审旧问题",
        &[],
        None,
        true,
        true,
    )
    .await;
    assert_eq!(r.get(&endpoint).await["entries"][0]["issueId"], old);
    assert_eq!(r.get(&endpoint).await["entries"][0]["status"], "resolved");
    let unrelated = Runtime::new().await;
    let (other, run) = unrelated.start().await;
    unrelated.saved_chapter(&other, &run, BODY).await;
    unrelated.assess_chapter(&other, &run, "supported").await;
    assert_eq!(
        unrelated
            .get(&work(&other, &unrelated.get(&run).await))
            .await["totalCount"],
        0
    );
    let workspace = r.get(&format!("{root}/creative-workspace")).await;
    let foreign=r.post(&format!("{root}/tasks"),json!({"operation":"advisory","target":{"type":"project"},"inputRevision":workspace["revision"],"threadId":"foreign-issues","authorIntent":"只核对另一个任务自己的状态","capabilityHash":format!("{:x}",<sha2::Sha256 as sha2::Digest>::digest(support::CAPABILITY.as_bytes())),"capabilityExpiresAt":r.get(&uri).await["deadlineAt"],"initialBudget":{"authorizationId":"foreign-issue-budget","authorMessageId":"foreign-issue-author","expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],"maxRequests":4,"maxTokens":4000,"deadlineAt":r.get(&uri).await["deadlineAt"]}})).await;
    let (foreign_code,_)=r.call("GET",&format!("{root}/tasks/{}/work-issues?threadId=foreign-issues&expectedRevision={}&issueId={}",foreign["id"].as_str().unwrap(),foreign["inputRevision"],old.as_str().unwrap()),CANDIDATE,None).await;
    assert!(
        !foreign_code.is_success(),
        "a real second Work cannot inspect the first Work's actual issue"
    );
    let (code, _) = r
        .call(
            "GET",
            &format!("{endpoint}?issueId=other-work-issue"),
            support::AUTHOR,
            None,
        )
        .await;
    assert!(!code.is_success());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_scene_limited_writer_grants_cannot_read_issue_metadata_or_history() {
    for scope_type in ["throughScene", "selectedScenes"] {
        let r = Runtime::new().await;
        let (root, uri) = r.start().await;
        let (original, saved) = r.saved_chapter(&root, &uri, BODY).await;
        r.assess_chapter(&root, &uri, "unmet").await;
        let actual_issue =
            r.get(&work(&root, &r.get(&uri).await)).await["entries"][0]["issueId"].clone();
        let workspace = r.get(&format!("{root}/creative-workspace")).await;
        let draft = &workspace["chapters"][0]["candidate"];
        let scope = if scope_type == "throughScene" {
            json!({"type":"throughScene","endSceneOrder":1})
        } else {
            json!({"type":"selectedScenes","sourceRevisionId":saved["revisionId"],
                "groupIds":[draft["sceneGroups"][0]["id"]]})
        };
        let task = r.post(&format!("{root}/tasks"), json!({
            "operation":"generateChapter","target":{"type":"chapter",
                "chapterId":original["target"]["chapterId"],"chapterNumber":1,
                "readingChapter":1,"sceneOrder":1,"scope":scope,"readScope":{"type":"throughScene","endSceneOrder":1}},
            "inputRevision":workspace["revision"],"threadId":"limited-writer",
            "authorIntent":"仅修订指定场景，不读取范围外的正文或相关报告",
            "capabilityHash":format!("{:x}",<sha2::Sha256 as sha2::Digest>::digest(support::CAPABILITY.as_bytes())),
            "capabilityExpiresAt":r.get(&uri).await["deadlineAt"],
            "initialBudget":{"authorizationId":"limited-issue-budget","authorMessageId":"limited-issue-author",
                "expectedBaseSnapshotId":workspace["headSnapshotId"],"expectedSourceDigest":workspace["sourceDigest"],
                "maxRequests":4,"maxTokens":4000,"deadlineAt":r.get(&uri).await["deadlineAt"]}
        })).await;
        assert_eq!(task["target"]["scope"]["type"], scope_type);
        for suffix in [
            String::new(),
            format!("&issueId={}", actual_issue.as_str().unwrap()),
        ] {
            let (code, rejected) = r
                .call(
                    "GET",
                    &format!(
                        "{root}/tasks/{}/work-issues?threadId={}&expectedRevision={}{}",
                        task["id"].as_str().unwrap(),
                        task["threadId"].as_str().unwrap(),
                        task["inputRevision"],
                        suffix
                    ),
                    CANDIDATE,
                    None,
                )
                .await;
            assert!(code.is_client_error(), "{scope_type}: {rejected}");
            assert!(
                rejected["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("explicit reading fence"),
                "scope rejects metadata before looking up potentially hidden problem facts: {rejected}"
            );
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_read_prior_dependency_invalidates_issue_even_with_same_current_draft() {
    let r = Runtime::new().await;
    let (root, uri) = r.start_with_chapters(2).await;
    let (first, saved) = r.saved_chapter(&root, &uri, BODY).await;
    r.finish_chapter(&root, &uri).await;
    let (second, _) = r
        .saved_chapter(&root, &uri, "林夕依据前章灯光记录继续调查。")
        .await;
    r.assess_chapter_report_with_prior(
        &root,
        &uri,
        "unmet",
        "根据真实已读前章定位尚未解决的问题",
        &[],
        Some(1),
    )
    .await;
    let endpoint = work(&root, &r.get(&uri).await);
    let before = r.get(&endpoint).await;
    let id = before["entries"][0]["issueId"].as_str().unwrap();
    assert_eq!(before["entries"][0]["status"], "open");
    let workspace = r.get(&format!("{root}/creative-workspace")).await;
    r.post(&format!("{root}/drafts/checkpoints"),json!({"expectedRevision":workspace["revision"],"expectedCandidateRevisionId":saved["revisionId"],"idempotencyKey":"prior-source-revised","payload":{"chapterId":first["target"]["chapterId"],"chapterNumber":1,"title":"真正前文变化","body":"林夕在前章确认灯光来自另一条航道。","completeness":"chapterComplete","sceneGroups":[]}})).await;
    let after = r.get(&endpoint).await;
    assert_eq!(after["entries"][0]["status"], "stale");
    let views = r.get(&format!("{endpoint}?issueId={id}")).await;
    assert_eq!(views["entries"][0]["current"], false);
    assert_eq!(views["entries"][0]["recheckRequired"], true);
    assert_eq!(
        views["entries"][0]["draftRevisionId"],
        r.get(&format!(
            "{root}/revision-outcomes/{}",
            second["id"].as_str().unwrap()
        ))
        .await["assessment"]["draftRevisionId"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_actual_report_can_associate_multiple_criteria_with_one_new_problem_atomically() {
    for variant in [
        "repeat-definition",
        "reference-before-definition",
        "definition-before-reference",
    ] {
        let r = Runtime::new().await;
        let (root, uri) = r.start().await;
        let decision = r.next_task(&root, &uri, "two-criteria").await;
        r.choose_with_acceptance(&root,&decision,"writeNextChapter",Some(json!({
            "requirements":[{"id":"evidence-origin","text":"交代灯光来源"},{"id":"evidence-action","text":"调查行动沿用同一证据"}],
            "length":{"requirement":"作者未指定篇幅","minCharacters":null,"maxCharacters":null}
        }))).await;
        r.complete_and_advance(&root, &uri, &decision).await;
        let writer = r.next_task(&root, &uri, "writer").await;
        r.save_writer(&root, &writer, BODY, "two-criterion-body")
            .await;
        r.complete_and_advance(&root, &uri, &writer).await;
        let foreign_issue = other_work_problem(&r, &root, &uri).await;
        let project =
            novel_codex_story_core::ProjectId::parse(root.rsplit('/').next().unwrap()).unwrap();
        let issue_id = "shared-evidence-problem";
        let original_critic=r.assess_chapter_transformed(&root,&uri,"unmet","两个标准涉及模型选定的同一证据问题",&[],None,false,false,|critic,mut payload| {
            let definition=json!({"issueId":issue_id,"newIssue":{"summary":"证据来源与调查行动的对应关系仍未交代"},"resolutionOf":[]});
            let reference=json!({"issueId":issue_id,"newIssue":null,"resolutionOf":[]});
            payload["outcomes"][0]["issue"]=if variant=="reference-before-definition"{reference.clone()}else{definition.clone()};
            payload["outcomes"][1]["issue"]=if variant=="definition-before-reference"{reference}else{definition};
            let repo=novel_codex_story_core::SqliteStoryRepository::open(&r.path).unwrap();
            let task=repo.task(&project,critic["id"].as_str().unwrap()).unwrap().unwrap();
            let mut conflict=payload.clone();
            conflict["outcomes"][0]["issue"]["newIssue"]=json!({"summary":"另一问题定义"});
            conflict["outcomes"][1]["issue"]["newIssue"]=json!({"summary":"原问题定义"});
            assert!(repo.prepare_revision_outcome_payload(&task,&conflict).is_err());
            let mut forged=payload.clone();
            forged["outcomes"][0]["issue"]["resolutionOf"]=json!(["unknown-observation"]);
            assert!(repo.prepare_revision_outcome_payload(&task,&forged).is_err());
            let mut foreign=payload.clone();
            foreign["outcomes"][0]["issue"]=json!({"issueId":foreign_issue,"newIssue":null,"resolutionOf":[]});
            assert!(repo.prepare_revision_outcome_payload(&task,&foreign).is_err(),
                "a real issue registered by another author's Work cannot be reassociated by this report");
            assert_eq!(repo.task_work_issues(&task,None,0,16).unwrap()["totalCount"],0,
                "rejected definitions and fabricated references must not register any issue");
            payload
        }).await;
        let endpoint = work(&root, &r.get(&uri).await);
        let first_views = r.get(&format!("{endpoint}?issueId={issue_id}")).await;
        let saved_report = r
            .get(&format!(
                "{root}/revision-outcomes/{}",
                writer["id"].as_str().unwrap()
            ))
            .await;
        let replay=r.candidate(&format!("{root}/revision-outcomes/{}",writer["id"].as_str().unwrap()),json!({
            "taskId":original_critic["id"],"threadId":original_critic["threadId"],"expectedRevision":original_critic["inputRevision"],
            "expectedCandidateRevisionId":null,"idempotencyKey":format!("independent-report-{}",original_critic["id"].as_str().unwrap()),
            "payload":saved_report["assessment"]
        })).await;
        assert_eq!(replay["revisionId"], saved_report["revisionId"]);
        assert_eq!(
            r.get(&format!("{endpoint}?issueId={issue_id}")).await,
            first_views,
            "replay retains the exact Native problem and viewpoint identities"
        );
        r.complete_and_advance(&root, &uri, &original_critic).await;
        let metadata = r.get(&endpoint).await;
        assert_eq!(metadata["totalCount"], 1);
        assert_eq!(metadata["entries"][0]["observationCount"], 2);
        assert_eq!(metadata["entries"][0]["issueId"], issue_id);
        let actual = r.get(&format!("{endpoint}?issueId={issue_id}")).await;
        assert_eq!(actual["entries"].as_array().unwrap().len(), 2);
        assert!(
            actual["entries"]
                .as_array()
                .unwrap()
                .iter()
                .all(|o| o["current"] == true)
        );
        assert_eq!(
            actual["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|o| o["criterionId"].as_str().unwrap())
                .collect::<std::collections::BTreeSet<_>>(),
            ["evidence-origin", "evidence-action"].into_iter().collect()
        );
        let before = actual.clone();
        // A later evaluator cannot reuse a first-declaration payload to replace
        // the identity, even when its description is identical.
        let coordinator = r.next_task(&root, &uri, "redeclare").await;
        r.choose(&root, &coordinator, "assessRevisionOutcome").await;
        r.complete_and_advance(&root, &uri, &coordinator).await;
        let critic = r.next_task(&root, &uri, "redeclare-critic").await;
        r.read_author_requirements(&root, &critic).await;
        let repo = novel_codex_story_core::SqliteStoryRepository::open(&r.path).unwrap();
        let current = repo
            .revision_outcome(&project, writer["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        let (code,rejected)=r.call("POST",&format!("{root}/revision-outcomes/{}",writer["id"].as_str().unwrap()),CANDIDATE,Some(json!({
            "taskId":critic["id"],"threadId":critic["threadId"],"expectedRevision":critic["inputRevision"],
            "expectedCandidateRevisionId":current.revision_id,"idempotencyKey":"illegal-later-first-declaration","payload":current.content
        }))).await;
        assert!(code.is_client_error());
        assert!(
            rejected["error"]["message"]
                .as_str()
                .unwrap()
                .contains("existing issue must be selected with newIssue:null"),
            "actual later-report save must reach the redefinition fence: {rejected}"
        );
        assert_eq!(
            r.get(&format!("{endpoint}?issueId={issue_id}")).await,
            before,
            "failed later redeclaration must roll back report and viewpoint changes"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_maximum_unicode_metadata_pages_preserve_full_summaries_and_real_keyset() {
    let r = Runtime::new().await;
    let (root, uri) = r.start().await;
    let mut expected = std::collections::BTreeMap::new();
    let mut unresolved = Vec::new();
    for n in 0..16 {
        let id = format!("unicode-problem-{n}");
        let title = format!("问题{n}：");
        let summary = format!("{title}{}", "🕯".repeat(600 - title.chars().count()));
        assert_eq!(summary.chars().count(), 600);
        unresolved.push(
            json!({"issue":{"issueId":id,"newIssue":{"summary":summary},"resolutionOf":[]},
            "observationId":null,"reason":"尚待模型逐项核对的实际记录"}),
        );
        expected.insert(id, summary);
    }
    let task = other_work_problems(&r, &root, &uri, json!(unresolved)).await;
    let endpoint = format!(
        "{root}/work-tasks/work-{}/issues",
        task["id"].as_str().unwrap()
    );
    let mut returned = std::collections::BTreeMap::new();
    let mut cursor = 0;
    let mut pages = 0;
    loop {
        let page = r.get(&format!("{endpoint}?cursor={cursor}&limit=16")).await;
        assert!(
            page.to_string().len() <= 32_768,
            "actual UTF-8 package must stay bounded"
        );
        assert_eq!(page["totalCount"], 16);
        let entries = page["entries"].as_array().unwrap();
        assert!(!entries.is_empty());
        for entry in entries {
            assert_eq!(
                entry["observations"],
                json!([]),
                "overview does not preload report bodies"
            );
            let id = entry["issueId"].as_str().unwrap().to_owned();
            assert!(
                returned
                    .insert(id, entry["summary"].as_str().unwrap().to_owned())
                    .is_none(),
                "keyset pages must not repeat an issue"
            );
        }
        pages += 1;
        let Some(next) = page["nextCursor"].as_u64() else {
            break;
        };
        assert_ne!(next, cursor);
        cursor = next;
        assert!(pages < 16);
    }
    assert!(
        pages > 1,
        "largest valid Unicode metadata must shrink a nominal sixteen-entry page"
    );
    assert_eq!(
        returned, expected,
        "every original summary is returned in full, without loss or truncation"
    );
}

#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn selected_write_scope_can_read_its_own_work_issues_without_a_read_fence() {
    let r=Runtime::new().await;let (root,uri)=r.start().await;
    let (writer,saved)=r.saved_chapter(&root,&uri,BODY).await;
    r.assess_chapter(&root,&uri,"unmet").await;
    let decision=r.next_task(&root,&uri,"coordinator").await;
    let state=r.decision_status(&root,&decision).await;
    let workspace=r.get(&format!("{root}/creative-workspace")).await;
    let group=&workspace["chapters"][0]["candidate"]["sceneGroups"][0]["id"];
    r.candidate(&format!("{root}/tasks/{}/work-decision/actions",decision["id"].as_str().unwrap()),json!({"taskId":decision["id"],"threadId":decision["threadId"],"expectedRevision":decision["inputRevision"],"expectedWorkVersion":state["version"],"expectedSourceDigest":state["sourceDigest"],"proposal":{"operation":"generateChapter","target":{"chapterId":writer["target"]["chapterId"],"scope":{"type":"selectedScenes","sourceRevisionId":saved["revisionId"],"groupIds":[group]}}},"chapterAcceptance":state["chapterAcceptance"],"instruction":"只修改所选场景，读取本章问题以定位原因","idempotencyKey":"selected-read-separate"})).await;
    r.complete_and_advance(&root,&uri,&decision).await;
    let writer=r.next_task(&root,&uri,"writer").await;
    let issues=r.ok("GET",&format!("{root}/tasks/{}/work-issues?threadId={}&expectedRevision={}&cursor=0&limit=8",writer["id"].as_str().unwrap(),writer["threadId"].as_str().unwrap(),writer["inputRevision"]),CANDIDATE,None).await;
    assert!(!issues["entries"].as_array().unwrap().is_empty(),"{issues}");
}

use novel_codex_story_core::{RevisionOutcomeRepository, RevisionOutcomeValidationRepository, TasksRepository, WorkIssuesRepository};
