let input = "";
for await (const chunk of process.stdin) input += chunk;
const request = JSON.parse(input);
if (request.contract !== "agent-engine-behavior-executor-v1") process.exit(2);
process.stdout.write(JSON.stringify({
  status: "completed",
  output: `${request.task.prompt} :: ${request.task.metadata.answer}`,
  runRef: `${request.task.taskId}:${request.repetition}`,
  usage: { inputTokens: 4, outputTokens: 3, source: "fixture" },
  executionFacts: { toolCalls: [], lineage: [] },
}));
