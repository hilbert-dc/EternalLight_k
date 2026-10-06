import React from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { SavedAnalysisDialog } from "../../../src/SavedAnalysisDialog";
import type { PlanningReviewRun, SavedAnalysis } from "../../../src/types";
import "../../../src/styles.css";

const host = window as typeof window & { testKey?: object | null; testViewport?: object | null; testResult?: object };
const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const check = (condition, message) => { if (!condition) throw Error(message); };
const luminance = color => color.match(/[\d.]+/g).slice(0, 3).map(Number).map(value => { const channel = value / 255; return channel <= .04045 ? channel / 12.92 : ((channel + .055) / 1.055) ** 2.4; }).reduce((total, value, index) => total + value * [.2126, .7152, .0722][index], 0);
const contrast = (left, right) => { const a = luminance(left), b = luminance(right); return (Math.max(a, b) + .05) / (Math.min(a, b) + .05); };
const until = async (predicate, message) => { for (let i = 0; i < 100 && !predicate(); i++) await tick(); check(predicate(), message); };
const key = async (keyCode, modifiers = []) => { host.testKey = { keyCode, modifiers }; await until(() => !host.testKey, "Native key did not settle"); await tick(); };
const run: PlanningReviewRun = { id: "run-original", workTaskId: "original-work", projectId: "project-A", artifactKind: "outline", revisionId: "historical-outline", targetNodeId: "original-volume", authorIntent: "保持原结局，检查第一卷人物动机", authorMessageId: "author-original", inputRevision: 4, roles: ["structure", "character", "experience"], sourcePacketHash: "a".repeat(64), status: "partial", coverageStatus: "partial", checksByRole: { structure: ["causality", "motivation", "ending"], character: ["arc"], experience: ["pacing"] }, seats: [], createdAt: "2026-10-03T00:00:00Z", updatedAt: "2026-10-03T00:01:00Z",
  reports: [{ id: "report-original", runId: "run-original", role: "structure", status: "partial", report: { summary: "原版本中的因果联系需要补足。", strengths: [{ quote: "他仍然决定赶回邮局。", reason: "行动目标明确，可保留。" }], findings: [{ quote: "她相信那封信，却没有打开。", observation: "信任来源不够明确", impact: "关键转折的因果尚缺依据。", suggestion: "补充此前收到可靠消息的经历。" }], coverage: [{ checkId: "causality", status: "checked", reason: "已对照指定卷的转折。" }, { checkId: "motivation", status: "unknown", reason: "材料不足，无法判断。" }] } },
    { id: "report-other", runId: "run-original", role: "character", status: "complete", report: { summary: "其他席位摘要不应替换所选报告。", strengths: [], findings: [], coverage: [{ checkId: "arc", status: "checked", reason: "人物弧线已检查。" }] } }],
  synthesis: { priorities: [], disagreements: ["是否提前透露信件内容，各席仍有不同意见。"], incompleteRoles: ["experience"] } };
