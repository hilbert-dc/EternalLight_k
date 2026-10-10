import { readFileSync, writeFileSync, renameSync, unlinkSync } from "node:fs";
import { randomUUID } from "node:crypto";

export function readModelSelection(path) {
  let value;
  try { value = JSON.parse(readFileSync(path, "utf8")); }
  catch (error) { if (error.code === "ENOENT") return null; throw new Error("模型选择配置无法读取，请重新选择模型"); }
  if (!value || typeof value.provider !== "string" || typeof value.model !== "string"
    || value.provider.length > 40 || value.model.length > 120
    || !(value.reasoningEffort === null || typeof value.reasoningEffort === "string" && value.reasoningEffort.length <= 24)) throw new Error("模型选择配置无效，请重新选择模型");
  return value;
}

// Host-generated configuration only; no author documents or domain database.
// A failed switch must not leave next-start preferences pointing at a profile
// whose runtime config did not commit, or replace the still-active config.
export function withModelConfigurationRollback(paths, change) {
  const before = paths.map(path => {
    try { return { path, bytes: readFileSync(path) }; }
    catch (error) { if (error.code === "ENOENT") return { path, bytes: null }; throw error; }
  });
  try { return change(); }
  catch (error) {
    const failures = [];
    for (const { path, bytes } of before) {
      try {
        if (bytes === null) { try { unlinkSync(path); } catch (missing) { if (missing.code !== "ENOENT") throw missing; } }
        else {
          const temporary = `${path}.${randomUUID()}.restore`;
          try { writeFileSync(temporary, bytes, { mode: 0o600, flag: "wx" }); renameSync(temporary, path); }
          finally { try { unlinkSync(temporary); } catch (missing) { if (missing.code !== "ENOENT") throw missing; } }
        }
      } catch (failure) { failures.push(failure); }
    }
    if (failures.length) throw new AggregateError([error, ...failures], "模型切换失败，配置恢复未完成，请重新选择模型");
    throw error;
  }
}

export function saveModelSelection(path, selection) {
  const temporary = `${path}.${randomUUID()}.tmp`;
  try {
    writeFileSync(temporary, `${JSON.stringify(selection)}\n`, { mode: 0o600, flag: "wx" });
    renameSync(temporary, path);
  } finally {
    try { unlinkSync(temporary); } catch (error) { if (error.code !== "ENOENT") throw error; }
  }
}
