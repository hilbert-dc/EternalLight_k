import type { DirectionDecision, DirectionDecisionValue } from './creative-workspace-types';

export function factKindLabel(kind: string): string {
  return ({ world: "世界事实", reader: "读者已知", character: "人物认知", "author-secret": "作者秘密" } as Record<string, string>)[kind] ?? "正文事实";
}

export function epistemicStateLabel(state: string): string {
  return ({ unknown: "未知", suspected: "怀疑", known: "已知", false: "已证伪" } as Record<string, string>)[state] ?? state;
}

export function manualDecisionKey(factId: string, targetKey: string): string {
  return `${factId}\0${targetKey}`;
}

export function DirectionDecisionEditor({ value, onChange, disabled = false }: { value?: DirectionDecisionValue; onChange: (value: DirectionDecisionValue) => void; disabled?: boolean }) {
  const decision = value?.decision ?? "";
  return <div className="direction-decision"><select disabled={disabled} aria-label="保护冲突处理决定" value={decision} onChange={(event) => onChange({ decision: event.target.value as DirectionDecision, note: value?.note ?? "" })}><option value="">请选择处理方式</option><option value="return-draft">退回草稿修改</option><option value="accept-and-open-impact">采用正文并打开反馈</option><option value="false-positive">确认是误报</option></select>{decision === "false-positive" && <input disabled={disabled} aria-label="误报说明" value={value?.note ?? ""} onChange={(event) => onChange({ decision, note: event.target.value })} placeholder="说明为什么该事实不影响此规划" />}</div>;
}
