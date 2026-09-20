import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { evaluateModelFile } from "@eternallight/agent-engine/qualification/evaluate-model";

export { evaluateModelFile };

const scriptPath = fileURLToPath(import.meta.url);
if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  const inputPath = process.argv[2];
  if (!inputPath) throw new Error("usage: node scripts/evaluate-model.mjs <evaluation-input.json> [report.json]");
  process.stdout.write(`${JSON.stringify(await evaluateModelFile(inputPath, process.argv[3]))}\n`);
}
