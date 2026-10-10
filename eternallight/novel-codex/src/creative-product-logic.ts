import type { DraftRevision, FactProposal, OutlineNode } from "./types";

type DraftSceneGroup = DraftRevision["sceneGroups"][number];

export interface EditorBaseline {
  baseProjectRevision: number;
  baseCandidateRevisionId: string | null;
  baseContent: string;
}

export function valueForIdentity<T>(state: { identity: string; value: T } | null, identity: string | null): T | null {
  return state && identity && state.identity === identity ? state.value : null;
}

export function rebaseEditorBaseline<T extends EditorBaseline>(session: T, next: EditorBaseline): T {
  return { ...session, ...next };
}

export function formatFactValue(value: unknown): string {
  const serialized = JSON.stringify(value);
  return serialized === undefined ? String(value) : serialized;
}

export function newDraftScene(): DraftSceneGroup {
  return { id: `scene-${crypto.randomUUID()}`, sceneId: null, body: "", locked: false };
}

export function selectionAwareReviewChecksPass(
  checks: { id: string; passed: boolean; blocking: boolean }[],
  confirmedFacts: FactProposal[],
  sceneGroups: DraftSceneGroup[],
): boolean {
  const groups = new Map(sceneGroups.map((group) => [group.id, group]));
  return checks.every((check) => {
    if (!check.blocking || check.passed) return true;
    if (check.id === "fact-evidence") return confirmedFacts.every((fact) => fact.evidence.trim());
    if (check.id === "fact-evidence-anchor") {
      return confirmedFacts.every((fact) => fact.factKind === "author-secret" || groups.get(fact.sceneGroupId)?.body.includes(fact.evidence.trim()));
    }
    return false;
  });
}

export function outlinePathLabels(nodes: OutlineNode[]): Map<string, string> {
  const result = new Map<string, string>();
  const byId = new Map(nodes.flatMap((node) => node.id ? [[node.id, node] as const] : []));
  const label = (node: OutlineNode) => `${node.kind === "book" ? "全书" : node.kind === "volume" ? "卷" : "单元"} · ${node.publicTitle || node.title || "未命名"}`;
  for (const node of nodes) {
    if (!node.id) continue;
    const path: string[] = [];
    const visited = new Set<string>();
    let current: OutlineNode | undefined = node;
    while (current) {
      if (current.id && visited.has(current.id)) break;
      if (current.id) visited.add(current.id);
      path.unshift(label(current));
      current = current.parentId ? byId.get(current.parentId) : undefined;
    }
    result.set(node.id, path.join(" → "));
  }
  return result;
}

export function firstOutlineEditorError(nodes: OutlineNode[]): { nodeId: string; message: string } | null {
  const firstUntitled = nodes.find((node) => !node.title.trim());
  if (!firstUntitled) return null;
  return { nodeId: firstUntitled.id ?? firstUntitled.clientKey ?? "", message: "请先填写这个大纲节点的标题。" };
}

export function generationStatusLabel(task: { status: string; deliveryStatus: "pending" | "ready_for_review" | "incomplete"; outputRefs: unknown[] }): { label: string; failed: boolean } {
  if (task.deliveryStatus === "ready_for_review") return { label: "内容已保存", failed: false };
  if (task.status === "cancelled") return { label: task.outputRefs.length > 0 ? "已停止，部分稿已保存" : "生成已停止", failed: false };
  if (task.deliveryStatus === "incomplete" && task.outputRefs.length > 0) return { label: "生成部分完成", failed: false };
  if (task.status === "failed" || task.deliveryStatus === "incomplete") return { label: "生成失败", failed: true };
  return { label: "生成任务进行中", failed: false };
}
