//! Controlled model choices over the real, scoped Runtime decision protocol.
//! Optional creative methods are not a prescribed production pipeline.
#[path = "continuous_recovery.rs"]
mod support;

use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};

use eternallight_engine_core::PersistentBudget;
use pretty_assertions::assert_eq;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use support::{CANDIDATE, Runtime};

const ORIGINAL: &str = "记忆逐渐消失的港城";
const FIRST_UNMET: &str = "林夕走过港城。邻人认得每一位亲友，所有人的记忆完好，旧账也无一缺失。";
const FIRST_REPAIRED: &str = "港城每夜遗失一段记忆。林夕发现邻人已认不出母亲，白塔旧账上却还留着她的名字。他把名字抄进衣袖，决定追查记忆消失的原因。";
const SECOND: &str = "港城又失去了一夜的记忆。第一场，林夕带着衣袖上的名字走到白塔，守门人已经忘了昨日的访客。第二场，他在月光🌙下摊开白塔账簿，同一名字旁有两次不同的登记，纸上的旧印与母亲留下的收据吻合。结尾，林夕仍决定追查记忆消失的原因。";
const THIRD: &str = "港城的人们继续失去记忆。林夕沿着第二章白塔账簿的登记寻找抄写人，对方虽已忘记母亲，却指认了纸上相同的旧印。林夕将证据收好，仍决定追查记忆消失的原因。";
const FOURTH_INTENT: &str = "第四章让林夕追查白塔记录";
const FOURTH: &str = "港城又有一条街忘了昨夜。林夕拿着先前的旧印与收据追查白塔记录，发现被划去的登记仍透出母亲的名字。他顺着抄写人的笔迹走进塔内，继续寻找记忆消失的原因。";

