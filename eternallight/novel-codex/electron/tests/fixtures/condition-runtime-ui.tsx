import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { ConditionEvaluationPanel } from "../../../src/ConditionEvaluationPanel";

const fixture = window.conditionFixture;
createRoot(document.getElementById("root")).render(<ConditionEvaluationPanel
  api={window.novelCodex} projectId={fixture.projectId}
  target={fixture.target} initialJobId={fixture.jobId}
/>);
