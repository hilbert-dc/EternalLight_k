// Product-reviewed choices, not a provider capability or an execution grant.
// Each new model supplies its own mapping after the same acceptance checks.
const policies = {
  "deepseek-flash": {
    defaultEffort: "low",
    options: [
      { effort: "low", label: "标准", description: "创作基线，兼顾响应速度" },
      { effort: "high", label: "深入", description: "投入更多推理，可能需要更长时间" },
      { effort: "max", label: "最深", description: "使用该模型最高推理档，耗时和用量可能增加" },
    ],
  },
};

export function modelReasoning(entry, requested = null) {
  const policy = policies[entry.id];
  if (!policy) {
    if (requested !== null && requested !== undefined) throw new Error("当前模型尚未开放推理强度选择");
    return null;
  }
  const effort = requested ?? policy.defaultEffort;
  if (!policy.options.some(option => option.effort === effort)) throw new Error("该推理档位未通过当前模型的产品准入，请选择列表中的档位");
  return { effort, defaultEffort: policy.defaultEffort, options: policy.options.map(option => ({ ...option })) };
}

export function modelConnectionDefinition(entry, requested = null) {
  const reasoning = modelReasoning(entry, requested);
  return reasoning ? { ...entry, protocolOptions: { ...entry.protocolOptions,
    reasoningEffort: reasoning.defaultEffort, allowedReasoningEfforts: reasoning.options.map(option => option.effort) } } : entry;
}

// Atomic Host publication: persist first, then replace only selection metadata.
export function selectModelReasoning({ current, provider, model, effort, persist }) {
  if (!current || current.provider !== provider || ![current.id, current.model].includes(model)) throw new Error("当前模型已改变，请重新选择推理强度");
  if (typeof effort !== "string") throw new Error("请选择有效的推理档位");
  const reasoning = modelReasoning(current, effort);
  if (!reasoning) throw new Error("当前模型尚未开放推理强度选择");
  persist({ provider: current.provider, model: current.id, reasoningEffort: reasoning.effort });
  return { ...current, reasoning };
}

export function requestReasoningConfiguration(connections, current, connectionId) {
  const connection = connections.find(item => item.id === connectionId);
  if (!connection) throw new Error("模型请求连接不在当前执行范围内");
  if (!connection.protocolOptions?.allowedReasoningEfforts) return null;
  const effort = current?.id === connectionId ? current.reasoning?.effort : connection.protocolOptions?.reasoningEffort;
  return effort ? { reasoningEffort: effort } : null;
}
