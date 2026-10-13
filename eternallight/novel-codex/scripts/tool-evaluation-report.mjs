import { mkdir, rename, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { randomUUID } from "node:crypto";

// Keep only locally assigned failure categories; raw errors may contain credentials or prose.
export function safeFailure(error, phase) {
  const code = error?.name === "TimeoutError" || error?.code === "EVALUATION_TIMEOUT" ? "timeout"
    : error?.code === "ERR_ASSERTION" ? "assertion_failed"
    : ["ENOENT", "ECONNREFUSED", "EPIPE"].includes(error?.code) ? "transport_unavailable" : "execution_failed";
  const causeCode = ["MCP_STARTUP_TIMEOUT", "MCP_STARTUP_FAILED", "MCP_PROCESS_ERROR", "MCP_PROCESS_EXIT"].includes(error?.code) ? error.code : typeof error?.code === "string" && /^(provider_|tool_|task_|cli_|context_|bridge_)[a-z_]{1,80}$/.test(error.code) ? error.code : null;
  const message = typeof error?.message === "string" ? error.message : "";
  const reportedHint = ["task token budget exhausted", "task request budget exhausted", "task duration budget exhausted", "discovered tool is outside the current admitted catalog"].find(value => message.includes(value));
  // Diagnostic observation only: never use provider text to authorize recovery or settlement.
  return { phase, code, ...(causeCode ? { causeCode } : {}), ...(reportedHint ? { reportedHint } : {}) };
}
export async function saveReport(path, report) {
  const target = resolve(path), temporary = `${target}.${randomUUID()}.tmp`;
  await mkdir(dirname(target), { recursive: true });
  await writeFile(temporary, JSON.stringify(report, null, 2) + "\n", { mode: 0o600 });
  await rename(temporary, target);
}
const sum = (rows, field, encoding) => rows.reduce((sum, row) => sum + (row[field]?.[encoding] ?? 0), 0);
export function comparisonModeHonored(tools, mode) {
  const saves = tools.filter(tool => tool.name === "save_draft_checkpoint");
  return ["full", "patch"].includes(mode)
    && saves.some(tool => tool.status === "completed" && tool.error === false)
    && saves.every(tool => tool.requestedMode === mode);
}
export function summarizeRuns(runs) {
  return ["full", "patch", "deliberateCorrection"].map(mode => {
    const rows = runs.filter(row => row.mode === mode);
    const successful = rows.filter(row => row.status === "passed");
    const latency = successful.map(row => row.elapsedMs).filter(Number.isFinite).sort((a,b)=>a-b);
    const tokenFields = Object.fromEntries(["inputTokens", "outputTokens"].map(field => [field,
      Object.fromEntries(["cl100k_base", "o200k_base"].map(encoding => [encoding, sum(rows, field, encoding)]))]));
    return { mode, cases: rows.length, passed: successful.length, failed: rows.length-successful.length,
      firstAttemptSuccessRate: rows.length ? rows.filter(row=>row.firstAttemptSucceeded === true).length/rows.length : null,
      correctionCount: rows.reduce((n,row)=>n+(row.corrections??0),0), ...tokenFields,
      latencyMs: { p50: latency.length ? latency[Math.ceil(latency.length*0.5)-1] : null,
        p95: latency.length ? latency[Math.ceil(latency.length*0.95)-1] : null }, latencyPopulation: "passed cases only" };
  });
}
export function checkToolEvaluation(report) {
  const failures=[];
  if (report.evidence !== "deterministicToolReplay" || report.status !== "completed" || report.fatalError) failures.push("incomplete_report");
  for (const mode of ["full", "patch", "deliberateCorrection"]) {
    const rows=report.runs.filter(row=>row.mode===mode);
    if (!Number.isSafeInteger(report.repeats) || report.repeats<1 || rows.length!==report.repeats*2 || rows.some(row=>row.status!=="passed")) failures.push(`${mode}_cases_failed_or_missing`);
    const keys=new Set(rows.map(row=>`${row.repeat}:${row.size}`));
    if(keys.size!==rows.length || rows.some(row=>!Number.isSafeInteger(row.repeat)||row.repeat<0||row.repeat>=report.repeats||!["short","long"].includes(row.size))) failures.push(`${mode}_case_identity_invalid`);
    if (mode!=="deliberateCorrection" && rows.some(row=>row.firstAttemptSucceeded!==true || row.corrections!==0 || row.contentPreserved!==true)) failures.push(`${mode}_correctness_regression`);
    if (mode==="deliberateCorrection" && rows.some(row=>row.corrections!==1)) failures.push("correction_regression");
  }
  for (const encoding of ["cl100k_base","o200k_base"]) {
    const full=sum(report.runs.filter(r=>r.mode==="full"&&r.size==="long"),"saveInputTokens",encoding);
    const patch=sum(report.runs.filter(r=>r.mode==="patch"&&r.size==="long"),"saveInputTokens",encoding);
    // Broad relative bound avoids hardware/UUID noise; correctness must stay 100%.
    if (!(full>0 && patch>0 && patch/full<=0.5)) failures.push(`${encoding}_long_save_token_regression`);
  }
  return { passed: failures.length===0, failures, thresholds:{validCaseSuccessRate:1,contentPreservationRate:1,maxLongSaveTokenRatio:0.5}, latencyGate:false };
}
