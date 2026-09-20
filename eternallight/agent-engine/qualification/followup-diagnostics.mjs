export function followupDiagnostics(actual, expected) {
  return {
    receivedType: typeof actual,
    receivedLength: typeof actual === "string" ? actual.length : null,
    expectedLength: expected.length,
    exactMatch: actual === expected,
    trimMatch: typeof actual === "string" && actual.trim() === expected,
    includesExpected: typeof actual === "string" && actual.includes(expected),
    includesMarker: typeof actual === "string" && actual.includes("FOLLOWUP_TOKEN="),
    firstDifference: typeof actual === "string" ? [...expected].findIndex((character, index) => actual[index] !== character) : null,
  };
}
