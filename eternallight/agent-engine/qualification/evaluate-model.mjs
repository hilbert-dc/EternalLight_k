import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { parseBehaviorEvaluationInput } from "../src/evaluation/command-executor.mjs";

export async function evaluateModelFile(inputPath, outputPath = null) {
  const absoluteInput = resolve(inputPath);
  const report = await parseBehaviorEvaluationInput(JSON.parse(readFileSync(absoluteInput, "utf8")));
  const absoluteOutput = resolve(outputPath ?? `.evaluation-reports/${basename(absoluteInput, ".json")}-${report.reportId}.json`);
  mkdirSync(dirname(absoluteOutput), { recursive: true });
  writeFileSync(absoluteOutput, `${JSON.stringify(report, null, 2)}\n`, { flag: "wx", mode: 0o600 });
  return Object.freeze({ outputPath: absoluteOutput, reportId: report.reportId, status: report.evaluation.status, mode: report.mode });
}

const scriptPath = fileURLToPath(import.meta.url);
if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  const inputPath = process.argv[2];
  if (!inputPath) throw new Error("usage: node qualification/evaluate-model.mjs <evaluation-input.json> [report.json]");
  process.stdout.write(`${JSON.stringify(await evaluateModelFile(inputPath, process.argv[3]))}\n`);
}
