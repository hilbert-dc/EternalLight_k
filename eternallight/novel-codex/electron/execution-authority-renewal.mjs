// @ts-check
const RENEW_WINDOW_MS = 3 * 60_000;

// The host owns this loop. It never grants authority itself: both issuers validate
// the current persisted bindings before extending a still-live, finite lease.
export function maintainExecutionAuthority({ capability, sessionPlan, renewTask, renewPlan,
  isCurrent, now = Date.now, pollMs = 60_000, schedule = setTimeout, unschedule = clearTimeout }) {
  let active = true;
  let timer = null;
  let pending = null;
  let plan = sessionPlan;
  let taskExpiry = capability?.expiresAt * 1000;
  let taskComplete = !capability;
  let rejectFailure;
  const failure = new Promise((_resolve, reject) => { rejectFailure = reject; });
  void failure.catch(() => {});
  const current = () => active && isCurrent();
  const tick = async () => {
    if (!current()) return;
    try {
      const at = now();
      if (!taskComplete && taskExpiry - at <= RENEW_WINDOW_MS) {
        if (taskExpiry <= at) throw new Error("任务授权已过期，请恢复任务后继续");
        const renewed = await renewTask(capability.expiresAt);
        if (!current()) return;
        if (renewed === null) taskComplete = true;
        else {
          if (!Number.isSafeInteger(renewed.expiresAt)
            || renewed.expiresAt * 1000 <= taskExpiry
            || renewed.expiresAt * 1000 > now() + 30 * 60_000) {
            throw new Error("任务续期回执无效");
          }
          capability.expiresAt = renewed.expiresAt;
          taskExpiry = renewed.expiresAt * 1000;
        }
      }
      if (!current()) return;
      const expiry = Date.parse(plan.expiresAt);
      if (!Number.isFinite(expiry) || expiry <= now()) throw new Error("模型会话授权已过期，请重新验证模型");
      if (expiry - now() <= RENEW_WINDOW_MS) {
        plan = await renewPlan(plan, RENEW_WINDOW_MS + pollMs);
        if (!current()) return;
        if (Date.parse(plan.expiresAt) <= expiry) throw new Error("没有可续期的有效模型证据，请重新验证模型");
      }
      if (current()) timer = schedule(run, pollMs);
    } catch (error) {
      if (current()) {
        active = false;
        rejectFailure(Object.assign(error, { code: "EXECUTION_AUTHORITY_INVALID" }));
      }
    }
  };
  function run() { timer = null; pending = tick().finally(() => { pending = null; }); }
  timer = schedule(run, Math.min(pollMs, Math.max(0,
    Math.min(taskComplete ? Infinity : taskExpiry, Date.parse(plan.expiresAt)) - now() - RENEW_WINDOW_MS)));
  return { failure, async stop() {
    active = false;
    if (timer !== null) unschedule(timer);
    await pending;
  } };
}
