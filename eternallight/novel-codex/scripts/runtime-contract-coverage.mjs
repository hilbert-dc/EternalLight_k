/** Fail closed when a named serde transport type lacks a generated counterpart. */
export function assertContractCoverage(source, filename, { publicOnly = false } = {}) {
  // ts-rs cannot infer arbitrary serde wire transformations. The current custom
  // readers only require an explicit Option (null is valid; omission is not).
  const presenceReaders = new Set(["crate::deserialize_execution_limit", "novel_codex_story_core::deserialize_execution_limit", "required_continuation", "required_proposal", "required_decision_head", "required_nullable"]);
  const reviewedAttributes = new Set(["rename_all", "deny_unknown_fields", "rename", "deserialize_with", "tag", "default", "rename_all_fields", "skip_serializing_if", "flatten", "untagged", "try_from", "into", "transparent"]);
  for (const [, attributes] of source.matchAll(/#\[serde\(([\s\S]*?)\)\]/g)) {
    for (const [, name] of attributes.matchAll(/(?:^|,)\s*(\w+)/g)) {
      if (!reviewedAttributes.has(name)) throw new Error(`${filename}: unreviewed serde wire serialization attribute ${name}`);
    }
    for (const [, representation] of attributes.matchAll(/(?:try_from|into)\s*=\s*"([^"]+)"/g)) {
      if (!(representation === "BudgetLedger" && /(?:^|\/)provider_budget\.rs$/.test(filename)) && !(representation === "bool" && /renderer_responses\/mod\.rs$/.test(filename))) throw new Error(`${filename}: unreviewed serde representation ${representation}`);
    }
    if (/\b(?:serialize_with|with)\s*=/.test(attributes)) throw new Error(`${filename}: custom serde wire serialization requires an explicit generated-contract mapping and round-trip test`);
    for (const [, reader] of attributes.matchAll(/deserialize_with\s*=\s*"([^"]+)"/g)) {
      if (!presenceReaders.has(reader)) throw new Error(`${filename}: unreviewed serde deserializer ${reader}`);
    }
  }
  let count = 0;
  for (const match of source.matchAll(/#\[derive\(([^)]*\b(?:Serialize|Deserialize)\b[^)]*)\)\]([\s\S]*?)(pub(?:\([^)]*\))?\s+)?(?:struct|enum)\s+(\w+)/g)) {
    if (publicOnly && match[3]?.trim() !== "pub") continue;
    if (!match[1].includes("ts_rs::TS") || !/#\[ts\(\s*export\s*[,)]/.test(match[2])) throw new Error(`${filename}:${match[4]} is missing its generated wire contract`);
    count++;
  }
  return count;
}
