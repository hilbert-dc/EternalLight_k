import { readdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

// checkJs stays opt-in because Host imports the Engine's independent JS source.
// Every production Host module must opt in; no file may silence diagnostics.
export async function verifyHostTypeCoverage(directory = new URL("../electron/", import.meta.url)) {
  const problems = [];
  let files = 0;
  async function scan(folder, prefix = "") {
  for (const entry of await readdir(folder, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (entry.name !== "tests") await scan(new URL(`${entry.name}/`, folder), `${prefix}${entry.name}/`);
      continue;
    }
    if (!entry.isFile() || !/\.(?:mjs|cjs)$/.test(entry.name)) continue;
    files++;
    const source = await readFile(new URL(entry.name, folder), "utf8");
    if (!/^\s*\/\/\s*@ts-check\b/m.test(source)) problems.push(`${prefix}${entry.name}: missing @ts-check`);
    if (/@ts-(?:nocheck|ignore|expect-error)\b/.test(source)) problems.push(`${prefix}${entry.name}: type checking suppression`);
  }
  }
  await scan(directory);
  return { files, problems };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = await verifyHostTypeCoverage();
  console.log(JSON.stringify(result, null, 2));
  if (result.problems.length) process.exitCode = 1;
}
