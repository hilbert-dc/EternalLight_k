// @ts-check
export const MODEL_CONNECTION_UNAVAILABLE = "MODEL_CONNECTION_UNAVAILABLE";
const explanation = "当前模型验证不可用或已失效，请重新验证并应用模型后再发送。";

export function modelConnectionUnavailable(reason) {
  if (reason?.code === MODEL_CONNECTION_UNAVAILABLE) return reason;
  const details = Array.from(reason instanceof Error ? reason.message : String(reason)).slice(0, 2000).join("");
  return Object.assign(new Error(`${MODEL_CONNECTION_UNAVAILABLE}: ${explanation}\n原始诊断：${details}`), {
    code: MODEL_CONNECTION_UNAVAILABLE,
  });
}

// Status and execution share the same current-evidence check; this never verifies
// a provider or changes an applied plan.
export async function assertAgentAvailability(check) {
  try { return await check(); }
  catch (reason) { throw modelConnectionUnavailable(reason); }
}

export async function readAgentStatus({ readStatus, isActive, getOwner, check }) {
  const status = readStatus();
  if (isActive()) return status;
  const owner = getOwner();
  try {
    if (!status.configured) throw new Error(status.error ?? "创作引擎尚未配置");
    await check();
    if (isActive()) return readStatus();
    if (getOwner() !== owner) throw new Error("读取期间模型配置已变化，请重新读取当前模型状态");
    return readStatus();
  } catch (reason) {
    if (isActive()) return readStatus();
    return { ...readStatus(), configured: false, state: "error", error: modelConnectionUnavailable(reason).message };
  }
}
