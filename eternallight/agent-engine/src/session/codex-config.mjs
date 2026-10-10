import { nativeCall } from "../native.mjs";
export const compileCodexModelCatalog = (plan, { baseInstructions } = {}) => nativeCall("config.catalog", { plan, baseInstructions });
// Optional mcpServer.inputSchemaMaxBytes compiles to Core's per-tool schema byte
// limit. Native config.compile validates it; this does not change model budgets.
export const compileCodexConfig = (input) => nativeCall("config.compile", input);
