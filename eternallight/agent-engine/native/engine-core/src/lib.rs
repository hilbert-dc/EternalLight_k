mod authority;
mod http_transport;
pub use http_transport::{HttpEvent, HttpOptions, HttpTransport};
mod command_executor;
pub use command_executor::execute as execute_behavior_command;
mod rpc_pending;
pub use rpc_pending::RpcPending;
mod turn;
pub use turn::AppServerTurn;
mod process_transport;
pub use process_transport::{ProcessOptions, ProcessTransport};
mod reports;
pub use reports::execute as report_store_operation;
mod behavior;
mod behavior_run;
pub use behavior_run::BehaviorRun;
mod certification;
mod trace;
pub use trace::ExecutionTrace;
mod budget;
mod canonical;
mod config;
mod connection;
mod contracts;
mod evidence;
mod evidence_selection;
mod evidence_storage;
mod extensions;
mod message_evidence;
mod proof;
mod session;
pub use evidence_storage::execute as evidence_store_operation;
mod chat_stream;
mod lease;
mod persistent_budget;
mod portable;
mod recovery;
mod recovery_contract;
mod recovery_journal;
pub use recovery_journal::RecoveryJournal;
mod storage;
pub use persistent_budget::PersistentBudget;
pub use recovery::execute as recovery_operation;
pub use storage::durable_replace_json;
mod responses;
mod responses_request;
mod schema;
mod sse;
mod tokens;
mod tool_registry;
pub use authority::ExecutionAuthority;
pub use budget::TaskBudget;
pub use chat_stream::ChatStream;
pub use lease::RouteLineage;
pub use responses::ResponsesStream;
use serde_json::Value;
pub use sse::SseDecoder;

pub type Result<T> = std::result::Result<T, String>;

pub fn dispatch(method: &str, input: Value) -> Result<Value> {
    match method {
        method if method.starts_with("recoveryContract.") => {
            recovery_contract::dispatch(method, &input)
        }
        "report.define" => reports::define(&input),
        "certification.define" => certification::define(&input),
        "certification.materialize" => certification::materialize(&input),
        method if method.starts_with("behavior.") => behavior::dispatch(method, &input),
        "session.protocols" => session::protocols(&input),
        "session.compile" => session::compile(&input),
        "session.finalize" => session::finalize(&input),
        "evidence.select" => evidence_selection::select(&input),
        "evidence.credentialFailure" => evidence_selection::credential_failure(&input),
        "trace.classify" => Ok(trace::classify(&input)),
        method if method.starts_with("message.") => message_evidence::dispatch(method, &input),
        "config.compile" => config::compile(&input),
        "config.catalog" => config::catalog(&input),
        "connection.define" => connection::define(&input),
        "connection.endpoint" => connection::endpoint(&input),
        method if method.starts_with("extensions.") => extensions::dispatch(method, &input),
        method if method.starts_with("evidence.") => evidence::dispatch(method, &input),
        method if method.starts_with("proof.") => proof::dispatch(method, &input),
        "contracts.definitions" => Ok(contracts::definitions().clone()),
        "contracts.domain" => contracts::domain(canonical::string(&input)).map(Value::from),
        "contracts.profile" => contracts::profile(canonical::string(&input)),
        "lease.create" => lease::create(&input),
        "lease.validate" => lease::validate(&input),
        "responses.encode" => responses_request::encode(
            &input["request"],
            &input["connection"],
            canonical::string(&input["apiKey"]),
        ),
        "responses.decode" => responses::decode(&input["payload"], &input["context"]),
        "codec.encode" => portable::encode(
            canonical::string(&input["protocol"]),
            &input["request"],
            &input["connection"],
            canonical::string(&input["apiKey"]),
        ),
        "codec.decode" => portable::decode(
            canonical::string(&input["protocol"]),
            &input["payload"],
            &input["context"],
        ),
        name if name.starts_with("canonical.") => canonical::dispatch(name, input),
        name if name.starts_with("budget.") => budget::dispatch(name, input),
        "schema.check" => schema::check(&input["schema"]),
        "schema.convert" => schema::convert(&input["schema"]),
        "schema.validate" => schema::validate(
            &input["value"],
            &input["schema"],
            input["label"].as_str().unwrap_or("tool arguments"),
        ),
        "tokens.count" => Ok(tokens::count(input.as_str().unwrap_or("")).into()),
        "tokens.bound" => {
            tokens::bounded(
                &input["value"],
                input["label"].as_str().unwrap_or("model-visible item"),
            )?;
            Ok(Value::Null)
        }
        "tools.register" | "tools.require" | "tools.name" | "tools.snapshot" => {
            tool_registry::dispatch(method, input)
        }
        _ => Err(format!("unknown engine method: {method}")),
    }
}
