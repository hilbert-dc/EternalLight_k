function isOutputItemLimit(error: string): boolean {
  const message = error.replace(/^(?:Error invoking remote method '[^'\r\n]+': )?(?:Error: )?/, "");
  if (message === "provider_output_item_limit") return true;
  try {
    const envelope = JSON.parse(message);
    if (envelope?.code === "provider_output_item_limit" || envelope?.error?.code === "provider_output_item_limit") return true;
  } catch { /* A diagnostic string must match the known adapter format below. */ }
  const prefix = "provider_output_item_limit; outputItem=";
  if (!message.startsWith(prefix)) return false;
  try {
    const diagnostic = JSON.parse(message.slice(prefix.length));
    return ["function_call", "message", "reasoning"].includes(diagnostic?.type)
      && Number.isSafeInteger(diagnostic.utf8Bytes) && diagnostic.utf8Bytes >= 0
      && ((diagnostic.limitKind === "conservativeTokens" && diagnostic.limit === 10000
        && Number.isSafeInteger(diagnostic.conservativeTokens) && diagnostic.conservativeTokens > 10000)
        || (diagnostic.limitKind === "utf8Bytes" && diagnostic.limit === 262144
          && diagnostic.utf8Bytes > 262144 && diagnostic.conservativeTokens === null));
  } catch { return false; }
}

export function isModelConnectionUnavailable(error: string): boolean {
  return /^(?:Error invoking remote method '[^']+': Error: )?MODEL_CONNECTION_UNAVAILABLE:/.test(error);
}

export function describeAgentError(error: string): string {
  if (isModelConnectionUnavailable(error)) return "当前模型验证不可用或已失效，请重新验证并应用模型后再发送。";
  if (/^(?:Error invoking remote method '[^'\r\n]+': )?(?:Error: )?unexpected status 402 Payment Required(?:$|: )/.test(error)) {
    return "模型服务商账户的余额或计费状态需要处理，本轮已停止。已保存的内容仍保留，请先处理服务商账户。";
  }
  try {
    const failure = JSON.parse(error)?.error;
    if (failure?.code === "task_provider_usage_unknown" && failure.type === "budget_unknown") {
      return "本次模型请求未取得完整用量回执，旧请求已停止，已保存内容仍保留。这不代表额度已用完；能否继续以原任务的恢复状态为准，无需重复输入创作要求。";
    }
  } catch { /* Non-JSON failures retain their original classification. */ }
  if (/^(?:Error invoking remote method '[^'\r\n]+': )?(?:Error: )?unexpected status 401 Unauthorized(?:$|: )/.test(error)) {
    return "模型服务商未接受当前凭据。请在模型设置中更新凭据并验证连接，再从原任务继续；已保存内容仍保留。";
  }
  if (/^(?:Error invoking remote method '[^'\r\n]+': )?(?:Error: )?unexpected status 403 Forbidden(?:$|: )/.test(error)) {
    return "模型服务商拒绝了当前访问。请核对该账户的模型访问权限，或选择有权限的模型后从原任务继续；已保存内容仍保留。";
  }
  if (/(?:^|Error: )invalid_tool_arguments(?:$|; toolValidation=)/.test(error)) return "模型提交的工具参数不符合要求，本轮未完成。已保存的内容仍会保留，请查看任务详情。";
  if (/^(?:Error: )?unauthorized_tool_call(?:$|; toolValidation=)/.test(error)) return "模型调用了本轮未开放的工具，该调用未执行。这不代表需要您增加权限；应用会按当前可用能力尝试纠正，若仍无法推进会保留暂停原因。";
  if (isOutputItemLimit(error)) return "模型返回的一条结果超过可处理的长度上限，本次任务未完成。";
  if (/timeout|timed out|超时/i.test(error)) return "请求超时，本轮未完成。请检查连接后再继续。";
  if (/network|ECONN|fetch failed|连接失败/i.test(error)) return "连接中断，本轮未完成。请恢复连接后再继续。";
  return "本轮遇到问题，尚未完成。请查看错误详情和任务恢复安排。";
}
