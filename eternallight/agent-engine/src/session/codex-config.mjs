import { nativeCall } from "../native.mjs";
export const compileCodexModelCatalog = (plan, { baseInstructions } = {}) => nativeCall("config.catalog", { plan, baseInstructions });
export const compileCodexConfig = (input) => nativeCall("config.compile", input);
