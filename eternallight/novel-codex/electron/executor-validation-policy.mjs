// @ts-check
import { readFileSync } from "node:fs";

// Bundled executor names only. Native tests require exact agreement with its
// tool specs; neither model arguments nor connection settings can extend this.
const names = JSON.parse(readFileSync(new URL("./executor-validated-tools.json", import.meta.url), "utf8"));
if (!Array.isArray(names) || names.length === 0 || names.length > 64
  || names.some(name => typeof name !== "string" || !/^[a-z][a-z0-9_]*$/.test(name))
  || new Set(names).size !== names.length) throw new Error("Invalid bundled executor tool manifest");
export const executorValidatedToolNames = Object.freeze([...names]);
const trustedNames = new Set(executorValidatedToolNames);
export const isExecutorValidatedTool = name => trustedNames.has(name);
export const executorValidatedTools = Object.freeze(executorValidatedToolNames.flatMap(name => [
  Object.freeze({ namespace: "mcp__novel_workspace", name }),
  Object.freeze({ namespace: null, name: `mcp__novel_workspace__${name}` }),
]));
