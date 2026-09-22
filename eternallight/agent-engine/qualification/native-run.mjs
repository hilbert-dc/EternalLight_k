import { nativeBinding } from "../src/native.mjs";
import { ModelBridgeProcess, TaskBudget, CodexAppServerClient } from "../src/index.mjs";

export async function runNativeQualification({ credentials, connections, sessionPlan, childModel = null, scenario = "message-running", ...metadata }, options) {
    const native = new (nativeBinding().NativeQualificationRun)(JSON.stringify({
      ...metadata, connections, sessionPlan, childModel, scenario, baseEnvironment: options.baseEnvironment,
      baseInstructions: options.baseInstructions, command: options.command, cwd: options.cwd,
      nodeCommand: options.nodeCommand, mcpPath: options.mcpPath, timeoutMs: options.timeoutMs,
    }));
    let bridge, adapter, budget, resolve, reject, timer;
    const completion = new Promise((yes, no) => { resolve = yes; reject = no; });
    const callback = async ({ method, params }) => {
      switch (method) {
        case "bridgeStart":
          bridge = new ModelBridgeProcess({ ...params, credentialResolver: async ref => credentials[ref] ?? null,
            requestObserver: value => apply("request", value), responseObserver: value => apply("response", value) });
          return bridge.start();
        case "budgetCreate": budget = new TaskBudget(params); return { ...budget.snapshot(), id: budget.id };
        case "budgetStart": await bridge.beginTaskBudget(budget); return null;
        case "leaseStart": await bridge.beginExecutionLease(params); return null;
        case "appCreate": adapter = new CodexAppServerClient(params); adapter.setCallbacks({ onEvent: value => apply("event", value) }); return null;
        case "ensureSession": return adapter.ensureSession();
        case "sendTurn": return adapter.sendTurn(params.input, params.options);
        case "budgetSnapshot": return budget?.snapshot() ?? null;
        case "closeApp": await adapter?.close(); return null;
        case "endLease": await bridge?.endExecutionLease(params); return null;
        case "endBudget": await bridge?.endTaskBudget(budget); return null;
        case "closeBridge": await bridge?.close(); return null;
        default: throw new Error(`Unknown native qualification callback: ${method}`);
      }
    };
    const apply = (method, input = {}) => {
      for (const effect of JSON.parse(native.apply(method, JSON.stringify(input)))) {
        if (effect.kind === "result") { clearInterval(timer); resolve(effect.result); }
        else if (effect.kind === "error") { clearInterval(timer); reject(Object.assign(new Error(effect.error.message), effect.error)); }
        else if (effect.kind === "callback") void callback(effect).then(
          result => apply("callback", { id: effect.id, result: result ?? null }),
          error => apply("callback", { id: effect.id, error: String(error?.message ?? error) }),
        );
        else throw new Error(`Unknown native qualification effect: ${effect.kind}`);
      }
    };
    timer = setInterval(() => apply("tick"), 20);
    try { apply("start"); return await completion; } finally { clearInterval(timer); }
  }
