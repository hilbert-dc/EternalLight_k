use super::fixture::*;
use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

pub(super) struct Flow {
    pub fixture: Fixture,
    pub writer: Value,
    pub draft: Value,
    pub batch: Value,
    pub model: Value,
    pub grants: Vec<Value>,
}

pub(super) enum Goals {
    Approved,
    Absent,
}

impl Flow {
    pub async fn modeled(goals: Goals) -> Self {
        Self::from_fixture(Fixture::new().await, goals).await
    }
    pub async fn from_fixture(fixture: Fixture, goals: Goals) -> Self {
        Self::from_fixture_with_modeling(fixture, goals, true).await
    }
    pub async fn unmodeled() -> Self {
        let flow =
            Self::from_fixture_with_modeling(Fixture::new().await, Goals::Approved, false).await;
        flow.fixture
            .choose(&flow.writer, "assessRevisionOutcome")
            .await;
        flow
    }
    async fn from_fixture_with_modeling(
        fixture: Fixture,
        goals: Goals,
        model_knowledge: bool,
    ) -> Self {
        let mut grants = vec![fixture.grant(&fixture.discussion).await];
        let mut target = fixture.target.clone();
        if let Goals::Absent = goals {
            target.as_object_mut().unwrap().remove("desiredOutcomes");
        }
        let writer = fixture.writer(target).await;
        grants.push(fixture.grant(&writer).await);
        let mut output = fixture.original.clone();
        output["sceneGroups"][1]["body"] = json!(NEW_BODY);
        join_body(&mut output);
        let (status, draft) = fixture
            .checkpoint(Some(&writer), &fixture.original, output, "writer-draft")
            .await;
        assert_eq!(status, StatusCode::CREATED, "{draft}");
        let (status, complete) = fixture.complete(&writer).await;
        assert_eq!(status, StatusCode::OK, "{complete}");
        fixture.settle(&writer);
        if !model_knowledge {
            return Self {
                fixture,
                writer,
                draft,
                batch: Value::Null,
                model: Value::Null,
                grants,
            };
        }
        let model_step = fixture.choose(&writer, "modelKnowledge").await;
        let mut target = model_step["target"].clone();
        target["resumeStepId"] = model_step["id"].clone();
        let (status, model) = fixture.create_task("modelKnowledge", target).await;
        assert_eq!(status, StatusCode::CREATED, "{model}");
        let batch = fixture
            .repo()
            .modeling_batch(
                &fixture.project(),
                model["target"]["batchId"].as_str().unwrap(),
            )
            .unwrap()
            .as_json();
        grants.push(fixture.grant(&model).await);
        let body = draft["body"].as_str().unwrap();
        let quote = "密函";
        let start = body[..body.find(quote).unwrap()].chars().count();
        let evidence = json!({"quote":quote,"start":start,"end":start+quote.chars().count()});
        let (status, published) = fixture.request("POST",
            &format!("/modeling-batches/{}/publish",batch["id"].as_str().unwrap()), CANDIDATE,
            json!({"taskId":model["id"],"threadId":model["threadId"],"expectedRevision":0,
                "entities":[{"id":"letter","kind":"item","displayName":"密函","aliases":[],"evidence":evidence}],
                "assertions":[{"kind":"assertion","subjectId":"letter","predicate":"location",
                    "value":"衣襟","evidence":evidence}]})).await;
        assert_eq!(status, StatusCode::OK, "{published}");
        assert_eq!(published["status"], "ready");
        assert_eq!(
            step(&fixture.work().await, "modelKnowledge")["id"],
            model_step["id"]
        );
        Self {
            fixture,
            writer,
            draft,
            batch,
            model,
            grants,
        }
    }
    pub async fn queued() -> Self {
        let flow = Self::modeled(Goals::Approved).await;
        let (status, completed) = flow.fixture.complete(&flow.model).await;
        assert_eq!(status, StatusCode::OK, "{completed}");
        flow.fixture.settle(&flow.model);
        flow.fixture
            .choose(&flow.model, "assessRevisionOutcome")
            .await;
        flow
    }
    pub async fn assessment_target(&self) -> Value {
        let queued = step(&self.fixture.work().await, "assessRevisionOutcome");
        let mut target = queued["target"].clone();
        target["resumeStepId"] = queued["id"].clone();
        target
    }
    pub async fn assessment(&self) -> Value {
        let (status, task) = self
            .fixture
            .create_task("assessRevisionOutcome", self.assessment_target().await)
            .await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        task
    }
    pub fn path(&self) -> String {
        format!("/revision-outcomes/{}", self.writer["id"].as_str().unwrap())
    }
    pub fn report(&self) -> Value {
        json!({"writerTaskId":self.writer["id"],"chapterId":"chapter-one",
        "originalRevisionId":self.fixture.original["revisionId"],"draftRevisionId":self.draft["revisionId"],
        "knowledgeBatchId":self.batch["id"],"outcomes":[
            {"id":"hide-letter","status":"supported","reason":"改稿明确藏信。","evidence":[evidence("letter",NEW_BODY,"她将密函藏进衣襟。") ]},
            {"id":"confess","status":"unmet","reason":"改稿明确没有坦白。","evidence":[evidence("letter",NEW_BODY,"她没有向守卫坦白。") ]},
            {"id":"guard-motive","status":"insufficientEvidence","reason":"守卫场景未改，动机没有新证据。","evidence":[]}
        ]})
    }
    pub fn submission(&self, task: &Value, report: Value, key: &str) -> Value {
        json!({"taskId":task["id"],"threadId":task["threadId"],"expectedRevision":0,
            "expectedCandidateRevisionId":null,"idempotencyKey":key,"payload":report})
    }
}

pub(super) fn evidence(group: &str, body: &str, quote: &str) -> Value {
    let start = body[..body.find(quote).unwrap()].chars().count();
    json!({"groupId":group,"quote":quote,"start":start,"end":start+quote.chars().count()})
}
