process.on("message", (message) => {
  if (message?.method === "start" && process.connected) {
    process.send({ id: message.id, error: { message: "fixture start failed", code: "fixture" } });
  }
});
