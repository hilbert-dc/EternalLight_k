use crate::*;
use rusqlite::params;
use serde_json::{Value, json};

struct Fixture {
    repo: SqliteStoryRepository,
    project: ProjectId,
    task: TaskRecord,
    work: String,
    event: String,
    next: Option<WorkStep>,
}
impl Fixture {
    fn new(path: &std::path::Path) -> Self {
        let mut repo = SqliteStoryRepository::open(path).unwrap();
        let project = ProjectId::parse("wake-project").unwrap();
        repo.create_project(
            ProjectRecord {
                project_id: project.clone(),
                title: "事件验收".into(),
                logline: String::new(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
        let digest = repo.project_source_watermarks(&project).unwrap()["digest"]
            .as_str()
            .unwrap()
            .to_owned();
        let task = repo
            .create_budgeted_root_task(
                CreateTaskRequest {
                    task_id: "ignored".into(),
                    project_id: project.clone(),
                    thread_id: "original-thread".into(),
                    operation: "advisory".into(),
                    input_revision: 0,
                    model: None,
                    target_json: json!({"type":"project"}).to_string(),
                    input_manifest_json: json!({"authorIntent":"读取结果，按实际情况继续创作"})
                        .to_string(),
                    capability_hash: "a".repeat(64),
                    capability_expires_at: i64::MAX,
                },
                &InitialWorkBudget {
                    authorization_id: "authorization-root".into(),
                    author_message_id: "message-root".into(),
                    expected_base_snapshot_id: "initial".into(),
                    expected_source_digest: digest,
                    max_requests: Some(10),
                    max_tokens: Some(1000),
                    deadline_at: None,
                },
                1,
            )
            .unwrap();
        let work = format!("work-{}", task.task_id);
        repo.complete_task(
            &project,
            &task.task_id,
            "original-thread",
            "读取完成".into(),
            None,
            2,
        )
        .unwrap();
        let stored: String = repo
            .connection
            .query_row(
                "SELECT grant_json FROM story_task_provider_budgets_v1 WHERE task_id=?1",
                [&task.task_id],
                |r| r.get(0),
            )
            .unwrap();
        let grant: TaskProviderBudget = serde_json::from_str(&stored).unwrap();
        repo.charge_work_provider_usage(&project,&work,&task.task_id,&json!({"runId":task.task_id,"budgetId":grant.budget_id,"status":"completed","limits":{"maxRequests":10,"maxTokens":1000,"maxDurationMs":null},"usage":{"requests":0,"tokens":0,"unknownTokens":0,"overrun":false}})).unwrap();
        // Current Native pending-delivery state; no fabricated completion or extra author input.
        repo.connection
            .execute(
                "INSERT INTO story_work_decisions_v1(work_task_id,origin_task_id) VALUES(?1,?2)",
                params![work, task.task_id],
            )
            .unwrap();
        repo.connection
            .execute(
                "UPDATE story_work_tasks_v1 SET status='running' WHERE id=?1",
                [&work],
            )
            .unwrap();
        let event = repo.work_execution_events(&project, None, 20).unwrap()["items"][0]["eventId"]
            .as_str()
            .unwrap()
            .to_owned();
        Self {
            repo,
            project,
            task,
            work,
            event,
            next: None,
        }
    }
    fn request(&self, session: &str) -> WorkExecutionClaimRequest {
        WorkExecutionClaimRequest {
            host_session_id: session.into(),
            expected_work_version: self
                .repo
                .work_task(&self.project, &self.work)
                .unwrap()
                .version,
            expected_project_revision: 0,
            expected_source_digest:
                self.repo.project_source_watermarks(&self.project).unwrap()["digest"]
                    .as_str()
                    .unwrap()
                    .into(),
        }
    }
    fn claim(&mut self, session: &str, now: i64) -> Value {
        let request = self.request(session);
        self.repo
            .claim_work_execution_event(&self.project, &self.event, request, now)
            .unwrap()
    }
    fn queued_successor(&mut self) {
        let request = self.request("host");
        let continuation = self
            .repo
            .prepare_work_decision(
                &self.project,
                &self.work,
                &WorkDecisionRequest {
                    expected_work_version: request.expected_work_version,
                    expected_revision: request.expected_project_revision,
                    expected_source_digest: request.expected_source_digest,
                },
            )
            .unwrap();
        self.next = Some(
            continuation
                .step
                .expect("Native decision queued in original Work"),
        );
    }
    fn attach_successor(&mut self) {
        let mut target = self.next.as_ref().unwrap().target.clone();
        target["resumeStepId"] = json!(self.next.as_ref().unwrap().id);
        self.repo
            .create_funded_task(
                CreateTaskRequest {
                    task_id: "next-attempt".into(),
                    project_id: self.project.clone(),
                    thread_id: "original-thread".into(),
                    operation: "advisory".into(),
                    input_revision: 0,
                    model: None,
                    target_json: target.to_string(),
                    input_manifest_json: "{}".into(),
                    capability_hash: "b".repeat(64),
                    capability_expires_at: i64::MAX,
                },
                50,
            )
            .unwrap();
    }
    fn ack(
        &mut self,
        claim: &Value,
        step: Option<&str>,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        self.repo.ack_work_execution_event(
            &self.project,
            &self.event,
            WorkExecutionAckRequest {
                host_session_id: claim["lease"]["hostSessionId"].as_str().unwrap().into(),
                claim_id: claim["event"]["claimId"].as_str().unwrap().into(),
                lease_id: claim["lease"]["leaseId"].as_str().unwrap().into(),
                consumer_step_id: step.map(str::to_owned),
            },
            now,
        )
    }
}

#[test]
fn event_is_atomic_unique_and_reopened_with_real_source_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.db");
    let f = Fixture::new(&path);
    let terminal = f.repo.task(&f.project, &f.task.task_id).unwrap().unwrap();
    let tx = f.repo.connection.unchecked_transaction().unwrap();
    crate::work_execution_events::record_terminal(&tx, &terminal).unwrap();
    tx.commit().unwrap();
    let mut repo = SqliteStoryRepository::open(&path).unwrap();
    let page = repo.work_execution_events(&f.project, None, 20).unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["items"][0]["sourceAttemptId"], f.task.task_id);
    assert_eq!(page["items"][0]["rootThreadId"], "original-thread");
    assert!(!page.to_string().contains("读取结果"));
    assert!(page["items"][0]["createdAt"].as_i64().unwrap() > 0);
    let claim = repo
        .claim_work_execution_event(&f.project, &f.event, f.request("session-a"), 50)
        .unwrap();
    assert_eq!(claim["admission"], "claimed");
    let reopened = SqliteStoryRepository::open(&path)
        .unwrap()
        .work_execution_events(&f.project, None, 20)
        .unwrap();
    assert_eq!(reopened["items"][0]["claimId"], claim["event"]["claimId"]);
    // Transaction rollback cannot publish a notification for a result it did not commit.
    let count: i64 = f
        .repo
        .connection
        .query_row(
            "SELECT COUNT(*) FROM story_work_execution_events_v1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    {
        let tx = f.repo.connection.unchecked_transaction().unwrap();
        tx.execute("DELETE FROM story_work_execution_events_v1", [])
            .unwrap();
        crate::work_execution_events::record_terminal(&tx, &terminal).unwrap();
    }
    assert_eq!(
        f.repo
            .connection
            .query_row(
                "SELECT COUNT(*) FROM story_work_execution_events_v1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        count
    );
}
#[test]
fn two_connections_exact_claim_replay_and_expiry_fence_stale_consumers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("owners.db");
    let mut f = Fixture::new(&path);
    let first = f.claim("session-a", 50);
    let replay = f.claim("session-a", 51);
    assert_eq!(first["event"]["claimId"], replay["event"]["claimId"]);
    assert_eq!(first["lease"]["leaseId"], replay["lease"]["leaseId"]);
    let mut other = SqliteStoryRepository::open(&path).unwrap();
    assert_eq!(
        other
            .claim_work_execution_event(&f.project, &f.event, f.request("session-b"), 52)
            .unwrap()["blockedReason"],
        "activeOwner"
    );
    let next = other
        .claim_work_execution_event(&f.project, &f.event, f.request("session-b"), 82)
        .unwrap();
    assert_ne!(first["event"]["claimId"], next["event"]["claimId"]);
    assert!(f.ack(&first, None, 83).is_err());
    assert!(f.repo.release_work_execution_event(&f.project,&f.event,serde_json::from_value(json!({"hostSessionId":"session-a","claimId":first["event"]["claimId"],"leaseId":first["lease"]["leaseId"]})).unwrap()).is_err());
    assert_eq!(
        f.repo
            .release_work_execution_owner(
                &f.project,
                &f.work,
                serde_json::from_value(
                    json!({"hostSessionId":"session-a","leaseId":first["lease"]["leaseId"]})
                )
                .unwrap()
            )
            .unwrap()["released"],
        false
    );
    assert_eq!(
        f.repo.work_execution_events(&f.project, None, 20).unwrap()["items"][0]["claimId"],
        next["event"]["claimId"]
    );
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let contenders: Vec<_> = ["race-a", "race-b"]
        .into_iter()
        .map(|session| {
            let path = path.clone();
            let project = f.project.clone();
            let event = f.event.clone();
            let request = f.request(session);
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut repo = SqliteStoryRepository::open(&path).unwrap();
                barrier.wait();
                repo.claim_work_execution_event(&project, &event, request, 150)
                    .unwrap()
            })
        })
        .collect();
    barrier.wait();
    let outcomes: Vec<_> = contenders
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| result["admission"] == "claimed")
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| result["blockedReason"] == "activeOwner")
            .count(),
        1
    );
}
#[test]
fn active_flow_ownership_does_not_require_idle_attempt_or_usage() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(&dir.path().join("active.db"));
    f.queued_successor();
    f.attach_successor();
    let version = f.request("a").expected_work_version;
    let lease = f
        .repo
        .acquire_work_execution_owner(
            &f.project,
            &f.work,
            WorkExecutionOwnerRequest {
                host_session_id: "active".into(),
                expected_work_version: version,
            },
            50,
        )
        .unwrap();
    assert_eq!(lease["expiresAt"], 80);
    assert_eq!(f.claim("idle", 90)["blockedReason"], "activeAttempt");
    let renewed = f
        .repo
        .renew_work_execution_owner(
            &f.project,
            &f.work,
            serde_json::from_value(json!({"hostSessionId":"active","leaseId":lease["leaseId"]}))
                .unwrap(),
            60,
        )
        .unwrap();
    assert_eq!(renewed["expiresAt"], 90);
    assert!(
        f.repo
            .renew_work_execution_owner(
                &f.project,
                &f.work,
                serde_json::from_value(json!({"hostSessionId":"other","leaseId":lease["leaseId"]}))
                    .unwrap(),
                61
            )
            .is_err()
    );
}
#[test]
fn ack_requires_actual_attached_successor_not_queued_plan_or_false_final() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(&dir.path().join("ack.db"));
    let claim = f.claim("host", 50);
    assert!(f.ack(&claim, None, 51).is_err());
    f.queued_successor();
    let next = f.next.as_ref().unwrap().id.clone();
    assert!(f.ack(&claim, Some(&next), 51).is_err());
    f.attach_successor();
    assert!(f.ack(&claim, Some("step-nonexistent"), 51).is_err());
    assert_eq!(f.ack(&claim, Some(&next), 51).unwrap()["state"], "consumed");
    assert_eq!(f.ack(&claim, Some(&next), 52).unwrap()["state"], "consumed");
    assert!(
        f.repo.work_execution_events(&f.project, None, 20).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn actual_successor_after_crash_settles_old_event_without_another_decision() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(&dir.path().join("successor.db"));
    f.queued_successor();
    f.attach_successor();
    f.repo
        .update_task(
            &f.project,
            "next-attempt",
            "original-thread",
            TaskUpdate {
                status: "failed".into(),
                error: Some("provider_output_limit".into()),
                result_summary: None,
                now: 60,
            },
        )
        .unwrap();
    f.repo.charge_work_provider_usage(&f.project,&f.work,"next-attempt",&json!({"runId":"next-attempt","budgetId":"budget-next-attempt","status":"failed","limits":{"maxRequests":10,"maxTokens":1000,"maxDurationMs":null},"usage":{"requests":0,"tokens":0,"unknownTokens":0,"overrun":false}})).unwrap();
    let claim = f.claim("new-host", 90);
    assert_eq!(claim["admission"], "settled");
    assert_eq!(claim["event"]["state"], "consumed");
    assert!(claim["lease"].is_null());
    assert_eq!(f.repo.tasks(&f.project).unwrap().len(), 2);
}
#[test]
fn pause_cancel_intent_and_current_source_cas_block_without_changing_permission() {
    for state in ["paused", "cancelled"] {
        let dir = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(&dir.path().join("stop.db"));
        f.repo
            .connection
            .execute(
                "UPDATE story_work_tasks_v1 SET status=?1 WHERE id=?2",
                params![state, f.work],
            )
            .unwrap();
        assert_eq!(f.claim("host", 50)["blockedReason"], "authorStopped");
    }
    let dir = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(&dir.path().join("intent.db"));
    let mut request = f.request("host");
    request.expected_source_digest = "stale".into();
    assert!(
        f.repo
            .claim_work_execution_event(&f.project, &f.event, request, 50)
            .is_err()
    );
    let mut request = f.request("host");
    request.expected_work_version += 1;
    assert!(
        f.repo
            .claim_work_execution_event(&f.project, &f.event, request, 50)
            .is_err()
    );
    f.repo.connection.execute("UPDATE story_author_messages_v1 SET author_text='新的作者决定' WHERE message_id='message-root'",[]).unwrap();
    assert_eq!(f.claim("host", 50)["blockedReason"], "authorIntentChanged");
    assert_eq!(
        f.repo.work_execution_events(&f.project, None, 20).unwrap()["items"][0]["state"],
        "pending"
    );
}
#[test]
fn unknown_write_unsettled_cost_unknown_usage_and_exhaustion_do_not_restart() {
    for reason in [
        "unknownWrite",
        "unsettledUsage",
        "unknownUsage",
        "budgetExhausted",
        "budgetExpired",
        "budgetAuthorizationMissing",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(&dir.path().join("guards.db"));
        match reason {
            "unknownWrite" => {
                f.repo.connection.execute("INSERT INTO story_tool_operations_v1(operation_id,project_id,task_id,thread_id,turn_id,call_id,tool_name,request_hash,state,created_at,updated_at) VALUES('op',?1,?2,'original-thread','turn','call','save_candidate','hash','accepted',1,1)",params![f.project.as_str(),f.task.task_id]).unwrap();
            }
            "unsettledUsage" => {
                f.repo
                    .connection
                    .execute(
                        "UPDATE story_task_provider_budgets_v1 SET settled=0 WHERE task_id=?1",
                        [&f.task.task_id],
                    )
                    .unwrap();
            }
            "unknownUsage" | "budgetExhausted" => {
                let mut ledger =
                    crate::work_budget_storage::load(&f.repo.connection, &f.project, &f.work)
                        .unwrap();
                ledger.charge_receipt("cost-evidence",&json!({"runId":"cost-evidence","budgetId":"cost-budget","status":"completed","limits":{"maxRequests":10,"maxTokens":1000,"maxDurationMs":null},"usage":{"requests":if reason=="budgetExhausted"{10}else{1},"tokens":0,"unknownTokens":if reason=="unknownUsage"{100}else{0},"overrun":false}})).unwrap();
                f.repo
                    .connection
                    .execute(
                        "UPDATE story_work_budgets_v1 SET ledger_json=?2 WHERE work_task_id=?1",
                        params![f.work, serde_json::to_string(&ledger).unwrap()],
                    )
                    .unwrap();
            }
            "budgetExpired" => {
                f.repo.connection.execute("UPDATE story_work_budget_authorizations_v1 SET authorization_json=json_set(authorization_json,'$.deadlineAt',49) WHERE work_task_id=?1",[&f.work]).unwrap();
            }
            _ => {
                f.repo
                    .connection
                    .execute(
                        "DELETE FROM story_work_budget_authorizations_v1 WHERE work_task_id=?1",
                        [&f.work],
                    )
                    .unwrap();
            }
        }
        assert_eq!(f.claim("host", 50)["blockedReason"], reason, "{reason}");
        assert!(
            f.repo
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM story_work_execution_owners_v1",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap()
                == 0
        );
    }
}
#[test]
fn native_final_closure_only_and_pending_handoff_cannot_be_discarded() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(&dir.path().join("final.db"));
    let claim = f.claim("host", 50);
    f.repo
        .connection
        .execute(
            "UPDATE story_work_tasks_v1 SET status='completed' WHERE id=?1",
            [&f.work],
        )
        .unwrap();
    f.repo
        .connection
        .execute(
            "UPDATE story_work_decisions_v1 SET finished=1 WHERE work_task_id=?1",
            [&f.work],
        )
        .unwrap();
    f.repo.connection.execute("INSERT INTO story_generation_handoffs_v1(proposal_id,source_task_id,idempotency_key,content_json) VALUES('proposal',?1,'handoff','{}')",[&f.task.task_id]).unwrap();
    assert!(f.ack(&claim, None, 51).is_err());
    f.repo.connection.execute("UPDATE story_generation_handoffs_v1 SET consumed_run_id='existing-run' WHERE proposal_id='proposal'",[]).unwrap();
    assert_eq!(f.ack(&claim, None, 51).unwrap()["state"], "consumed");
}
#[test]
fn bounded_project_cursor_survives_consumption_and_identity_mismatch_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(&dir.path().join("page.db"));
    // Independent real roots create their own terminal events; no history backfill.
    for number in 0..22 {
        let task = f
            .repo
            .create_task(CreateTaskRequest {
                task_id: format!("root-{number}"),
                project_id: f.project.clone(),
                thread_id: format!("thread-{number}"),
                operation: "advisory".into(),
                input_revision: 0,
                model: None,
                target_json: "{}".into(),
                input_manifest_json: json!({"authorIntent":"只读讨论"}).to_string(),
                capability_hash: "a".repeat(64),
                capability_expires_at: 1000,
            })
            .unwrap();
        f.repo
            .complete_task(
                &f.project,
                &task.task_id,
                &task.thread_id,
                "讨论结束".into(),
                None,
                2,
            )
            .unwrap();
    }
    let page = f.repo.work_execution_events(&f.project, None, 20).unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 20);
    let cursor = page["nextCursor"].as_str().unwrap().to_owned();
    let claim = f.claim("host", 50);
    f.queued_successor();
    f.attach_successor();
    let successor = f.next.as_ref().unwrap().id.clone();
    assert_eq!(
        f.ack(&claim, Some(&successor), 51).unwrap()["state"],
        "consumed"
    );
    assert_eq!(
        f.repo
            .work_execution_events(&f.project, Some(&cursor), 20)
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(
        f.repo
            .work_execution_events(&ProjectId::parse("foreign").unwrap(), Some(&cursor), 20)
            .is_err()
    );
    assert!(f.repo.work_execution_events(&f.project, None, 21).is_err());
    assert!(
        f.repo
            .claim_work_execution_event(
                &ProjectId::parse("foreign").unwrap(),
                &f.event,
                f.request("host"),
                50
            )
            .is_err()
    );
    let mut request = f.request("host");
    request.host_session_id = " ".into();
    assert!(
        f.repo
            .claim_work_execution_event(&f.project, &f.event, request, 50)
            .is_err()
    );
}
#[test]
fn current_json_requires_explicit_nullable_consumption_and_rejects_hidden_authority() {
    assert!(
        serde_json::from_value::<WorkExecutionAckRequest>(
            json!({"hostSessionId":"host","claimId":"claim","leaseId":"lease"})
        )
        .is_err()
    );
    assert!(serde_json::from_value::<WorkExecutionClaimRequest>(json!({"hostSessionId":"host","expectedWorkVersion":0,"expectedProjectRevision":0,"expectedSourceDigest":"digest","grant":true})).is_err());
}

