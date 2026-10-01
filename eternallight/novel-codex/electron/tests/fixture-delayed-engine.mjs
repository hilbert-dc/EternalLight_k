import readline from "node:readline";

const input = readline.createInterface({ input: process.stdin });
input.on("line", line => {
  const message = JSON.parse(line);
  if (message.id === undefined) return;
  const delay = Number(message.method === "initialize" ? process.env.FIXTURE_INITIAL_DELAY : process.env.FIXTURE_REQUEST_DELAY) || 0;
  setTimeout(() => process.stdout.write(JSON.stringify({id:message.id,result:{method:message.method}}) + "\n"),delay);
});
