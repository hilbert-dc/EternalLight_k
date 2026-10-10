import type { ChapterAcceptanceReport, WorkTaskSummary } from "./types";
import { CreationIssueReference } from "./CreationIssues";

const checkLabels = { supported: "满足", unmet: "未满足", insufficientEvidence: "依据不足" };

function progress(task: WorkTaskSummary, report: ChapterAcceptanceReport) {
  if (report.verdict === "pending") return report.chapterAcceptance.verification?.method === "direct"
    ? "正文已保存，待核对" : "正文已保存，待独立检查";
  if (!report.current || report.verdict === "stale") return "检查尚需重新核对";
  if (report.verdict === "passed") return "当前稿检查通过";
  if (report.verdict === "insufficientEvidence") return "检查依据不足，待补充核对";
  const writers = task.steps.filter(step => ["generateChapter", "writeNextChapter"].includes(step.operation)
    && step.target.chapterId === report.chapterId);
  return writers.some(step => step.status === "running") ? "检查未通过，正在修订"
    : writers.some(step => step.status === "queued") ? "检查未通过，准备修订" : "检查未通过，待修订";
}

export function WorkChapterAcceptance({ task, onRetry }: { task: WorkTaskSummary; onRetry: () => void }) {
  // A malformed current-protocol response is an explicit read error. Do not
  // substitute an empty array, older fields, or a successful completion state.
  if (!Array.isArray(task.deliveryReport.chapterAcceptanceReports)) return <p role="alert">章节检查记录无法核对。<button type="button" className="text-button" onClick={onRetry}>重新读取任务</button></p>;
  // Each receipt must belong to a writer attempt in this Work. Derive ordering
  // from its registered step, never the arrival order of reports or reply prose.
  const reports = new Map<string, { report: ChapterAcceptanceReport; ordinal: number }>();
  for (const report of task.deliveryReport.chapterAcceptanceReports) {
    const ordinal = task.steps.findIndex(step => step.target.chapterId === report.chapterId
      && step.attempts.some(attempt => attempt.id === report.writerTaskId));
    if (ordinal < 0) continue;
    const prior = reports.get(report.chapterId);
    if (!prior || ordinal > prior.ordinal || ordinal === prior.ordinal && report.current) reports.set(report.chapterId, { report, ordinal });
  }
  if (!reports.size) return null;
  const ordered = [...reports.values()].sort((a, b) => a.ordinal - b.ordinal);
  const earlier = ordered.slice(0, -3), recent = ordered.slice(-3);
  const render = ({ report, ordinal }: { report: ChapterAcceptanceReport; ordinal: number }) => {
    const number = task.steps[ordinal].target.chapterNumber;
    const label = typeof number === "number" && Number.isSafeInteger(number) && number > 0 ? `第 ${number} 章` : "本章";
    return <section key={report.chapterId}>
      <p role="status">{label} · {progress(task, report)}</p>
      <details><summary>查看检查详情</summary>
        <p>正文 {report.length.characterCount} 字符（不含空白，含标点）</p>
        <p>篇幅要求：{report.chapterAcceptance.length.requirement}</p>
        {!report.length.satisfied && <p>篇幅尚未符合要求。</p>}
        {!report.current && report.verdict !== "pending" && <p>这份结论尚不能代表当前正文，需要重新核对。</p>}
        {report.checks.length > 0 && <ul>{report.checks.map(check => {
          const requirement = report.chapterAcceptance.requirements.find(requirement => requirement.id === check.id);
          return <li key={check.id}>{requirement && <strong>{requirement.text} · </strong>}{checkLabels[check.status]}：{check.reason}
            {check.issueId && <CreationIssueReference issueId={check.issueId} />}
          </li>;
        })}</ul>}
      </details>
    </section>;
  };
  return <div className="work-chapter-acceptance" aria-label="章节检查进展">
    {recent.map(render)}
    {earlier.length > 0 && <details><summary>更早 {earlier.length} 章的检查</summary>{earlier.map(render)}</details>}
  </div>;
}
