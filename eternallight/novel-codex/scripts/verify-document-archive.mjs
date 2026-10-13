import { readFile, access, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const organized = JSON.parse(await readFile(path.join(root, "docs/document-locations.json"), "utf8"));
const entries = JSON.parse(await readFile(path.join(root, "archive/reviews/manifest.json"), "utf8"));
for (const entry of [...entries, ...organized.map((entry) => ({ original: entry.original, archived: entry.current, reason: entry.status }))]) {
  if (!entry.reason?.trim()) throw new Error(`Missing archive evidence: ${entry.original}`);
  await access(path.join(root, entry.archived));
  try {
    await access(path.join(root, entry.original));
    throw new Error(`Archived document still has a competing root copy: ${entry.original}`);
  } catch (error) { if (error.code !== "ENOENT") throw error; }
}
for (const relative of [...(await readdir(path.join(root, "archive/reviews"))).filter((name) => name.endsWith(".md")).map((name) => `archive/reviews/${name}`), ...organized.map((entry) => entry.current)]) {
  const name = relative;
  const file = path.join(root, relative);
  const source = await readFile(file, "utf8");
  for (const [, link] of source.matchAll(/\]\(([^)]+)\)/g)) {
    const target = link.split("#")[0];
    // Historical absolute source pointers are preserved; only portable local links are gated.
    if (!target || target.includes("://") || path.isAbsolute(target)) continue;
    await access(path.resolve(path.dirname(file), target)).catch(() => { throw new Error(`Broken archived link: ${name} -> ${target}`); });
  }
}
console.log(`Verified ${entries.length} evidence-based archives, ${organized.length} organized documents and relative links.`);
