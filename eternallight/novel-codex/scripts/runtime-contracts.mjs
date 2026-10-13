import { jsonWireNumberTypes } from "./json-wire-types.mjs";
import { assertContractCoverage } from "./runtime-contract-coverage.mjs";
import { spawnSync } from "node:child_process";
import { mkdtemp, readdir, readFile, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const output = path.join(root, "shared/generated");
const staging = await mkdtemp(path.join(tmpdir(), "novel-contracts-"));
const check = process.argv.includes("--check");
async function files(dir) {
  const entries = await readdir(dir, { withFileTypes: true }).catch((error) => error.code === "ENOENT" ? [] : Promise.reject(error));
  return (await Promise.all(entries.map(async (entry) => entry.isDirectory() ? (await files(path.join(dir, entry.name))).map((name) => `${entry.name}/${name}`) : [entry.name]))).flat().sort();
}
try {
  // Public domain DTOs may be consumed directly by Json<T>, without a Runtime wrapper.
  let coverage = 0;
  for (const crate of ["story-core", "story-authoring", "story-runtime"]) {
    const directory = path.join(root, "native", crate, "src");
    for (const name of await files(directory)) {
      if (!name.endsWith(".rs") || name === "main.rs") continue; // process credentials are not HTTP DTOs
      coverage += assertContractCoverage(await readFile(path.join(directory, name), "utf8"), `${crate}/${name}`, { publicOnly: crate !== "story-runtime" });
    }
  }
  console.log(`Checking generation coverage for ${coverage} named serde DTOs.`);
  const result = spawnSync("cargo", ["test", "--manifest-path", "native/Cargo.toml", "-p", "novel-codex-story-core", "-p", "novel-codex-story-authoring", "-p", "novel-codex-story-runtime", "--lib", "export_bindings", "--", "--test-threads=1"], { cwd: root, env: { ...process.env, TS_RS_EXPORT_DIR: staging }, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`Rust contract export failed (${result.status ?? "signal"})`);
  const generated = await files(staging);
  if (!generated.length) throw new Error("No Rust wire contracts exported");
  for (const name of generated) {
    // serde_json transports Rust integers as JSON numbers, never JavaScript bigint literals.
    // This is a transport mapping, not a promise that integers outside JS's safe range are exact.
    const source = jsonWireNumberTypes(await readFile(path.join(staging, name), "utf8"));
    await writeFile(path.join(staging, name), source);
  }
  const typecheck = spawnSync(process.execPath, [path.join(root, "node_modules/typescript/bin/tsc"), "--noEmit", "--strict", "--skipLibCheck", "--target", "ES2022", "--module", "ESNext", "--moduleResolution", "Bundler", ...generated.map((name) => path.join(staging, name))], { cwd: root, stdio: "inherit" });
  if (typecheck.error) throw typecheck.error;
  if (typecheck.status !== 0) throw new Error("Generated wire declarations do not typecheck");
  if (check) {
    const committed = await files(output);
    const drift = [...new Set([...generated, ...committed])].filter((name) => !generated.includes(name) || !committed.includes(name));
    for (const name of generated.filter((name) => committed.includes(name))) {
      if (await readFile(path.join(staging, name), "utf8") !== await readFile(path.join(output, name), "utf8")) drift.push(name);
    }
    if (drift.length) throw new Error(`Rust/TypeScript contract drift: ${drift.join(", ")}. Run npm run generate:runtime-contracts.`);
    console.log(`Verified ${generated.length} generated Rust wire contracts.`);
  } else {
    await rm(output, { recursive: true, force: true });
    for (const name of generated) {
      await mkdir(path.dirname(path.join(output, name)), { recursive: true });
      await writeFile(path.join(output, name), await readFile(path.join(staging, name)));
    }
    console.log(`Generated ${generated.length} Rust wire contracts.`);
  }
} finally { await rm(staging, { recursive: true, force: true }); }