#[test]
fn saved_chapter_failure_is_recoverable_without_replaying_writer() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(&dir.path().join("saved-chapter.db"));
    f.repo.connection.execute("UPDATE story_tasks_v2 SET operation='generateChapter',status='running',required_output_kind='draft',target_json=?2,input_manifest_json=json_set(input_manifest_json,'$.outputObjectId','draft-chapter-1','$.expectedCandidateRevisionId',NULL),capability_revoked_at=NULL WHERE task_id=?1",params![f.task.task_id,json!({"type":"chapter","chapterId":"chapter-1"}).to_string()]).unwrap();
    f.repo.save_candidate(SaveCandidateRequest {
        project_id:f.project.clone(),branch_id:BranchId::parse("main").unwrap(),object_id:ObjectId::parse("draft-chapter-1").unwrap(),object_kind:"draft".into(),
        revision_id:RevisionId::parse("saved-draft").unwrap(),commit_id:CommitId::parse("saved-commit").unwrap(),idempotency_key:IdempotencyKey::parse("saved-key").unwrap(),request_digest:"digest".into(),content_digest:"content".into(),
        content_json:json!({"chapterId":"chapter-1","chapterNumber":1,"title":"来信","body":"潮水退去后，一封信留在门口。"}).to_string(),expected_project_revision:0,expected_snapshot_id:SnapshotId::parse("initial").unwrap(),next_snapshot_id:SnapshotId::parse("saved-snapshot").unwrap(),expected_candidate_revision_id:None,
        chapter_projection:Some(ChapterProjection{chapter_id:"chapter-1".into(),number:1,title:"来信".into()}),task_output:None,linked_impact_ids:vec![],
    }).unwrap();
    f.repo.connection.execute("INSERT INTO story_task_outputs_v2(task_id,ordinal,output_kind,revision_id,completeness) VALUES(?1,0,'draft','saved-draft','chapterComplete')",[&f.task.task_id]).unwrap();
    f.repo.connection.execute("UPDATE story_tasks_v2 SET status='failed',error='TaskOutputMissing' WHERE task_id=?1",[&f.task.task_id]).unwrap();
    f.repo.connection.execute("UPDATE story_work_steps_v1 SET status='blocked',operation='generateChapter' WHERE current_attempt_id=?1",[&f.task.task_id]).unwrap();
    f.repo.connection.execute("UPDATE story_work_tasks_v1 SET kind='generation',status='blocked' WHERE id=?1",[&f.work]).unwrap();
    // Freeze a genuine generation source and writer method binding. An advisory
    // binding cannot be repurposed merely by changing the operation column.
    let run = f.task.task_id.clone();
    let digest = f.repo.project_source_watermarks(&f.project).unwrap()["digest"].clone();
    let state = json!({"id":run,"projectId":f.project.as_str(),"version":0,
        "stage":"coordinate","status":"blocked","activeTaskId":f.task.task_id,
        "stepToken":"saved-step","intentGeneration":0,"authorMessageId":"message-root",
        "completedChapters":0,"chapterDeliveries":[],"startChapterNumber":1,"targetChapters":1,
        "feedbackDecisions":[],"intentRevisions":[{"id":"generation-intent","generation":0,
            "instruction":"读取结果，按实际情况继续创作","effectiveFromChapter":1,
            "source":{"type":"authorMessage","authorMessageId":"message-root","createdAt":3}}],
        "deadlineAt":null,"usedTaskAttempts":1,"maxTaskAttempts":null,
        "sourceDigest":digest,"baseSnapshotId":"initial"});
    f.repo.connection.execute("INSERT INTO story_generation_runs_v1(run_id,project_id,version,state_json) VALUES(?1,?2,0,?3)",params![run,f.project.as_str(),state.to_string()]).unwrap();
    f.repo.connection.execute("UPDATE story_author_messages_v1 SET generation_run_id=?1,task_id=NULL,thread_id=NULL,created_at=3 WHERE message_id='message-root'",[&run]).unwrap();
    crate::generation_execution_scope::record(&f.repo.connection,&f.project,&run,&state,3).unwrap();
    crate::generation_author_source::validate_run(&f.repo.connection,&f.project,&run,&state).unwrap();
    let target = json!({"type":"chapter","chapterId":"chapter-1","runId":run,"intentGeneration":0});
    let mut manifest = json!({"outputObjectId":"draft-chapter-1","expectedCandidateRevisionId":null});
    let request = CreateTaskRequest { task_id:f.task.task_id.clone(),project_id:f.project.clone(),thread_id:"original-thread".into(),operation:"generateChapter".into(),input_revision:0,model:None,target_json:target.to_string(),input_manifest_json:manifest.to_string(),capability_hash:"a".repeat(64),capability_expires_at:i64::MAX };
    crate::generation_author_source::freeze(&f.repo.connection,&request,&target,&mut manifest).unwrap();
    crate::creative_methods::freeze(&f.repo.connection,&request,&target,&mut manifest).unwrap();
    f.repo.connection.execute("UPDATE story_tasks_v2 SET target_json=?2,input_manifest_json=?3 WHERE task_id=?1",params![f.task.task_id,target.to_string(),manifest.to_string()]).unwrap();
    let saved = crate::tasks::load_task(&f.repo.connection,&f.task.task_id).unwrap();
    assert!(crate::tasks::recoverable_output(&f.repo.connection,&saved).expect("fixture saved output"));
    let eligible = |f:&Fixture| crate::conversation_receipts::recoverable_saved_chapter(&f.repo.connection,&f.task.task_id).unwrap();
    assert!(eligible(&f));
    assert!(!crate::work_decision_status::retryable(&f.repo.connection,&f.task.task_id).unwrap());
    let mut ledger = crate::work_budget_storage::load(&f.repo.connection,&f.project,&f.work).unwrap();
    ledger.unknown_tokens=5;
    ledger.charges[0].unknown_tokens=5;
    ledger.charges[0].status="failed".into();
    f.repo.connection.execute("UPDATE story_work_budgets_v1 SET ledger_json=?2 WHERE work_task_id=?1",params![f.work,serde_json::to_string(&ledger).unwrap()]).unwrap();
    assert!(!eligible(&f),"finite budget keeps unknown usage fenced");
    ledger.unknown_tokens=0;
    ledger.charges[0].unknown_tokens=0;
    f.repo.connection.execute("UPDATE story_work_budgets_v1 SET ledger_json=?2 WHERE work_task_id=?1",params![f.work,serde_json::to_string(&ledger).unwrap()]).unwrap();
    assert!(eligible(&f));
    for state in ["paused","cancelled"] {
        f.repo.connection.execute("UPDATE story_work_tasks_v1 SET status=?2 WHERE id=?1",params![f.work,state]).unwrap(); assert!(!eligible(&f));
    }
    f.repo.connection.execute("UPDATE story_work_tasks_v1 SET status='blocked' WHERE id=?1",[&f.work]).unwrap();
    f.repo.connection.execute("UPDATE story_task_provider_budgets_v1 SET settled=0 WHERE task_id=?1",[&f.task.task_id]).unwrap(); assert!(!eligible(&f));
    f.repo.connection.execute("UPDATE story_task_provider_budgets_v1 SET settled=1 WHERE task_id=?1",[&f.task.task_id]).unwrap();
    f.repo.connection.execute("UPDATE story_task_outputs_v2 SET completeness='partial' WHERE task_id=?1",[&f.task.task_id]).unwrap(); assert!(!eligible(&f));
    f.repo.connection.execute("UPDATE story_task_outputs_v2 SET completeness='chapterComplete' WHERE task_id=?1",[&f.task.task_id]).unwrap();
    f.repo.connection.execute("INSERT INTO story_tool_operations_v1(operation_id,project_id,task_id,thread_id,turn_id,call_id,tool_name,request_hash,state,created_at,updated_at) VALUES('pending',?1,?2,'original-thread','turn','call','save_candidate','hash','accepted',1,1)",params![f.project.as_str(),f.task.task_id]).unwrap(); assert!(!eligible(&f));
    f.repo.connection.execute("UPDATE story_tool_operations_v1 SET state='rejected' WHERE operation_id='pending'",[]).unwrap(); assert!(eligible(&f));
    f.repo.connection.execute("UPDATE story_object_heads_v2 SET candidate_revision_id=NULL WHERE project_id=?1",[f.project.as_str()]).unwrap(); assert!(!eligible(&f));
}

