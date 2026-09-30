import type { PropertyRuleDefinition } from "./types";
import { eventRuleInputError } from "./EventRuleFields";

export const ruleKinds: Record<PropertyRuleDefinition["subjectKind"], string> = {
  character: "人物", organization: "组织", location: "地点", item: "物品", proposition: "命题", event: "事件",
};
export const ruleTypes = { boolean: "是 / 否", integer: "整数", text: "文字", entity: "实体引用" };
const bytes = (value: string) => new TextEncoder().encode(value).length;

export function ruleInputError(definitions: unknown, eventRules: unknown): string | null {
  if (!Array.isArray(definitions) || definitions.length > 64 || bytes(JSON.stringify({ definitions, eventRules })) > 16 * 1024) return "结构规则最多 64 条，完整内容不得超过 16 KiB。";
  const seen = new Set<string>();
  for (const [index, rule] of definitions.entries()) {
    if (!rule || typeof rule !== "object" || Object.keys(rule).length !== 5
      || Object.keys(rule).some((key) => !["predicate", "subjectKind", "schema", "description", "exceptions"].includes(key))
      || !Object.hasOwn(ruleKinds, rule.subjectKind) || !rule.schema || Object.keys(rule.schema).length !== 2
      || !["single", "multiple"].includes(rule.schema.cardinality) || !rule.schema.valueType
      || !Object.hasOwn(ruleTypes, rule.schema.valueType.type)
      || Object.keys(rule.schema.valueType).length !== (rule.schema.valueType.type === "entity" ? 2 : 1)
      || (rule.schema.valueType.type === "entity" && !Object.hasOwn(ruleKinds, rule.schema.valueType.kind))) return `第 ${index + 1} 条不是支持的结构规则。`;
    if (typeof rule.predicate !== "string" || !rule.predicate.trim() || bytes(rule.predicate) > 80) return `第 ${index + 1} 条属性键必填，最多 80 字节。`;
    if (typeof rule.description !== "string" || !rule.description.trim() || bytes(rule.description) > 512) return `第 ${index + 1} 条说明必填，最多 512 字节。`;
    if (!Array.isArray(rule.exceptions) || rule.exceptions.length > 8 || rule.exceptions.some((entry: unknown) => typeof entry !== "string" || !entry.trim() || bytes(entry) > 256)) return `第 ${index + 1} 条例外最多 8 条，每条必填且不超过 256 字节。`;
    const key = JSON.stringify([rule.subjectKind, rule.predicate]);
    if (seen.has(key)) return `第 ${index + 1} 条与已有主体种类和属性键重复。`;
    seen.add(key);
  }
  return eventRuleInputError(eventRules, definitions);
}

export function PropertyRuleFields({ rules, disabled, onChange }: {
  rules: PropertyRuleDefinition[]; disabled: boolean; onChange: (rules: PropertyRuleDefinition[]) => void;
}) {
  function update(index: number, patch: Partial<PropertyRuleDefinition>) {
    onChange(rules.map((rule, position) => position === index ? { ...rule, ...patch } : rule));
  }
  return <div className="property-rule-fields">
    {rules.map((rule, index) => <fieldset key={index} disabled={disabled}>
      <legend>规则 {index + 1}</legend>
      <label>主体种类<select value={rule.subjectKind} onChange={(event) => update(index, { subjectKind: event.target.value as PropertyRuleDefinition["subjectKind"] })}>
        {Object.entries(ruleKinds).map(([kind, label]) => <option key={kind} value={kind}>{label}</option>)}
      </select></label>
      <label>属性键<input value={rule.predicate} placeholder="例如 alive，与主张中的属性键对应" onChange={(event) => update(index, { predicate: event.target.value })} /></label>
      <label>值类型<select value={rule.schema.valueType.type} onChange={(event) => update(index, { schema: { ...rule.schema,
        valueType: event.target.value === "entity" ? { type: "entity", kind: "character" } : { type: event.target.value as "boolean" | "integer" | "text" } } })}>
        {Object.entries(ruleTypes).map(([kind, label]) => <option key={kind} value={kind}>{label}</option>)}
      </select></label>
      {rule.schema.valueType.type === "entity" && <label>引用种类<select value={rule.schema.valueType.kind} onChange={(event) => update(index, { schema: { ...rule.schema,
        valueType: { type: "entity", kind: event.target.value as PropertyRuleDefinition["subjectKind"] } } })}>
        {Object.entries(ruleKinds).map(([kind, label]) => <option key={kind} value={kind}>{label}</option>)}
      </select></label>}
      <label>同一时段的值<select value={rule.schema.cardinality} onChange={(event) => update(index, { schema: { ...rule.schema, cardinality: event.target.value as "single" | "multiple" } })}>
        <option value="single">只能有一个值</option><option value="multiple">允许多个成员</option>
      </select></label>
      <label className="property-rule-wide">说明<textarea value={rule.description} onChange={(event) => update(index, { description: event.target.value })} /></label>
      <label className="property-rule-wide">例外（每行一条，最多 8 条）<textarea value={rule.exceptions.join("\n")} onChange={(event) => update(index, { exceptions: event.target.value === "" ? [] : event.target.value.split("\n") })} /></label>
      <button type="button" onClick={() => onChange(rules.filter((_, position) => position !== index))}>移除规则 {index + 1}</button>
    </fieldset>)}
    <button type="button" disabled={disabled || rules.length >= 64} onClick={() => onChange([...rules, { predicate: "", subjectKind: "character", schema: { cardinality: "single", valueType: { type: "boolean" } }, description: "", exceptions: [] }])}>添加结构规则</button>
  </div>;
}

export function PropertyRuleSummary({ rules }: { rules: PropertyRuleDefinition[] }) {
  return <>{rules.length === 0 ? <p>此版本不定义结构规则。</p> : <ul className="property-rule-summary">{rules.map((rule) => <li key={JSON.stringify([rule.subjectKind, rule.predicate])}>
    <strong>{ruleKinds[rule.subjectKind]} · {rule.predicate}</strong>
    <span>{ruleTypes[rule.schema.valueType.type]}{rule.schema.valueType.type === "entity" ? `（${ruleKinds[rule.schema.valueType.kind]}）` : ""} · {rule.schema.cardinality === "single" ? "单值" : "多值"}</span>
    <p>{rule.description}</p>
    {rule.exceptions.length > 0 && <><p>有例外：适用性待解释，不自动执行单值/多值约束。</p><ul>{rule.exceptions.map((entry, index) => <li key={index}>{entry}</li>)}</ul></>}
  </li>)}</ul>}</>;
}
