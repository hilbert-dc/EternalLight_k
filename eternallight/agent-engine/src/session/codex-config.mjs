import { nativeCall } from "../native.mjs";
export const compileCodexModelCatalog = (plan, { baseInstructions, autoCompactTargetTokens } = {}) => nativeCall("config.catalog", { plan, baseInstructions, autoCompactTargetTokens });
// Optional mcpServer.inputSchemaMaxBytes compiles to Core's per-tool schema byte
// limit. Native config.compile validates it; this does not change model budgets.
// Optional autoCompactTokenLimitScope selects Core's total/body_after_prefix
// maintenance counter; full context capacity and provider admission remain intact.
// Optional postTurnCompactThresholdPercent (integer 0..100) maintains completed
// history against effective model capacity, including across resume boundaries.
export const compileCodexConfig = (input) => nativeCall("config.compile", input);
