use eternallight_engine_core::BehaviorRun;
use serde_json::{Value, json};

fn options() -> Value {
    json!({"taskSet":{"taskSetId":"evaluation","taskSetVersion":1,"tasks":[{"taskId":"one","prompt":"Read the document"}]},"repetitions":2,"now":10,"budget":{"maxTotalTokens":5,"maxDurationMs":100}})
}

#[test]
fn sample_identity_and_single_flight_protect_accounting() {
    let mut run = BehaviorRun::new(&options()).unwrap();
    let item = run.apply("next", &json!({"now":10})).unwrap();
    assert_eq!(item["sampleId"], 0);
    assert!(run.apply("next", &json!({"now":11})).is_err());
    assert!(
        run.apply("complete", &json!({"now":12,"sampleId":1}))
            .is_err()
    );
    let completion = json!({"now":15,"sampleId":0,"execution":{"usage":{"input_tokens":2,"output_tokens":3}},"score":{"outcome":"passed","blockingFailures":[]}});
    run.apply("complete", &completion).unwrap();
    assert!(run.apply("complete", &completion).is_err());
    assert_eq!(run.apply("next", &json!({"now":16})).unwrap(), Value::Null);
    let mut result = run.apply("result", &json!({"now":16})).unwrap();
    assert_eq!(
        result["budget"],
        json!({"maxExecutions":2,"maxTotalTokens":5,"maxDurationMs":100,"executions":1,"consumedTokens":5,"durationMs":6,"stoppedReason":"token-budget-exhausted"})
    );
    result["budget"]["consumedTokens"] = 0.into();
    assert_eq!(run.apply("next", &json!({"now":17})).unwrap(), Value::Null);
}

#[test]
fn over_budget_result_cannot_be_reported_as_passing() {
    let mut run = BehaviorRun::new(&options()).unwrap();
    run.apply("next", &json!({"now":10})).unwrap();
    run.apply("complete", &json!({"now":111,"sampleId":0,"execution":{"usage":{"totalTokens":6}},"score":{"outcome":"passed"}})).unwrap();
    let result = run.apply("result", &json!({"now":111})).unwrap();
    assert_eq!(
        result["samples"],
        json!([{"taskId":"one","category":"general","repetition":0,"outcome":"failed","blockingFailures":["token-budget-exceeded","duration-budget-exceeded"],"durationMs":101,"usage":{"totalTokens":6},"runRef":null,"executionFacts":null}])
    );
    assert!(run.apply("next", &json!({"now":110})).is_err());
}

#[test]
fn lazy_schedule_is_bounded_and_deadline_stops_unstarted_work() {
    let mut too_large = options();
    too_large["repetitions"] = 10_001.into();
    assert!(BehaviorRun::new(&too_large).is_err());
    let mut run = BehaviorRun::new(&options()).unwrap();
    assert!(run.apply("result", &json!({"now":10})).is_err());
    assert_eq!(run.apply("next", &json!({"now":110})).unwrap(), Value::Null);
    assert_eq!(
        run.apply("result", &json!({"now":110})).unwrap()["budget"],
        json!({"maxExecutions":2,"maxTotalTokens":5,"maxDurationMs":100,"executions":0,"consumedTokens":0,"durationMs":100,"stoppedReason":"duration-budget-exhausted"})
    );
}
