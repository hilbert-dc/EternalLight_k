import { app, dialog } from "electron";

try {
  await import("./main.mjs");
} catch (error) {
  console.error("小说工坊启动失败：", error);
  if (process.env.NOVEL_CODEX_PACKAGE_SMOKE !== "1") {
    dialog.showErrorBox("小说工坊无法启动", error instanceof Error ? error.message : String(error));
  }
  app.exit(1);
}
