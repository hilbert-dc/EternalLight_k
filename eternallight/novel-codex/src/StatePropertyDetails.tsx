import type { StateProperty } from "./types";

const valueLabels = { boolean: "布尔值", integer: "整数", text: "文字", entity: "实体引用" };
const entityLabels = { character: "人物", organization: "组织", location: "地点", item: "物品", proposition: "命题", event: "事件" };

export function StatePropertyDetails({ property }: { property?: StateProperty }) {
  if (!property) return null;
  const type = property.schema.valueType;
  return <span className="state-property-details">
    <small>属性状态 · {property.schema.cardinality === "single" ? "单值解释" : "多值成员"} · {valueLabels[type.type]}{type.type === "entity" ? `（${entityLabels[type.kind]}）` : ""}</small>
    <small>世界时间：{property.validFrom === null ? "开始时间未知" : `从 ${property.validFrom}`}；{property.validUntilExclusive === null ? "结束时间未定" : `到 ${property.validUntilExclusive} 前（不含结束时刻）`}</small>
    <small>规则由模型声明；确认本条主张不等于确认全局世界规则。区间结束不推导相反事实。</small>
  </span>;
}
