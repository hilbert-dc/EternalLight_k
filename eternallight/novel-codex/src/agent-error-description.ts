export function isModelConnectionUnavailable(error: string): boolean {
  return /^(?:Error invoking remote method '[^']+': Error: )?MODEL_CONNECTION_UNAVAILABLE:/.test(error);
}

export function describeAgentError(error: string): string {
  if (isModelConnectionUnavailable(error)) return "当前模型验证不可用或已失效，请重新验证并应用模型后再发送。";
  if (/(?:^|Error: )invalid_tool_arguments$/.test(error)) return "模型提交的工具参数不符合要求，本轮未完成。已保存的内容仍会保留，请查看任务详情。";
  return error.includes("provider_output_item_limit")
    ? "模型返回的一条结果超过可处理的长度上限，本次任务未完成。"
    : error;
}
