// @ts-check
function waitForClose(child, timeoutMs) {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve(true);
  return new Promise(resolve => {
    const onClose = () => { clearTimeout(timer); resolve(true); };
    const timer = setTimeout(() => { child.removeListener("close", onClose); resolve(false); }, timeoutMs);
    child.once("close", onClose);
  });
}

export async function terminateEngineProcess(child) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  if (!child.killed) child.kill();
  if (!await waitForClose(child, 1500) && child.exitCode === null && child.signalCode === null) {
    child.kill("SIGKILL");
    await waitForClose(child, 500);
  }
}
