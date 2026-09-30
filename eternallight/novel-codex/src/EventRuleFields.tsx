import type { EventCondition, EventRole, EventRuleDefinition, PropertyRuleDefinition } from "./types";

const roles: Record<EventRole, { label: string; kinds: string[] }> = {
  actor: { label: "行动者", kinds: ["character", "organization"] }, recipient: { label: "接收者", kinds: ["character", "organization"] },
  beneficiary: { label: "受益者", kinds: ["character", "organization"] }, affected: { label: "受影响者", kinds: ["character", "organization", "location", "item"] },
  location: { label: "发生地点", kinds: ["location"] }, instrument: { label: "工具", kinds: ["item"] }, item: { label: "物品", kinds: ["item"] },
  witness: { label: "见证者", kinds: ["character"] },
};
const bytes = (value: string) => new TextEncoder().encode(value).length;
const text = (value: unknown, limit: number): value is string => typeof value === "string" && !!value.trim() && bytes(value) <= limit;
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
const shape = (value: Record<string, unknown>, fields: string[]) => Object.keys(value).length === fields.length && fields.every((key) => Object.hasOwn(value, key));
const quantifier = (value: unknown) => value === "all" || value === "any";
const compatibleRole = (role: unknown, kind: unknown) => typeof role === "string" && Object.hasOwn(roles, role) && typeof kind === "string" && roles[role as EventRole].kinds.includes(kind);

export function eventRuleInputError(eventRules: unknown, definitions: PropertyRuleDefinition[]): string | null {
  if (!Array.isArray(eventRules) || eventRules.length > 16) return "事件规则最多 16 条，必须提供完整数组。";
  const seen = new Set<string>();
  for (const [index, rule] of eventRules.entries()) {
    let nodes = 0;
    function valid(condition: unknown, depth: number): boolean {
      if (++nodes > 31 || depth > 4 || !record(condition)) return false;
      if (condition.type === "all" || condition.type === "any") return shape(condition, ["type", "conditions"])
        && Array.isArray(condition.conditions) && condition.conditions.length > 0 && condition.conditions.length <= 8
        && condition.conditions.every((child) => valid(child, depth + 1));
      if (condition.type === "not") return shape(condition, ["type", "condition"]) && valid(condition.condition, depth + 1);
      if (condition.type !== "state" || !shape(condition, ["type", "role", "subjectKind", "predicate", "quantifier", "operator", "operand"])
        || !compatibleRole(condition.role, condition.subjectKind) || !quantifier(condition.quantifier)) return false;
      const definition = definitions.find((item) => item.subjectKind === condition.subjectKind && item.predicate === condition.predicate);
      const operand = condition.operand;
      if (!definition || !record(operand) || !["eq", "ne", "lt", "le", "gt", "ge"].includes(String(condition.operator))) return false;
      const type = definition.schema.valueType;
      if (condition.operator !== "eq" && condition.operator !== "ne" && type.type !== "integer") return false;
      if (type.type === "entity") return operand.type === "participant" && shape(operand, ["type", "role", "quantifier"])
        && quantifier(operand.quantifier) && compatibleRole(operand.role, type.kind);
      if (!shape(operand, ["type", "value"]) || operand.type !== type.type) return false;
      if (type.type === "boolean") return typeof operand.value === "boolean";
      if (type.type === "integer") return Number.isSafeInteger(operand.value);
      return text(operand.value, 512);
    }
    if (!record(rule) || !shape(rule, ["eventType", "description", "exceptions", "condition"])
      || !text(rule.eventType, 80) || !text(rule.description, 512) || seen.has(rule.eventType)
      || !Array.isArray(rule.exceptions) || rule.exceptions.length > 8 || !rule.exceptions.every((item) => text(item, 256))
      || bytes(JSON.stringify(rule)) > 4096 || !valid(rule.condition, 0)) return `第 ${index + 1} 条事件规则的类型、角色、属性引用或表达式界限无效。`;
    seen.add(rule.eventType);
  }
  return null;
}

function ConditionSummary({ condition }: { condition: EventCondition }) {
  if (condition.type === "not") return <li>非<ul><ConditionSummary condition={condition.condition} /></ul></li>;
  if (condition.type === "state") {
    const operators = { eq: "等于", ne: "不等于", lt: "小于", le: "小于等于", gt: "大于", ge: "大于等于" };
    const operand = condition.operand.type === "participant"
      ? `${condition.operand.quantifier === "all" ? "全部" : "任一"}${roles[condition.operand.role].label}`
      : typeof condition.operand.value === "boolean" ? condition.operand.value ? "是" : "否" : String(condition.operand.value);
    return <li>{condition.quantifier === "all" ? "全部" : "任一"}{roles[condition.role].label}（{condition.subjectKind}）的 {condition.predicate} {operators[condition.operator]} {operand}</li>;
  }
  return <li>{condition.type === "all" ? "同时满足" : "任一满足"}<ul>{condition.conditions.map((child, index) => <ConditionSummary key={index} condition={child} />)}</ul></li>;
}

export function EventRuleSummary({ rules, disabled, onRemove }: { rules: EventRuleDefinition[]; disabled?: boolean; onRemove?: (index: number) => void }) {
  return <>{rules.length > 0 && <><p>正式采用后按原始来源进行四值条件解释；来源缺失或未解释项保持未知，不认证世界真相或执行效果。通过对话提出或调整条件。</p>
    <ul className="property-rule-summary">{rules.map((rule, index) => <li key={rule.eventType}>
      <strong>事件 · {rule.eventType}</strong><p>{rule.description}</p><ul><ConditionSummary condition={rule.condition} /></ul>
      {rule.exceptions.length > 0 && <><p>例外仍待解释：</p><ul>{rule.exceptions.map((item, index) => <li key={index}>{item}</li>)}</ul></>}
      {onRemove && <button type="button" disabled={disabled} onClick={() => onRemove(index)}>移除事件规则 {index + 1}</button>}
    </li>)}</ul></>}</>;
}