#[test]
fn different_works_cannot_own_the_same_root_thread_even_in_one_host() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(&dir.path().join("thread-owner.db"));
    f.queued_successor();
    let version = f.request("a").expected_work_version;
    f.repo.acquire_work_execution_owner(&f.project,&f.work,WorkExecutionOwnerRequest {host_session_id:"host".into(),expected_work_version:version},50).unwrap();
    f.repo.create_task(CreateTaskRequest {task_id:"second-root".into(),project_id:f.project.clone(),thread_id:"original-thread".into(),
        operation:"advisory".into(),input_revision:0,model:None,target_json:"{}".into(),input_manifest_json:json!({"authorIntent":"新问题"}).to_string(),capability_hash:"c".repeat(64),capability_expires_at:1000}).unwrap();
    let second = f.repo.work_task(&f.project,"work-second-root").unwrap();
    for host in ["host","other-host"] {
        assert!(f.repo.acquire_work_execution_owner(&f.project,&second.id,WorkExecutionOwnerRequest{host_session_id:host.into(),expected_work_version:second.version},51).is_err());
    }
}

#[test]
fn proposed_new_scope_is_pending_without_execution_lease_or_false_ack() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = Fixture::new(&dir.path().join("story.sqlite"));
    f.repo.connection.execute("UPDATE story_work_tasks_v1 SET status='completed' WHERE id=?1", [&f.work]).unwrap();
    f.repo.connection.execute("INSERT INTO story_generation_handoffs_v1(proposal_id,source_task_id,idempotency_key,content_json) VALUES('proposal',?1,'handoff',?2)",
        params![f.task.task_id, json!({"state":"proposed","resumeRun":null}).to_string()]).unwrap();
    for now in [50, 80] {
        let admission = f.claim("host", now);
        assert_eq!(admission["admission"], "blocked");
        assert_eq!(admission["blockedReason"], "generationScopeApprovalRequired");
        assert!(admission["lease"].is_null());
        assert_eq!(admission["event"]["state"], "pending");
        assert!(admission["event"]["claimId"].is_null());
    }
    // A recorded continuation is a distinct state; it must reach normal
    // lifecycle admission instead of being classified as new-scope approval.
    f.repo.connection.execute("UPDATE story_generation_handoffs_v1 SET content_json=?1 WHERE proposal_id='proposal'",
        [json!({"state":"proposed","resumeRun":{"id":"original-run"}}).to_string()]).unwrap();
    assert_ne!(f.claim("host", 100)["blockedReason"], "generationScopeApprovalRequired");
}