pub async fn three_chapters_with_feedback(repair_instruction: &str) {
    let runtime = Runtime::new().await;
    let project = runtime
        .post(
            "/api/projects",
            json!({
                "medium":"novel", "idempotencyKey":uuid::Uuid::new_v4().to_string(),
                "title":"港城", "logline":""
            }),
        )
        .await;
    let root = format!(
        "/api/projects/{}",
        project["project"]["id"].as_str().unwrap()
    );
    let deadline = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let created = start(&runtime, &root, ORIGINAL, 3, 24, 64, deadline).await;
    let uri = format!("{root}/generation-runs/{}", created["id"].as_str().unwrap());
    let work_uri = format!("{root}/work-tasks/work-{}", created["id"].as_str().unwrap());
    assert_eq!(created["stage"], "coordinate");
    assert_eq!(created["intentRevisions"].as_array().unwrap().len(), 1);
    assert_eq!(created["intentRevisions"][0]["instruction"], ORIGINAL);
    assert_eq!(
        created["intentRevisions"][0]["source"]["type"],
        "authorMessage"
    );
    assert_eq!(message_count(&runtime, &created), 1);

    let (first_writer, first_saved) =
        write(&runtime, &root, &uri, FIRST_UNMET, 1, None, None).await;
    let first_revision = first_saved["revisionId"].as_str().unwrap().to_owned();
    assert_saved(&runtime, &first_writer, &first_saved, FIRST_UNMET);
    let first_critic = assess(
        &runtime,
        &root,
        &uri,
        "unmet",
        "已保存正文明确写所有人的记忆完好，没有呈现作者要求的记忆逐渐消失",
        None,
    )
    .await;
    assert_ne!(first_critic["threadId"], first_writer["threadId"]);
    assert_eq!(first_critic["target"]["writerTaskId"], first_writer["id"]);
    assert_eq!(
        first_critic["target"]["draftRevisionId"],
        first_saved["revisionId"]
    );
    let failed_report = report(&runtime, &root, &first_writer, "unmet").await;
    let issue_id = failed_report["assessment"]["outcomes"][0]["issue"]["issueId"].clone();
    assert!(issue_id.is_string());
    assert!(!failed_report["issues"].as_array().unwrap().is_empty());
    let original_observations = {
        let db = Connection::open(&runtime.path).unwrap();
        let mut query = db.prepare("SELECT observation_id FROM story_issue_observations_v1 WHERE issue_id=?1 AND report_revision_id=?2 ORDER BY observation_id").unwrap();
        query
            .query_map(
                params![
                    issue_id.as_str().unwrap(),
                    failed_report["revisionId"].as_str().unwrap()
                ],
                |row| row.get::<_, String>(0),
            )
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
    };
    assert!(!original_observations.is_empty());
    let history_before = runtime.get(&uri).await;
    let messages_before = message_count(&runtime, &history_before);
    if !repair_instruction.is_empty() {
        let revised = runtime
            .post(
                &format!("{uri}/future-intents"),
                json!({
                    "expectedVersion":history_before["version"], "instruction":repair_instruction
                }),
            )
            .await;
        let intents = revised["intentRevisions"].as_array().unwrap();
        assert_eq!(
            &intents[..1],
            history_before["intentRevisions"]
                .as_array()
                .unwrap()
                .as_slice()
        );
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[1]["instruction"], repair_instruction);
        assert_eq!(intents[1]["effectiveFromChapter"], 2);
        assert_eq!(intents[1]["generation"], 1);
        assert_eq!(intents[1]["source"]["type"], "authorMessage");
        assert_eq!(
            revised["feedbackDecisions"],
            history_before["feedbackDecisions"]
        );
        assert_eq!(message_count(&runtime, &revised), messages_before + 1);
        let db = Connection::open(&runtime.path).unwrap();
        let persisted: (String, String, i64) = db.query_row(
            "SELECT author_text,work_task_id,intent_revision FROM story_author_messages_v1 WHERE message_id=?1 AND generation_run_id=?2",
            params![intents[1]["source"]["authorMessageId"].as_str().unwrap(), created["id"].as_str().unwrap()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        ).unwrap();
        assert_eq!(
            persisted,
            (
                repair_instruction.to_owned(),
                format!("work-{}", created["id"].as_str().unwrap()),
                1
            )
        );
    } else {
        assert_eq!(
            runtime.get(&uri).await["intentRevisions"],
            history_before["intentRevisions"]
        );
        assert_eq!(message_count(&runtime, &history_before), messages_before);
        assert_eq!(history_before["feedbackDecisions"], json!([]));
    }

    // The actual negative report makes delivery unavailable. The model selects
    // another admitted writer over that current issue, without another author prompt.
    let (repair_writer, repaired) = write(
        &runtime,
        &root,
        &uri,
        FIRST_REPAIRED,
        1,
        None,
        Some((&first_critic, &failed_report)),
    )
    .await;
    assert_ne!(repair_writer["id"], first_writer["id"]);
    assert_ne!(repaired["revisionId"], first_saved["revisionId"]);
    assert_eq!(
        repair_writer["inputManifest"]["expectedCandidateRevisionId"],
        first_saved["revisionId"]
    );
    assert_saved(&runtime, &repair_writer, &repaired, FIRST_REPAIRED);
    assert_historical_body(&runtime, &first_revision, FIRST_UNMET);
    assert_eq!(
        runtime
            .get(&format!(
                "{root}/revision-outcomes/{}",
                first_writer["id"].as_str().unwrap()
            ))
            .await["state"],
        "stale"
    );
    let recheck = assess(
        &runtime,
        &root,
        &uri,
        "supported",
        "修订正文显示邻人逐渐失去对母亲的记忆；当前白塔账簿与衣袖记录承接调查",
        None,
    )
    .await;
    assert_ne!(recheck["id"], first_critic["id"]);
    assert_ne!(recheck["threadId"], repair_writer["threadId"]);
    let passed = report(&runtime, &root, &repair_writer, "supported").await;
    let resolved = &passed["assessment"]["outcomes"][0]["issue"];
    assert_eq!(resolved["issueId"], issue_id);
    assert!(!resolved["resolutionOf"].as_array().unwrap().is_empty());
    let mut resolved_observations = resolved["resolutionOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    resolved_observations.sort();
    assert_eq!(resolved_observations, original_observations);
    assert!(
        passed["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["issueId"] == issue_id && issue["status"] == "resolved")
    );
    let after_first = finish(&runtime, &root, &uri, &repair_writer, &passed, false).await;
    assert_eq!(after_first["completedChapters"], 1);
    assert_eq!(after_first["status"], "running");

    let note = (!repair_instruction.is_empty()).then_some(repair_instruction);
    let (second_writer, second_saved) = write(&runtime, &root, &uri, SECOND, 2, note, None).await;
    let second_critic = assess(
        &runtime,
        &root,
        &uri,
        "supported",
        "港城又失去一夜记忆，第二场通过白塔账簿旧印和收据提供证据，结尾调查方向保持",
        Some(1),
    )
    .await;
    assert_ne!(second_critic["threadId"], second_writer["threadId"]);
    let second_report = report(&runtime, &root, &second_writer, "supported").await;
    assert_eq!(
        finish(&runtime, &root, &uri, &second_writer, &second_report, false).await["completedChapters"],
        2
    );

    let (third_writer, third_saved) = write(&runtime, &root, &uri, THIRD, 3, note, None).await;
    assess(
        &runtime,
        &root,
        &uri,
        "supported",
        "抄写人忘记母亲但旧印还在，记忆消失继续推进，并承接第二章实际账簿证据",
        Some(2),
    )
    .await;
    let third_report = report(&runtime, &root, &third_writer, "supported").await;
    let finished = finish(&runtime, &root, &uri, &third_writer, &third_report, true).await;
    assert_eq!(finished["status"], "completed");
    assert_eq!(finished["stage"], "finished");
    assert_eq!(finished["completedChapters"], 3);
    assert_eq!(finished["targetChapters"], 3);
    assert_eq!(finished["maxTaskAttempts"], 24);
    assert_eq!(finished["deadlineAt"], deadline);
    assert_eq!(finished["providerBudget"]["maxRequests"], 64);
    assert_eq!(finished["providerBudget"]["maxTokens"], 4_000_000);
    assert_eq!(finished["providerBudget"]["usedRequests"], 64);
    assert_eq!(finished["providerBudget"]["usedTokens"], 448);
    assert_eq!(finished["providerBudget"]["unknownTokens"], 0);
    assert_eq!(finished["providerBudget"]["overrun"], false);
    assert_eq!(finished["feedbackDecisions"], json!([]));
    assert_eq!(
        message_count(&runtime, &finished),
        if repair_instruction.is_empty() { 1 } else { 2 }
    );
    let finished_work = assert_work(&runtime, &root, &work_uri, &finished, ORIGINAL).await;
    let original_drafts = draft_heads(&runtime);
    assert_eq!(original_drafts.len(), 3);
    assert_saved(&runtime, &repair_writer, &repaired, FIRST_REPAIRED);
    assert_saved(&runtime, &second_writer, &second_saved, SECOND);
    assert_saved(&runtime, &third_writer, &third_saved, THIRD);
    assert_no_adoption(&runtime);
    assert_historical_body(&runtime, &first_revision, FIRST_UNMET);
    let stopped = runtime.post(&format!("{uri}/next"), json!({})).await;
    assert_eq!(stopped["run"], finished);
    assert_ne!(stopped["action"]["kind"], "agentTask");

    // A new, explicit author scope starts at the real frontier; it cannot reopen
    // the completed three-chapter Work or manufacture another old allowance.
    let continued = start(&runtime, &root, FOURTH_INTENT, 1, 7, 256, deadline).await;
    assert_ne!(continued["id"], created["id"]);
    assert_eq!(continued["startChapterNumber"], 4);
    assert_eq!(continued["completedChapters"], 3);
    assert_eq!(continued["targetChapters"], 4);
    assert_eq!(
        continued["intentRevisions"][0]["instruction"],
        FOURTH_INTENT
    );
    assert_eq!(continued["intentRevisions"][0]["effectiveFromChapter"], 4);
    assert_eq!(continued["maxTaskAttempts"], 7);
    assert_eq!(continued["deadlineAt"], deadline);
    let next_uri = format!(
        "{root}/generation-runs/{}",
        continued["id"].as_str().unwrap()
    );
    let next_work_uri = format!(
        "{root}/work-tasks/work-{}",
        continued["id"].as_str().unwrap()
    );
    let (fourth_writer, fourth_saved) =
        write(&runtime, &root, &next_uri, FOURTH, 4, None, None).await;
    assess(
        &runtime,
        &root,
        &next_uri,
        "supported",
        "林夕持旧印与收据追查白塔记录，原章节证据与记忆消失的港城延续",
        Some(3),
    )
    .await;
    let fourth_report = report(&runtime, &root, &fourth_writer, "supported").await;
    let fourth_finished = finish(
        &runtime,
        &root,
        &next_uri,
        &fourth_writer,
        &fourth_report,
        false,
    )
    .await;
    assert_eq!(fourth_finished["status"], "completed");
    assert_eq!(fourth_finished["stage"], "finished");
    assert_eq!(fourth_finished["completedChapters"], 4);
    assert_eq!(fourth_finished["maxTaskAttempts"], 7);
    assert_eq!(fourth_finished["providerBudget"]["maxRequests"], 256);
    assert_eq!(fourth_finished["providerBudget"]["maxTokens"], 4_000_000);
    assert_eq!(
        fourth_finished["providerBudget"]["usedRequests"],
        fourth_finished["usedTaskAttempts"]
    );
    assert_eq!(fourth_finished["deadlineAt"], deadline);
    assert_work(
        &runtime,
        &root,
        &next_work_uri,
        &fourth_finished,
        FOURTH_INTENT,
    )
    .await;
    assert_saved(&runtime, &fourth_writer, &fourth_saved, FOURTH);
    assert_eq!(runtime.get(&uri).await, finished);
    assert_eq!(runtime.get(&work_uri).await, finished_work);
    let all_drafts = draft_heads(&runtime);
    assert_eq!(all_drafts.len(), 4);
    assert_eq!(&all_drafts[..3], original_drafts.as_slice());
    assert_no_adoption(&runtime);
    assert_historical_body(&runtime, &first_revision, FIRST_UNMET);
}

async fn start(
    runtime: &Runtime,
    root: &str,
    intent: &str,
    chapters: i64,
    attempts: i64,
    requests: i64,
    deadline: u64,
) -> Value {
    let workspace = runtime.get(&format!("{root}/creative-workspace")).await;
    runtime.post(&format!("{root}/generation-runs"), json!({
        "expectedProjectRevision":workspace["revision"], "expectedBaseSnapshotId":workspace["headSnapshotId"],
        "expectedSourceDigest":workspace["sourceDigest"], "authorIntent":intent, "targetChapters":chapters,
        "maxTaskAttempts":attempts, "maxProviderRequests":requests, "maxProviderTokens":4_000_000, "deadlineAt":deadline
    })).await
}

fn acceptance(chapter: i64) -> Value {
    json!({"requirements":[{"id":format!("port-memory-{chapter}"),"text":if chapter == 4 {FOURTH_INTENT} else {ORIGINAL}}],
        "length":{"requirement":"原作者没有指定单章篇幅","minCharacters":null,"maxCharacters":null}})
}

async fn write(
    runtime: &Runtime,
    root: &str,
    uri: &str,
    body: &str,
    chapter: i64,
    note: Option<&str>,
    repair_report: Option<(&Value, &Value)>,
) -> (Value, Value) {
    let repair = repair_report.is_some();
    let coordinator = runtime.next_task(root, uri, "coordinator").await;
    assert_eq!(coordinator["operation"], "advisory");
    let input = read_inputs(runtime, root, &coordinator).await;
    let run = runtime.get(uri).await;
    assert_author_source(&input["/authorRequirementsSource"], &coordinator, &run);
    let status = decision(runtime, root, &coordinator).await;
    let operation = if run["currentChapterId"].is_string() {
        "generateChapter"
    } else {
        "writeNextChapter"
    };
    if repair {
        let current = status["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|requirement| requirement["kind"] == "revisionOutcome")
            .unwrap();
        assert_eq!(current["assessment"]["verdict"], "notMet");
        assert_eq!(current["status"], "pending");
        assert!(
            !status["readyActions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|action| action["operation"] == "finishChapter")
        );
        assert_eq!(status["chapterAcceptance"], acceptance(chapter));
    }
    let readable_report = if let Some((producer, report)) = repair_report {
        let current = status["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|requirement| requirement["kind"] == "revisionOutcome")
            .unwrap();
        assert_eq!(
            current["assessment"]["reportRevisionId"],
            report["revisionId"]
        );
        Some(read_saved_report(runtime, root, uri, &coordinator, producer, report).await)
    } else {
        None
    };
    runtime
        .choose_with_acceptance(root, &coordinator, operation, Some(acceptance(chapter)))
        .await;
    runtime.complete_and_advance(root, uri, &coordinator).await;
    let writer = runtime
        .next_task(
            root,
            uri,
            &format!(
                "writer-{chapter}-{}",
                if repair { "repair" } else { "initial" }
            ),
        )
        .await;
    assert_eq!(writer["operation"], "generateChapter");
    assert_eq!(writer["target"]["chapterNumber"], chapter);
    let inputs = read_inputs(runtime, root, &writer).await;
    let current = runtime.get(uri).await;
    assert_author_source(&inputs["/authorRequirementsSource"], &writer, &current);
    assert_eq!(
        writer["inputManifest"]["authorIntent"],
        support::author_intent(&current)
    );
    if let Some(note) = note {
        assert!(
            writer["inputManifest"]["authorIntent"]
                .as_str()
                .unwrap()
                .contains(note)
        );
    } else if chapter == 1 {
        assert_eq!(
            writer["inputManifest"]["authorIntent"],
            format!("[要求版本 0] {ORIGINAL}")
        );
    }
    if let Some(item) = readable_report {
        let (producer, _) = repair_report.unwrap();
        assert_same_work_attempts(runtime, root, uri, &coordinator, producer, &writer).await;
        let before = runtime.get(uri).await;
        let writer_uri = format!("{root}/tasks/{}", writer["id"].as_str().unwrap());
        let writer_before = runtime.get(&writer_uri).await;
        assert_eq!(writer_before["status"], "queued");
        assert_eq!(writer_before, writer);
        assert_readable_writer_draft(runtime, root, &writer).await;
        assert!(workspace_reports(runtime, root, &writer).await.is_empty());
        for route in ["task-source", "task-source-content"] {
            let endpoint = format!(
                "{root}/tasks/{}/{route}?threadId={}&expectedRevision={}&sourceId={}",
                writer["id"].as_str().unwrap(),
                writer["threadId"].as_str().unwrap(),
                writer["inputRevision"],
                item["sourceId"].as_str().unwrap()
            );
            let (status, response) = runtime.call("GET", &endpoint, CANDIDATE, None).await;
            assert_eq!(status, axum::http::StatusCode::CONFLICT, "{response}");
            assert_eq!(
                response,
                json!({"error":{
                    "code":"task_source_not_in_catalog",
                    "message":"该 sourceId 不在本轮来源目录中。请先调用 read_context 获取本轮来源目录；不要复用其他任务的 sourceId 或 cursor。",
                    "details":{"taskId":writer["id"],"retryable":true,"sourceIsolation":"currentTaskOnly"},
                    "recovery":{"tool":"read_context","arguments":{
                        "projectId":writer["projectId"],"taskId":writer["id"],
                        "threadId":writer["threadId"],"expectedRevision":writer["inputRevision"]
                    },"discardPreviousSourceIdAndCursor":true}
                }})
            );
        }
        assert_eq!(runtime.get(uri).await, before);
        let writer_after = runtime.get(&writer_uri).await;
        assert_eq!(writer_after["status"], "queued");
        assert_eq!(writer_after, writer_before);
    }
    let saved = runtime
        .save_writer(
            root,
            &writer,
            body,
            &format!("save-{}", writer["id"].as_str().unwrap()),
        )
        .await;
    assert_saved(runtime, &writer, &saved, body);
    runtime.complete_and_advance(root, uri, &writer).await;
    (writer, saved)
}

async fn assess(
    runtime: &Runtime,
    root: &str,
    uri: &str,
    status: &str,
    reason: &str,
    prior: Option<i64>,
) -> Value {
    let critic = runtime
        .assess_chapter_options(root, uri, status, reason, &[], prior, true, false)
        .await;
    let input = read_inputs(runtime, root, &critic).await;
    let content: String = Connection::open(&runtime.path).unwrap().query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE revision_id=?1 AND object_kind='draft'",
        [critic["target"]["draftRevisionId"].as_str().unwrap()], |row| row.get(0)
    ).unwrap();
    let draft: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(
        input["/operationContext/revisionOutcomeInput"]["afterScenes"][0]["body"],
        draft["body"]
    );
    runtime.complete_and_advance(root, uri, &critic).await;
    critic
}

async fn report(runtime: &Runtime, root: &str, writer: &Value, expected: &str) -> Value {
    let report = runtime
        .get(&format!(
            "{root}/revision-outcomes/{}",
            writer["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(report["authority"], "candidateAnalysis");
    assert_eq!(report["state"], "current");
    assert_eq!(report["assessment"]["writerTaskId"], writer["id"]);
    assert_eq!(
        report["assessment"]["chapterId"],
        writer["target"]["chapterId"]
    );
    let db = Connection::open(&runtime.path).unwrap();
    let draft_revision: String = db.query_row("SELECT revision_id FROM story_task_outputs_v2 WHERE task_id=?1 AND output_kind='draft' AND completeness='chapterComplete' ORDER BY ordinal DESC LIMIT 1", [writer["id"].as_str().unwrap()], |row| row.get(0)).unwrap();
    assert_eq!(report["assessment"]["draftRevisionId"], draft_revision);
    assert_eq!(report["assessment"]["knowledgeBatchId"], Value::Null);
    let outcomes = report["assessment"]["outcomes"].as_array().unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0]["id"],
        acceptance(writer["target"]["chapterNumber"].as_i64().unwrap())["requirements"][0]["id"]
    );
    assert_eq!(outcomes[0]["status"], expected);
    assert!(!outcomes[0]["evidence"].as_array().unwrap().is_empty());
    assert_eq!(report["decisions"], json!([]));
    report
}

async fn finish(
    runtime: &Runtime,
    root: &str,
    uri: &str,
    writer: &Value,
    report: &Value,
    exhaust: bool,
) -> Value {
    let coordinator = runtime.next_task(root, uri, "coordinator").await;
    read_inputs(runtime, root, &coordinator).await;
    let status = decision(runtime, root, &coordinator).await;
    let current = status["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|requirement| requirement["kind"] == "revisionOutcome")
        .unwrap();
    assert_eq!(current["assessment"]["writerTaskId"], writer["id"]);
    assert_eq!(current["assessment"]["verdict"], "passed");
    assert_eq!(current["status"], "satisfied");
    // The model reads the full actual report, not only the decision's compact summary.
    let view = runtime
        .ok(
            "GET",
            &format!(
                "{root}/tasks/{}/revision-outcome?threadId={}&expectedRevision={}&writerTaskId={}",
                coordinator["id"].as_str().unwrap(),
                coordinator["threadId"].as_str().unwrap(),
                coordinator["inputRevision"],
                writer["id"].as_str().unwrap()
            ),
            CANDIDATE,
            None,
        )
        .await;
    assert_eq!(view["state"], "current");
    let source = view["taskSources"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["sectionPath"] == "/operationContext/revisionOutcomeReport")
        .unwrap();
    let full = read_source(runtime, root, &coordinator, source).await;
    assert!(contains_text(
        &full,
        report["assessment"]["draftRevisionId"].as_str().unwrap()
    ));
    assert!(contains_text(&full, "supported"));
    runtime.choose(root, &coordinator, "finishChapter").await;
    if exhaust {
        complete_with_remaining_requests(runtime, root, uri, &coordinator).await
    } else {
        runtime.complete_and_advance(root, uri, &coordinator).await
    }
}

// Every page belongs to the same actual directory revision. We use the issued
// action key and opaque cursor, never select by a keyword in the author's text.
async fn decision(runtime: &Runtime, root: &str, task: &Value) -> Value {
    let endpoint = format!(
        "{root}/tasks/{}/work-decision?threadId={}&expectedRevision={}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"]
    );
    let mut first = runtime.ok("GET", &endpoint, CANDIDATE, None).await;
    let mut cursor = first["pagination"]["nextCursor"].clone();
    let mut cursors = HashSet::new();
    assert!(first.to_string().len() <= 6500);
    while let Some(next) = cursor.as_str() {
        assert!(cursors.insert(next.to_owned()));
        let page = runtime
            .ok(
                "GET",
                &format!("{endpoint}&cursor={}", next.replace(':', "%3A")),
                CANDIDATE,
                None,
            )
            .await;
        assert!(page.to_string().len() <= 6500);
        for field in ["version", "sourceDigest"] {
            assert_eq!(page[field], first[field]);
        }
        for field in ["requirements", "readyActions"] {
            first[field]
                .as_array_mut()
                .unwrap()
                .extend(page[field].as_array().unwrap().iter().cloned());
        }
        first["receipts"]["items"].as_array_mut().unwrap().extend(
            page["receipts"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .cloned(),
        );
        cursor = page["pagination"]["nextCursor"].clone();
    }
    let mut keys = HashSet::new();
    for action in first["readyActions"].as_array().unwrap() {
        assert!(keys.insert(action["actionKey"].as_str().unwrap()));
    }
    assert_eq!(
        first["coverage"]["availableActions"],
        first["readyActions"].as_array().unwrap().len()
    );
    assert_eq!(
        first["coverage"]["totalRequirements"],
        first["requirements"].as_array().unwrap().len()
    );
    first
}

async fn read_inputs(runtime: &Runtime, root: &str, task: &Value) -> Value {
    let pack = read_context(runtime, root, task).await;
    let sources = pack["taskSources"]["entries"].as_array().unwrap();
    assert!(
        sources
            .iter()
            .any(|source| source["sectionPath"] == "/authorRequirementsSource")
    );
    let mut full = json!({});
    for source in sources
        .iter()
        .filter(|source| source["sectionPath"] != "/taskWorkspace")
    {
        full[source["sectionPath"].as_str().unwrap()] =
            read_source(runtime, root, task, source).await;
    }
    full
}

async fn read_context(runtime: &Runtime, root: &str, task: &Value) -> Value {
    let mut endpoint = format!(
        "{root}/context?taskId={}&threadId={}&projectRevision={}&purpose={}&byteBudget=60000",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"],
        task["purpose"].as_str().unwrap()
    );
    for field in [
        "readingChapter",
        "sceneOrder",
        "povCharacterId",
        "storyTime",
    ] {
        if let Some(value) = task["target"].get(field).filter(|value| !value.is_null()) {
            let rendered = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            endpoint.push_str(&format!("&{field}={rendered}"));
        }
    }
    runtime.ok("GET", &endpoint, CANDIDATE, None).await
}

async fn workspace_reports(runtime: &Runtime, root: &str, task: &Value) -> Vec<Value> {
    workspace_items(runtime, root, task, "revisionOutcome").await
}

async fn workspace_items(runtime: &Runtime, root: &str, task: &Value, kind: &str) -> Vec<Value> {
    let pack = read_context(runtime, root, task).await;
    assert_eq!(pack["taskWorkspace"]["textDelivered"], false);
    let source = pack["taskWorkspace"]["sourceId"].as_str().unwrap();
    assert!(
        pack["taskSources"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["sourceId"] == source && entry["sectionPath"] == "/taskWorkspace")
    );
    let endpoint = format!(
        "{root}/tasks/{}/task-source-content?threadId={}&expectedRevision={}&sourceId={source}&view=workspace&objectKind={kind}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"]
    );
    let mut items = Vec::new();
    let mut suffix = String::new();
    let mut cursors = HashSet::new();
    loop {
        let page = runtime
            .ok("GET", &format!("{endpoint}{suffix}"), CANDIDATE, None)
            .await;
        assert!(page.to_string().len() <= 6500);
        assert_eq!(page["sourceId"], source);
        assert_eq!(page["representation"], "workspaceDirectory");
        assert_eq!(page["contentRead"], false);
        assert_eq!(page["coverage"]["contentRead"], false);
        items.extend(page["items"].as_array().unwrap().iter().cloned());
        let Some(cursor) = page["nextCursor"].as_str() else {
            assert_eq!(page["coverage"]["searchScopeCompleteFromCursor"], true);
            break;
        };
        assert_eq!(page["coverage"]["searchScopeCompleteFromCursor"], false);
        assert!(cursors.insert(cursor.to_owned()));
        suffix = format!("&cursor={}", cursor.replace(':', "%3A"));
    }
    items
}

async fn assert_readable_writer_draft(runtime: &Runtime, root: &str, writer: &Value) {
    let items = workspace_items(runtime, root, writer, "draft").await;
    let object = format!("draft-{}", writer["target"]["chapterId"].as_str().unwrap());
    let revision = &writer["inputManifest"]["expectedCandidateRevisionId"];
    let item = items
        .iter()
        .find(|item| {
            item["objectId"] == object
                && item["revisionId"] == *revision
                && item["purpose"] == "writerInput"
        })
        .unwrap_or_else(|| panic!("current frozen writer draft is missing: {items:?}"));
    assert_eq!(item["authority"], "frozenWriterInput");
    assert_eq!(item["contentRead"], false);
    let first = runtime
        .ok(
            "GET",
            &format!(
                "{root}/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}",
                writer["id"].as_str().unwrap(),
                writer["threadId"].as_str().unwrap(),
                writer["inputRevision"],
                item["sourceId"].as_str().unwrap()
            ),
            CANDIDATE,
            None,
        )
        .await;
    assert_eq!(first["sourceId"], item["sourceId"]);
    assert_eq!(first["byteRange"]["start"], 0);
    let path = format!("/workspace/{object}/writerInput");
    assert_eq!(first["sectionPath"], path);
    let source = json!({"sourceId":item["sourceId"],"contentHash":first["contentHash"],
        "payloadBytes":first["byteRange"]["total"],"sectionPath":path});
    let full = read_source(runtime, root, writer, &source).await;
    let encoded: String = Connection::open(&runtime.path).unwrap().query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE revision_id=?1 AND object_id=?2 AND object_kind='draft'",
        params![revision.as_str().unwrap(), object], |row| row.get(0)
    ).unwrap();
    let expected: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(expected["body"], FIRST_UNMET);
    assert_eq!(
        full,
        json!({"objectId":object,"sourceRevisionId":revision,
        "purpose":"writerInput","authority":"frozenWriterInput","workingSelectionRevision":null,
        "content":expected})
    );
}

async fn read_saved_report(
    runtime: &Runtime,
    root: &str,
    uri: &str,
    reader: &Value,
    producer: &Value,
    report: &Value,
) -> Value {
    let items = workspace_reports(runtime, root, reader).await;
    let item = items
        .iter()
        .find(|item| {
            item["producerTaskId"] == producer["id"] && item["revisionId"] == report["revisionId"]
        })
        .unwrap_or_else(|| panic!("actual saved independent report is missing: {items:?}"));
    let object = format!(
        "revision-outcome-{}",
        producer["target"]["writerTaskId"].as_str().unwrap()
    );
    assert_eq!(item["objectId"], object);
    assert_eq!(item["outputKind"], "revisionOutcome");
    assert_eq!(item["outputOrdinal"], 1);
    assert_eq!(item["purpose"], "savedReport");
    assert_eq!(item["authority"], "nativeSavedCheckpoint");
    assert_eq!(item["producerStatusAtListing"], "completed");
    assert_eq!(item["contentRead"], false);
    assert_eq!(item["contentHash"], Value::Null);
    assert_eq!(
        item["version"],
        json!({"revisionId":report["revisionId"],"outputOrdinal":1})
    );

    let first = runtime
        .ok(
            "GET",
            &format!(
                "{root}/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}",
                reader["id"].as_str().unwrap(),
                reader["threadId"].as_str().unwrap(),
                reader["inputRevision"],
                item["sourceId"].as_str().unwrap()
            ),
            CANDIDATE,
            None,
        )
        .await;
    assert_eq!(first["sourceId"], item["sourceId"]);
    assert_eq!(first["byteRange"]["start"], 0);
    let path = format!(
        "/workspace/{object}/savedReport/{}/1",
        producer["id"].as_str().unwrap()
    );
    assert_eq!(first["sectionPath"], path);
    let source = json!({"sourceId":item["sourceId"],"contentHash":first["contentHash"],
        "payloadBytes":first["byteRange"]["total"],"sectionPath":path});
    let full = read_source(runtime, root, reader, &source).await;
    let encoded: String = Connection::open(&runtime.path).unwrap().query_row(
        "SELECT content_json FROM story_revisions_v2 WHERE revision_id=?1 AND object_id=?2 AND object_kind='revisionOutcome'",
        params![report["revisionId"].as_str().unwrap(), object], |row| row.get(0)
    ).unwrap();
    let expected: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(expected, report["assessment"]);
    assert_eq!(
        full,
        json!({"objectId":object,"sourceRevisionId":report["revisionId"],
        "purpose":"savedReport","authority":"nativeSavedCheckpoint","workingSelectionRevision":null,
        "content":expected,"producerTaskId":producer["id"],"outputOrdinal":1,
        "evidenceUse":"immutableSavedVersionForReadingAndComparison;notAcceptanceOrDelivery"})
    );
    assert_eq!(full["content"]["outcomes"][0]["status"], "unmet");

    let current = runtime
        .get(&format!(
            "{root}/tasks/{}",
            producer["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(current["status"], "completed");
    assert_eq!(current["operation"], "assessRevisionOutcome");
    assert!(
        current["outputRefs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|output| {
                output["kind"] == "revisionOutcome"
                    && output["objectId"] == object
                    && output["revisionId"] == report["revisionId"]
            })
    );
    let run = runtime.get(uri).await;
    let charges = run["providerBudget"]["charges"].as_array().unwrap();
    let charge = charges
        .iter()
        .find(|charge| charge["taskId"] == producer["id"])
        .unwrap();
    assert_eq!(charge["status"], "terminal");
    assert_eq!(
        charge["budgetId"],
        format!("budget-{}", producer["id"].as_str().unwrap())
    );
    assert_eq!(charge["requests"], 1);
    assert_eq!(charge["tokens"], 7);
    assert_eq!(charge["unknownTokens"], 0);
    let settled: (String, i64) = Connection::open(&runtime.path)
        .unwrap()
        .query_row(
            "SELECT work_task_id,settled FROM story_task_provider_budgets_v1 WHERE task_id=?1",
            [producer["id"].as_str().unwrap()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        settled,
        (format!("work-{}", run["id"].as_str().unwrap()), 1)
    );
    item.clone()
}

async fn assert_same_work_attempts(
    runtime: &Runtime,
    root: &str,
    uri: &str,
    reader: &Value,
    producer: &Value,
    writer: &Value,
) {
    let run = runtime.get(uri).await;
    let work_id = format!("work-{}", run["id"].as_str().unwrap());
    let mut sequences = Vec::new();
    for task in [producer, reader, writer] {
        let work = runtime
            .get(&format!(
                "{root}/tasks/{}/work-task",
                task["id"].as_str().unwrap()
            ))
            .await;
        assert_eq!(work["id"], work_id);
        assert_eq!(work["kind"], "generation");
        assert_eq!(work["status"], "running");
        assert_eq!(work["threadId"], reader["threadId"]);
        assert_eq!(work["goal"], ORIGINAL);
        let current_steps = work["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|step| step["currentAttemptId"] == writer["id"])
            .collect::<Vec<_>>();
        assert_eq!(current_steps.len(), 1);
        let current_step = current_steps[0];
        assert_eq!(current_step["status"], "running");
        assert_eq!(current_step["operation"], writer["operation"]);
        assert_eq!(current_step["inputRevision"], writer["inputRevision"]);
        let mut bound_target = writer["target"].clone();
        assert_eq!(
            bound_target.as_object_mut().unwrap().remove("resumeStepId"),
            Some(current_step["id"].clone())
        );
        assert_eq!(current_step["target"], bound_target);
        let current_attempts = current_step["attempts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|attempt| attempt["id"] == writer["id"])
            .collect::<Vec<_>>();
        assert_eq!(current_attempts.len(), 1);
        assert_eq!(current_attempts[0]["status"], "queued");
        assert_eq!(current_attempts[0]["threadId"], writer["threadId"]);
        let db = Connection::open(&runtime.path).unwrap();
        let actual: (i64,String,String,String) = db.query_row(
            "SELECT a.sequence,s.work_task_id,w.thread_id,(SELECT t.thread_id FROM story_work_attempts_v1 first JOIN story_work_steps_v1 fs ON fs.id=first.step_id JOIN story_tasks_v2 t ON t.task_id=first.attempt_id WHERE fs.work_task_id=w.id ORDER BY first.sequence LIMIT 1) FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE a.attempt_id=?1",
            [task["id"].as_str().unwrap()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        ).unwrap();
        assert_eq!(actual.1, work_id);
        assert_eq!(actual.2, reader["threadId"].as_str().unwrap());
        assert_eq!(actual.3, actual.2);
        sequences.push(actual.0);
    }
    assert!(sequences[0] < sequences[1] && sequences[1] < sequences[2]);
    assert_eq!(writer["operation"], "generateChapter");
    assert_ne!(writer["threadId"], producer["threadId"]);
    assert_ne!(writer["threadId"], reader["threadId"]);
}

async fn read_source(runtime: &Runtime, root: &str, task: &Value, source: &Value) -> Value {
    let endpoint = format!(
        "{root}/tasks/{}/task-source?threadId={}&expectedRevision={}&sourceId={}",
        task["id"].as_str().unwrap(),
        task["threadId"].as_str().unwrap(),
        task["inputRevision"],
        source["sourceId"].as_str().unwrap()
    );
    let mut suffix = String::new();
    let mut raw = String::new();
    let mut cursors = HashSet::new();
    loop {
        let page = runtime
            .ok("GET", &format!("{endpoint}{suffix}"), CANDIDATE, None)
            .await;
        assert!(page.to_string().len() <= 6500);
        assert_eq!(page["sourceId"], source["sourceId"]);
        assert_eq!(page["contentHash"], source["contentHash"]);
        assert_eq!(page["sectionPath"], source["sectionPath"]);
        assert_eq!(page["byteRange"]["start"], raw.len());
        let part = page["payloadPart"].as_str().unwrap();
        assert!(part.len() <= 2000);
        raw.push_str(part);
        assert_eq!(page["byteRange"]["end"], raw.len());
        assert_eq!(page["byteRange"]["total"], source["payloadBytes"]);
        let Some(cursor) = page["nextCursor"].as_str() else {
            assert_eq!(page["endOfInputFromCursor"], true);
            assert_eq!(source["payloadBytes"], raw.len());
            break;
        };
        assert!(cursors.insert(cursor.to_owned()));
        suffix = format!("&cursor={}", cursor.replace(':', "%3A"));
    }
    assert_eq!(
        source["contentHash"],
        format!("{:x}", Sha256::digest(raw.as_bytes()))
    );
    serde_json::from_str(&raw).unwrap()
}

fn assert_author_source(source: &Value, task: &Value, run: &Value) {
    let expected = support::author_intent(run);
    assert_eq!(source["authority"], "frozenAuthorIntent");
    assert_eq!(source["authorText"], expected);
    assert_eq!(task["inputManifest"]["authorIntent"], expected);
}

fn contains_text(value: &Value, expected: &str) -> bool {
    match value {
        Value::String(text) => text == expected,
        Value::Array(items) => items.iter().any(|item| contains_text(item, expected)),
        Value::Object(fields) => fields.values().any(|item| contains_text(item, expected)),
        _ => false,
    }
}

fn message_count(runtime: &Runtime, run: &Value) -> i64 {
    Connection::open(&runtime.path)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM story_author_messages_v1 WHERE generation_run_id=?1",
            [run["id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap()
}

fn assert_historical_body(runtime: &Runtime, revision: &str, expected: &str) {
    let content: String = Connection::open(&runtime.path).unwrap().query_row("SELECT content_json FROM story_revisions_v2 WHERE revision_id=?1 AND object_kind='draft'", [revision], |row| row.get(0)).unwrap();
    let body: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(body["body"], expected);
    assert_eq!(body["completeness"], "chapterComplete");
}

fn assert_saved(runtime: &Runtime, writer: &Value, saved: &Value, body: &str) {
    let revision = saved["revisionId"].as_str().unwrap();
    assert_historical_body(runtime, revision, body);
    let db = Connection::open(&runtime.path).unwrap();
    let (chapter, candidate, accepted): (i64, String, Option<String>) = db.query_row(
        "SELECT c.number,h.candidate_revision_id,h.accepted_revision_id FROM story_chapters_v2 c JOIN story_object_heads_v2 h ON h.project_id=c.project_id AND h.object_id='draft-'||c.chapter_id WHERE c.chapter_id=?1",
        [writer["target"]["chapterId"].as_str().unwrap()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    ).unwrap();
    assert_eq!(chapter, writer["target"]["chapterNumber"].as_i64().unwrap());
    assert_eq!(candidate, revision);
    assert_eq!(accepted, None);
    let linked: i64 = db.query_row("SELECT COUNT(*) FROM story_task_outputs_v2 WHERE task_id=?1 AND revision_id=?2 AND output_kind='draft' AND completeness='chapterComplete'", params![writer["id"].as_str().unwrap(), revision], |row| row.get(0)).unwrap();
    assert_eq!(linked, 1);
}

fn draft_heads(runtime: &Runtime) -> Vec<Value> {
    let db = Connection::open(&runtime.path).unwrap();
    let mut statement = db.prepare("SELECT c.chapter_id,c.number,h.candidate_revision_id,h.accepted_revision_id,r.content_json FROM story_chapters_v2 c JOIN story_object_heads_v2 h ON h.project_id=c.project_id AND h.object_id='draft-'||c.chapter_id JOIN story_revisions_v2 r ON r.revision_id=h.candidate_revision_id ORDER BY c.number").unwrap();
    statement.query_map([], |row| Ok(json!({"chapterId":row.get::<_,String>(0)?,"chapterNumber":row.get::<_,i64>(1)?,"revisionId":row.get::<_,String>(2)?,"acceptedRevisionId":row.get::<_,Option<String>>(3)?,"content":serde_json::from_str::<Value>(&row.get::<_,String>(4)?).unwrap()}))).unwrap().map(Result::unwrap).collect()
}

fn assert_no_adoption(runtime: &Runtime) {
    let db = Connection::open(&runtime.path).unwrap();
    for sql in [
        "SELECT COUNT(*) FROM story_object_heads_v2 WHERE accepted_revision_id IS NOT NULL",
        "SELECT COUNT(*) FROM story_acceptances_v2",
        "SELECT COUNT(*) FROM story_knowledge_heads_v2",
    ] {
        assert_eq!(
            db.query_row(sql, [], |row| row.get::<_, i64>(0)).unwrap(),
            0,
            "{sql}"
        );
    }
}

async fn assert_work(
    runtime: &Runtime,
    root: &str,
    work_uri: &str,
    run: &Value,
    original: &str,
) -> Value {
    let work = runtime.get(work_uri).await;
    assert_eq!(work["id"], format!("work-{}", run["id"].as_str().unwrap()));
    assert_eq!(work["kind"], "generation");
    assert_eq!(work["threadId"], "coordinator");
    assert_eq!(work["goal"], original);
    assert_eq!(work["status"], "completed");
    let steps = work["steps"].as_array().unwrap();
    let mut attempts = HashSet::new();
    for step in steps {
        assert!(
            ["advisory", "generateChapter", "assessRevisionOutcome"]
                .contains(&step["operation"].as_str().unwrap()),
            "optional methods were not selected by this model: {step}"
        );
        for attempt in step["attempts"].as_array().unwrap() {
            assert!(attempts.insert(attempt["id"].as_str().unwrap()));
            assert_eq!(attempt["status"], "completed");
            let own = runtime
                .get(&format!(
                    "{root}/tasks/{}/work-task",
                    attempt["id"].as_str().unwrap()
                ))
                .await;
            assert_eq!(own["id"], work["id"]);
            assert_eq!(own["threadId"], "coordinator");
        }
        assert!(step["changes"].as_array().unwrap().is_empty());
    }
    assert_eq!(run["usedTaskAttempts"], attempts.len());
    assert!(attempts.len() as i64 <= run["maxTaskAttempts"].as_i64().unwrap());
    let db = Connection::open(&runtime.path).unwrap();
    let charged: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM story_task_provider_budgets_v1 WHERE work_task_id=?1 AND settled=1",
            [work["id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(charged, attempts.len() as i64);
    let charges = run["providerBudget"]["charges"].as_array().unwrap();
    assert_eq!(charges.len(), attempts.len());
    let mut requests = 0;
    let mut tokens = 0;
    for charge in charges {
        assert!(attempts.contains(charge["taskId"].as_str().unwrap()));
        assert_eq!(
            charge["budgetId"],
            format!("budget-{}", charge["taskId"].as_str().unwrap())
        );
        assert_eq!(charge["status"], "terminal");
        assert_eq!(charge["unknownTokens"], 0);
        assert_eq!(charge["overrun"], false);
        requests += charge["requests"].as_u64().unwrap();
        tokens += charge["tokens"].as_u64().unwrap();
    }
    assert_eq!(run["providerBudget"]["usedRequests"], requests);
    assert_eq!(run["providerBudget"]["usedTokens"], tokens);
    assert_eq!(work["deliveryStatus"], "ready_for_review");
    let acceptance_reports = work["deliveryReport"]["chapterAcceptanceReports"]
        .as_array()
        .unwrap();
    let scope_chapters =
        run["targetChapters"].as_i64().unwrap() - run["startChapterNumber"].as_i64().unwrap() + 1;
    assert_eq!(acceptance_reports.len() as i64, scope_chapters);
    for report in acceptance_reports {
        assert_eq!(report["current"], true);
        assert_eq!(report["verdict"], "passed");
    }
    assert!(work["deliveryReport"]["deliveryReview"].is_object());
    assert_eq!(
        work["deliveryReport"]["deliveryReview"]["goalStatus"],
        "fulfilled"
    );
    assert!(
        work["deliveryReport"]["unsettledAttemptIds"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    work
}

// Preserve the original last-grant exhaustion assertion through genuine Engine
// reserve/settle receipts. No SQL ledger edits or new provider allowance.
async fn complete_with_remaining_requests(
    runtime: &Runtime,
    root: &str,
    uri: &str,
    task: &Value,
) -> Value {
    let grant = runtime
        .get(&format!(
            "{root}/tasks/{}/provider-budget",
            task["id"].as_str().unwrap()
        ))
        .await;
    let run = runtime.get(uri).await;
    let count = run["providerBudget"]["maxRequests"].as_u64().unwrap()
        - run["providerBudget"]["usedRequests"].as_u64().unwrap();
    assert_eq!(grant["maxRequests"], count);
    assert!(count > 0);
    read_inputs(runtime, root, task).await;
    let mut state = decision(runtime, root, task).await;
    assert_eq!(state["deliveryReviewRequired"], true);
    let evidence = state["receipts"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|receipt| receipt["status"] == "completed")
        .flat_map(|receipt| {
            receipt["outputs"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|output| output["revisionId"].is_string())
                .map(|output| json!({"taskId":receipt["taskId"],"revisionId":output["revisionId"]}))
        })
        .collect::<Vec<_>>();
    assert!(!evidence.is_empty());
    assert_eq!(state["workPlan"]["activeGoalId"], "fixture-chapters");
    let id = task["id"].as_str().unwrap();
    let plan = runtime.ok("GET", &format!("{root}/tasks/{id}/work-plan?threadId={}&expectedRevision={}&goalId=fixture-chapters", task["threadId"].as_str().unwrap(), task["inputRevision"]), CANDIDATE, None).await;
    let mut goal = plan["goal"].clone();
    goal["status"] = json!("satisfied");
    goal["assessment"] = json!("原三章范围当前候选和独立验收回执均已核对");
    goal["evidence"] = json!([evidence.first().unwrap()]);
    runtime.candidate(&format!("{root}/tasks/{id}/work-plan"), json!({"taskId":id,"threadId":task["threadId"],"expectedRevision":task["inputRevision"],"expectedPlanVersion":plan["version"],"idempotencyKey":"selected-chapters-final-review","plan":{"intentMode":"executeGoals","goals":[goal]}})).await;
    state = decision(runtime, root, task).await;
    runtime.candidate(&format!("{root}/tasks/{id}/complete"), json!({"threadId":task["threadId"],"summary":"原三章实际保存并通过当前独立检查","deliveryReview":{"expectedWorkVersion":state["version"],"expectedSourceDigest":state["sourceDigest"],"goalStatus":"fulfilled","goalAssessment":"三章完整候选保存，第一章实际修订后重新通过独立检查","checkedScope":"仅原三章当前正文与来源回执，不冒称选用未运行方法或真实供应商文学验收","unresolved":[],"issueResolutions":[],"authorDecisions":[],"evidence":evidence}})).await;
    let mut budget = PersistentBudget::new(&runtime.path.with_extension("budget.sqlite3"), id, json!({"options":{"id":grant["budgetId"],"maxRequests":grant["maxRequests"],"maxTokens":grant["maxTokens"],"maxDurationMs":600000,"startedAt":1000},"now":1000})).unwrap();
    for index in 0..count {
        let reservation = budget
            .apply(
                "budget.reserve",
                json!({"parameters":{"maxOutputTokens":8},"now":1001+index}),
            )
            .unwrap();
        budget
            .apply(
                "budget.settle",
                json!({"id":reservation,"usage":{"total_tokens":7}}),
            )
            .unwrap();
    }
    budget.complete("terminal").unwrap();
    let receipt =
        PersistentBudget::receipt(&runtime.path.with_extension("budget.sqlite3"), id).unwrap();
    assert_eq!(
        receipt,
        json!({"runId":id,"budgetId":grant["budgetId"],"status":"terminal","usage":{"requests":count,"tokens":(count*7) as f64,"unknownTokens":0.0,"overrun":false},"limits":{"maxRequests":grant["maxRequests"].as_f64().unwrap(),"maxTokens":grant["maxTokens"].as_f64().unwrap(),"maxDurationMs":600000.0},"startedAt":1000.0,"failureObservations":[],"usageReconciliations":[]})
    );
    let charged = runtime
        .post(&format!("{uri}/provider-usage"), json!({"taskId":id}))
        .await;
    runtime
        .post(
            &format!("{uri}/advance-task"),
            json!({"expectedVersion":charged["version"],"taskId":id}),
        )
        .await
}
