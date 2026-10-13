import ts from "typescript";

/** serde_json emits integer values as JSON numbers; preserve strings, names and comments. */
export function jsonWireNumberTypes(source) {
  const file = ts.createSourceFile("contract.ts", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const edits = [];
  const visit = (node) => {
    if (node.kind === ts.SyntaxKind.BigIntKeyword) edits.push([node.getStart(file), node.end]);
    ts.forEachChild(node, visit);
  };
  visit(file);
  for (const [start, end] of edits.sort((a, b) => b[0] - a[0])) source = source.slice(0, start) + "number" + source.slice(end);
  return source;
}
