// Only fixed query diagnostics leave the probe; never retain rejected values or prose.
const messages = new Map([
  ["query plan byteBudget must be between 4000 and 9000 bytes inclusive; reduce steps or results when coverage is truncated, not below this minimum", "query_report_budget_invalid"],
  ["invalid query plan budget or question", "query_plan_invalid"],
  ["invalid or duplicate query step id", "query_step_invalid"],
  ["invalid query operation", "query_operation_invalid"],
  ["invalid semantic judgments", "query_judgments_invalid"],
  ["semantic baseline changed; query again", "query_baseline_changed"],
  ["semantic judgments must cover exactly the returned results", "query_judgments_incomplete"],
]);
export function queryFailureDiagnostic(nodes) {
  for (const node of nodes) {
    // MCP preflight failures are text content; Runtime failures are structured errors.
    const message = node.error?.message ?? (node.type === "text" ? node.text : null);
    if (typeof message !== "string" || message.length > 2048) continue;
    if (message.startsWith("{")) {
      let feedback;
      try { feedback = JSON.parse(message); } catch { continue; }
      if (feedback.code === "invalid_query_plan") {
        const path = typeof feedback.path === "string" ? feedback.path.replace(/^queryPlan/, "plan") : "";
        const safe = /^plan(?:\.(?:question|byteBudget|steps|id|operation|type|query|limit|cursor|maxChars|name|entityId|perspective|predicate|ownerId|readingChapter|worldTime|epistemicTime|relationType|direction|maxHops|ranking|baseReportHash|judgments|resultId|score|rationale)|\[(?:[0-9]|[1-5][0-9]|6[0-4])\]){0,8}$/.test(path);
        return { code: "invalid_query_plan", field: safe ? path.replace(/^plan\.?/, "").replace(/\[\d+\]/g, "[]") || null : null };
      }
    }
    for (const [known, code] of messages) {
      if (message === known || message === `InvalidContent(${JSON.stringify(known)})`) return { code, field: null };
    }
  }
  return { code: "unclassified", field: null };
}
