/** Validate retention intent; Core owns the output-content protocol validation. */
export function prepareCodexToolOutputs(outputs) {
  if (outputs === undefined) return undefined;
  if (!Array.isArray(outputs)) throw new Error("Codex toolOutputs must be an array");
  return outputs.map(output => {
    if (!output || typeof output !== "object" || Array.isArray(output)
      || typeof output.name !== "string" || !output.name.trim()
      || !(output.namespace === null || typeof output.namespace === "string")
      || !(typeof output.output === "string" || Array.isArray(output.output))
      || (Object.hasOwn(output, "retain") && typeof output.retain !== "boolean")) {
      throw new Error("Invalid Codex tool output or retention intent");
    }
    return structuredClone({ name: output.name, namespace: output.namespace,
      output: output.output, retain: output.retain ?? false });
  });
}
