import { readdirSync } from "node:fs";
import { join } from "node:path";

export function assertCodexMigrationVersionsUnique(repositoryRoot) {
  const stateRoot = join(repositoryRoot, "codex-rs", "state");
  for (const directory of readdirSync(stateRoot, { withFileTypes: true })) {
    if (!directory.isDirectory() || !directory.name.endsWith("migrations")) continue;
    const versions = new Map();
    for (const file of readdirSync(join(stateRoot, directory.name)).sort()) {
      const match = /^(\d+)_.+\.sql$/.exec(file);
      if (!match) continue;
      const version = BigInt(match[1]).toString();
      if (versions.has(version)) {
        throw new Error(`Duplicate Codex migration version ${version} in ${directory.name}: ${versions.get(version)} and ${file}`);
      }
      versions.set(version, file);
    }
  }
}