const hypothesis: Extract<SavedAnalysis, { kind: "hypothesis" }> = { kind: "hypothesis", result: { id: "hypothesis-original", projectId: "project-A", baseSnapshotId: "snapshot-historical", intent: "假设邮差没有看到警告，仍保留原正文。", authority: "hypothesisOnly", assumptions: [{ kind: "assumeClaim", subjectId: "postman", predicate: "收到警告", value: false, storyTime: 4 }, { kind: "suppressClaim", modelingBatchId: "batch-original", sourceRevisionId: "source-original", assertionIndex: 3 }], baseline: { entryCount: 8 }, simulation: { entries: [{ canonicalSubjectId: "postman", confirmationState: "candidate", assertion: { predicate: "收到警告", value: true, storyTime: 4 } }, { canonicalSubjectId: "postman", confirmationState: "assumed", assertion: { predicate: "收到警告", value: false, storyTime: 4 } }], findings: [{ type: "polarityConflict", subjectId: "postman", predicate: "收到警告", confirmationState: "candidate", leftSourceRevisionId: "source-original", rightSourceRevisionId: null, leftEvidence: { quote: "邮差读过信上的警告，停在门口。" }, rightEvidence: null }], coverage: { projectionComplete: false, globalImpactComplete: false } } } };
hypothesis.result.simulation.findings.push(
  { type: "epistemicDisagreement", subjectId: "postman", predicate: "epistemicState", confirmationState: "candidateConflict", ownerId: "postman", propositionRef: { batchId: "batch-original", propositionId: "warning" }, leftState: "known", rightState: "suspected", leftSourceRevisionId: "source-original", rightSourceRevisionId: "source-later", leftEvidence: { quote: "邮差已知道门后有危险。" }, rightEvidence: { quote: "邮差仍怀疑信中内容。" } },
  { type: "stateValueConflict", subjectId: "postman", predicate: "年龄", confirmationState: "candidateConflict", schemaAuthority: "modelDeclared", propertyRulesRevisionId: null, leftSchema: { cardinality: "single", valueType: { type: "integer" } }, rightSchema: { cardinality: "single", valueType: { type: "integer" } }, leftValue: 18, rightValue: 19, leftSourceRevisionId: "source-original", rightSourceRevisionId: "source-later", leftEvidence: { quote: "邮差今年十八岁。" }, rightEvidence: { quote: "他仍是十九岁的邮差。" } },
  { type: "stateSchemaDisagreement", subjectId: "postman", predicate: "师傅", confirmationState: "candidateConflict", schemaAuthority: "authorConfirmed", propertyRulesRevisionId: "rules-original", leftSchema: { cardinality: "single", valueType: { type: "entity", kind: "character" } }, rightSchema: { cardinality: "multiple", valueType: { type: "text" } }, leftValue: { entityId: "mentor-original" }, rightValue: "旧称", leftSourceRevisionId: "source-original", rightSourceRevisionId: "source-later", leftEvidence: { quote: "他把老周认作唯一的师傅。" }, rightEvidence: { quote: "这几人都是他口中的师傅。" } },
);
let analysis: SavedAnalysis = { kind: "planningReview", run, reportId: "report-original" }, open = false, closes = 0;
const snapshots = [];
const root = createRoot(document.getElementById("root"));
const render = () => flushSync(() => root.render(<><main id="author-work" style={{ height: 200, overflow: "auto" }}><textarea id="editor" defaultValue={"未保存的作者正文。".repeat(80)} /><div style={{ height: 500 }} /><textarea id="composer" defaultValue="下一条尚未发送的构想" /></main><button id="launcher" onClick={() => { open = true; render(); }}>查看保存的分析</button>{open && <SavedAnalysisDialog analysis={analysis} onClose={() => { closes++; open = false; render(); }} />}</>));
const modal = () => document.querySelector<HTMLDialogElement>("dialog.saved-analysis-dialog");
const show = async () => { document.getElementById("launcher").focus(); document.getElementById("launcher").click(); await tick(); check(modal()?.matches(":modal"), "Saved result isn't a native modal"); };
const close = async () => { modal().querySelector<HTMLButtonElement>('[aria-label="关闭分析结果"]').click(); await tick(); };
const capture = label => snapshots.push({ case: label, headings: [...modal().querySelectorAll("h2,h3,h4")].map(node => node.textContent), scope: modal().querySelector(".saved-analysis-scope")?.textContent, warning: modal().querySelector('[role="status"], [role="alert"]')?.textContent ?? null, sourceCollapsed: !modal().querySelector<HTMLDetailsElement>(".saved-analysis-source")?.open });

