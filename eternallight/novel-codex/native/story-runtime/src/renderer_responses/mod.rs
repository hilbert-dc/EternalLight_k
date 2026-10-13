//! Stable Renderer HTTP projections. Rust serde is the wire authority.
//! Arbitrary author content is confined to explicit JsonValue fields.
use serde::{Deserialize, Deserializer};
fn required_nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d:D)->Result<Option<T>,D::Error>{Option::<T>::deserialize(d)}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/", type = "true")]
#[serde(try_from = "bool", into = "bool")]
pub struct WireTrue;
impl TryFrom<bool> for WireTrue {type Error=&'static str;fn try_from(v:bool)->Result<Self,Self::Error>{if v {Ok(Self)}else{Err("expected true")}}}
impl From<WireTrue> for bool {fn from(_:WireTrue)->Self{true}}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "responses/", type = "false")]
#[serde(try_from = "bool", into = "bool")]
pub struct WireFalse;
impl TryFrom<bool> for WireFalse {type Error=&'static str;fn try_from(v:bool)->Result<Self,Self::Error>{if !v {Ok(Self)}else{Err("expected false")}}}
impl From<WireFalse> for bool {fn from(_:WireFalse)->Self{false}}

mod receipts;
pub use receipts::*;

mod media;
pub use media::*;

mod state;
pub use state::*;

mod workspace;
pub use workspace::*;

mod knowledge;
pub use knowledge::*;

mod planning;
pub use planning::*;

mod reviews;
pub use reviews::*;

mod quality;
pub use quality::*;

mod proposals;
pub use proposals::*;

mod search;
pub use search::*;

mod hypotheses;
pub use hypotheses::*;

mod generation;
pub use generation::*;

mod commitment;
pub use commitment::*;

mod work;
pub use work::*;

mod audit;
pub use audit::*;

mod catalog;
pub use catalog::*;

mod media_api;
pub use media_api::*;
