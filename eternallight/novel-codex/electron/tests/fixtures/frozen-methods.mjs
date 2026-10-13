import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
const text = readFileSync(new URL("../../../codex-runtime/skills/novel-workflow/SKILL.md", import.meta.url), "utf8");
function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
  return value;
}
export function withFrozenMethods(task, projectId = "project") {
  const digest = (value) => createHash("sha256").update(value).digest("hex");
  const binding = {
    taskId:task.id, projectId, inputRevision:task.inputRevision, operation:task.operation,
    snapshotId:"fixture-snapshot", executionMode:"engineManaged", grantedToolScope:"existingOperationContract",
    selectedMethods:[{id:"novel-workflow",version:"1.0.0",status:"stable",kind:"executionContract",digest:digest(text),text}],
  };
  binding.bindingDigest = digest(JSON.stringify(canonical(binding)));
  task.inputManifest = {...task.inputManifest,creativeCapabilityBinding:binding};
  return task;
}
