import test from "node:test";
import assert from "node:assert/strict";
import { assertContractCoverage } from "../runtime-contract-coverage.mjs";

test("new HTTP serde DTOs cannot silently bypass generation", () => {
  assert.throws(() => assertContractCoverage("#[derive(Deserialize)]\nstruct Request { value: String }", "api.rs"), /Request.*missing/);
  assert.throws(() => assertContractCoverage("#[derive(Deserialize, ts_rs::TS)]\nstruct Request { value: String }", "api.rs"), /missing/);
});
test("multiline qualified derives, serde attributes and internal visibility are covered", () => {
  assert.equal(assertContractCoverage('#[derive(Debug, serde::Deserialize,\n ts_rs::TS)]\n#[ts(\n export, rename="ApiRequest")]\n#[serde(rename_all="camelCase")]\npub(crate) struct Request { value: String }', "api.rs"), 1);
});
test("public core DTOs are checked even without runtime wrappers", () => {
  assert.throws(() => assertContractCoverage("#[derive(Serialize)]\npub struct Receipt {}", "core.rs", { publicOnly: true }), /missing/);
  assert.equal(assertContractCoverage("#[derive(Serialize)]\nstruct Cursor {}", "core.rs", { publicOnly: true }), 0);
});

test("custom wire transformations require an explicit reviewed mapping", () => {
  assert.throws(() => assertContractCoverage('#[serde(serialize_with="number_as_string")]', "api.rs"), /wire serialization/);
  assert.throws(() => assertContractCoverage('#[serde(deserialize_with="string_as_number")]', "api.rs"), /unreviewed/);
  assert.throws(() => assertContractCoverage('#[serde(try_from="OtherShape")]', "api.rs"), /unreviewed/);
});
test("custom serialization via into cannot evade the reviewed wire mapping", () => {
  assert.throws(() => assertContractCoverage('#[serde(into="OtherShape")]', "api.rs"), /unreviewed/);
});