(async () => { try {
  render(); const editor = document.getElementById("editor") as HTMLTextAreaElement, composer = document.getElementById("composer") as HTMLTextAreaElement, main = document.getElementById("author-work");
  editor.setSelectionRange(4, 16, "backward"); editor.scrollTop = 30; main.scrollTop = 100;
  const initial = { text: editor.value, start: editor.selectionStart, end: editor.selectionEnd, direction: editor.selectionDirection, scroll: editor.scrollTop, main: main.scrollTop, composer: composer.value };
  await show();
  check(document.activeElement === modal().querySelector("button"), "Modal lacks predictable close focus");
  check(modal().textContent.includes("原版本中的因果联系") && !modal().textContent.includes("其他席位摘要"), "Dialog selected latest or another seat");
  check(modal().textContent.includes("尚缺 1 项") && modal().textContent.includes("无法判断") && modal().textContent.includes("本轮仍有分歧"), "Partial coverage or disagreements disappeared");
  check(!modal().querySelector("form,input,textarea,pre") && modal().querySelectorAll("button").length === 1, "Read-only dialog exposes mutation controls or raw JSON"); capture("exact historical partial report");
  await key("Tab", ["shift"]); check(modal().contains(document.activeElement), "Shift-Tab escaped modal");
  await key("Tab"); check(modal().contains(document.activeElement), "Tab escaped modal");
  await key("Escape"); check(!modal() && document.activeElement?.id === "launcher", "Escape lost trigger focus");
  check(editor === document.getElementById("editor") && composer === document.getElementById("composer"), "Result remounted author editor");
  check(JSON.stringify(initial) === JSON.stringify({ text: editor.value, start: editor.selectionStart, end: editor.selectionEnd, direction: editor.selectionDirection, scroll: editor.scrollTop, main: main.scrollTop, composer: composer.value }), "Read-only result changed author text/selection/scroll/draft");

  for (const [label, reports] of [["missing report", []], ["duplicate report", [run.reports[0], run.reports[0]]], ["foreign run report", [{ ...run.reports[0], runId: "different-run" }]], ["foreign seat report", [{ ...run.reports[0], role: "experience" }]]]) {
    analysis = { kind: "planningReview", reportId: "report-original", run: { ...run, roles: ["structure", "character"], reports } } as SavedAnalysis;
    await show(); check(modal().querySelector('[role="alert"]') && !modal().textContent.includes("原版本中的因果联系"), `${label} was guessed as valid`); capture(label); await close();
  }

  analysis = hypothesis; await show();
  check(modal().textContent.includes("收到警告 为 否") && modal().textContent.includes("原记录未删除") && modal().textContent.includes("邮差读过信上的警告"), "Hypothesis conditions or evidence were lost");
  check(modal().textContent.includes("本次假设，没有正文原句") && modal().textContent.includes("当前投影未完整覆盖") && modal().textContent.includes("模拟保留 1 条原主张，加入 1 条假设"), "Hypothesis was presented as a formal/complete result"); capture("hypothesis distinct from story");
  const findings = [...modal().querySelectorAll("section")].find(section => section.querySelector("h3")?.textContent === "分析发现");
  check(findings.querySelectorAll("h4").length === 4 && findings.textContent.includes("同一时段的属性值存在冲突 · 年龄") && findings.textContent.includes("属性值：18 / 19") && findings.textContent.includes("属性解释规则存在分歧 · 师傅") && findings.textContent.includes("同一时刻只取一个值 · 关联人物") && findings.textContent.includes("同一时刻可有多个值 · 文本"), "Actual state-value/schema conflicts were misclassified or lost their typed values");
  check(modal().textContent.includes("仅覆盖原快照收录的正文与知识") && !modal().textContent.includes("未发布正文与之后的知识不在"), "Snapshot coverage incorrectly excludes all unpublished chapters");
  // New analysis replaces only modal content; focus never returns to an old background trigger.
  modal().querySelector<HTMLDetailsElement>(".saved-analysis-source").open = true;
  analysis = { kind: "hypothesis", result: { ...hypothesis.result, id: "hypothesis-empty", simulation: { ...hypothesis.result.simulation, findings: [] } } };
  render(); await tick(); check(document.activeElement === modal().querySelector("button") && !modal().querySelector<HTMLDetailsElement>(".saved-analysis-source").open, "New result retained stale details or returned background focus");
  check(modal().textContent.includes("未知或未覆盖后果仍需判断") && !modal().textContent.includes("全书没有影响"), "Empty findings falsely guarantee no impact"); capture("empty findings remain uncertain");
  analysis = hypothesis; render(); await tick();
  for (const theme of ["light", "dark"]) for (const width of [330, 740]) {
    document.documentElement.dataset.theme = theme; host.testViewport = { width, height: 500 }; await until(() => !host.testViewport, "Viewport not resized"); await tick();
    const box = modal().getBoundingClientRect(), style = getComputedStyle(modal());
    check(box.left >= 0 && box.right <= innerWidth + 1 && box.top >= 0 && box.bottom <= innerHeight + 1 && modal().scrollWidth <= modal().clientWidth + 1, "Saved analysis overflows viewport");
    check(style.color !== style.backgroundColor, "Saved analysis theme hides text");
    check(contrast(style.color, style.backgroundColor) >= 4.5, "Analysis text lacks contrast");
    const quoteStyle = getComputedStyle(modal().querySelector("blockquote"));
    check(contrast(quoteStyle.color, quoteStyle.backgroundColor) >= 4.5, "Evidence quote lacks contrast");
    modal().scrollTop = modal().scrollHeight; await tick();
    check(modal().querySelector(".saved-analysis-source").getBoundingClientRect().bottom <= modal().getBoundingClientRect().bottom + 1, "Source records cannot be reached in narrow modal");
    snapshots.push({ case: `${theme} ${width}`, inViewport: true, sourceReachable: true, text: style.color, background: style.backgroundColor });
  }
  await close(); check(composer.value === initial.composer && editor.value === initial.text && !modal(), "Closing result changed the author work");
  root.unmount(); host.testResult = { passed: true, closes, snapshots, authorPreserved: true };
} catch (error) { host.testResult = { passed: false, error: error.stack, dom: document.body.innerHTML }; } })();
