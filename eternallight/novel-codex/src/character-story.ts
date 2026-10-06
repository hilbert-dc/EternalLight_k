import type { CharacterKnowledge, CreativeWorkspace, IdentityCatalog } from "./types";

export type CharacterRecord = { id: string; name: string; names: string[]; members: string[]; claims: IdentityCatalog["entries"][number]["claims"]; status: string };
export type StoryEntry = CharacterKnowledge["entries"][number];
export function characterRecords(catalog: IdentityCatalog | null): CharacterRecord[] {
  const groups = new Map<string, IdentityCatalog["entries"]>();
  for (const entry of catalog?.entries ?? []) {
    if (entry.kind !== "character") continue;
    groups.set(entry.canonicalId, [...(groups.get(entry.canonicalId) ?? []), entry]);
  }
  return [...groups].map(([id, entries]) => {
    const claims = entries.flatMap(entry => entry.claims);
    const names = [...new Set(claims.flatMap(claim => [claim.displayName, ...claim.aliases]).filter(name => name.trim()))];
    const ambiguous = catalog!.ambiguousNames.filter(group => group.canonicalIds.includes(id));
    return { id, name: names[0] ?? "未命名人物", names, members: [...new Set([id, ...entries.map(entry => entry.id)])], claims,
      status: ambiguous.some(group => !group.authorDecidedDistinct) ? "同名待核对" : entries.some(entry => entry.resolutionState === "authorMerged") ? "身份已合并" : ambiguous.length ? "已明确区分" : "" };
  });
}

const relationLabels: Record<string, string> = { memberOf: "属于", locatedAt: "位于", owns: "拥有", parentOf: "是其父母：", knows: "知道", believes: "相信", suspects: "怀疑", conceals: "隐瞒", enables: "使之成为可能", prevents: "阻止", motivates: "促使", foreshadows: "预示", paysOff: "兑现", contrasts: "对照", echoes: "呼应", reveals: "揭示", supersedes: "替代", inputDependency: "依据", causes: "导致", supports: "支持", contradicts: "矛盾于" };
const roleLabels: Record<string, string> = { actor: "行动者", recipient: "接收者", affected: "受影响者", location: "地点", instrument: "工具", item: "物品", witness: "目击者", beneficiary: "受益者" };
const epistemicLabels: Record<string, string> = { known: "已知", suspected: "怀疑", false: "错误认知" };
const kindLabels: Record<string, string> = { event: "经历", state: "状态", relation: "关系", epistemic: "认知", assertion: "人物信息" };
export function storyDescription(entry: StoryEntry, catalog: IdentityCatalog) {
  const names = new Map(catalog.entries.map(entity => [entity.id, entity.claims[0]?.displayName ?? "未命名对象"]));
  for (const entity of catalog.entries) if (!names.has(entity.canonicalId)) names.set(entity.canonicalId, entity.claims[0]?.displayName ?? "未命名对象");
  const name = (id: unknown) => typeof id === "string" ? names.get(id) ?? "未识别对象" : "未识别对象";
  const value = (input: unknown): string => {
    if (typeof input === "string") return input;
    if (typeof input === "number") return String(input);
    if (typeof input === "boolean") return input ? "是" : "否";
    if (input && typeof input === "object" && "entityId" in input) return name(input.entityId);
    return "尚无可展示的描述";
  };
  const claim = entry.assertion;
  const predicate = relationLabels[claim.predicate ?? ""] ?? claim.predicate ?? "已记录信息";
  let description = `${name(entry.canonicalSubjectId)} · ${predicate}：${value(claim.value)}`;
  if (claim.kind === "event") {
    const participants = claim.value && typeof claim.value === "object" && "participants" in claim.value && Array.isArray(claim.value.participants) ? claim.value.participants : [];
    description = participants.map(person => `${name(person.entityId)}（${roleLabels[person.role] ?? "参与者"}）`).join("、") || "参与者信息尚未汇集";
  } else if (claim.kind === "relation") description = `${name(entry.canonicalSubjectId)} ${predicate} ${name(entry.canonicalTargetId)}`;
  else if (claim.kind === "epistemic") description = `${name(entry.canonicalEpistemicOwnerId)} · ${epistemicLabels[claim.epistemicState ?? ""] ?? "认知记录"} · ${predicate}：${value(claim.value)}`;
  return { kind: kindLabels[claim.kind] ?? "故事信息", title: claim.kind === "event" ? names.get(claim.subjectId) ?? predicate : kindLabels[claim.kind] ?? "故事信息", description };
}

export function storySource(entry: Pick<StoryEntry, "sourceObjectId" | "sourceRevisionId">, workspace: CreativeWorkspace) {
  const chapter = workspace.chapters.find(item => entry.sourceObjectId === `draft-${item.id}`);
  const status = chapter?.accepted?.id === entry.sourceRevisionId ? "已发布正文" : chapter?.candidate?.id === entry.sourceRevisionId ? "已保存编辑稿" : "历史来源";
  return { chapter, status };
}
