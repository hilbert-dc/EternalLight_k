import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { EventEmitter } from "node:events";

// Execute the production main handlers with a real Adapter; only Electron startup
// and the Runtime transport are replaced so the tests run without a display.
export async function mainHandlers(runtime, command) {
  const mainUrl = new URL("../main.mjs", import.meta.url);
  let source = await readFile(mainUrl, "utf8");
  const bindings = { process, console, URL, URLSearchParams, Buffer, setTimeout, clearTimeout };
  for (const match of source.matchAll(/^import \{ (.+) \} from "(.+)";$/gm)) {
    if (match[2] === "electron") continue;
    const namespace = await import(match[2].startsWith(".") ? new URL(match[2], mainUrl) : match[2]);
    for (const name of match[1].split(", ")) bindings[name] = namespace[name];
  }
  bindings.app = { isPackaged: false, requestSingleInstanceLock: () => false, quit() {}, setPath() {} };
  bindings.ipcMain = {};
  bindings.fetch = async (url, options) => {
    const payload = await runtime(new URL(url).pathname, options?.body ? JSON.parse(options.body) : null, options);
    return { ok: true, json: async () => payload };
  };
  bindings.fixtureCommand = command;
  source = source.replace(/^import .+;\n/gm, "").replaceAll("import.meta.url", JSON.stringify(mainUrl.href));
  return runInNewContext(`${source}\n
    runtimeUrl = 'http://127.0.0.1:8000';
    novelSkillPrompt = loadNovelCodexSkills(fileURLToPath(new URL('../', ${JSON.stringify(mainUrl.href)})));
    agentProviderConfig = resolveCodexProvider({provider:'openai'});
    codexAdapter.configure({command:fixtureCommand,apiKey:'test-openai-key'});
    const handlers = new Map();
    registerIpcHandlers({handle:(name,handler)=>handlers.set(name,handler)});
    ({handlers, adapter:codexAdapter});
  `, bindings, { filename: mainUrl.pathname });
}

export function runtimeFixture() {
  const tasks = new Map();
  const completions = [];
  const lookup = [];
  return {
    tasks, completions, lookup,
    async request(path, body) {
      if (path.endsWith('/tasks/lookup')) {
        lookup.push(body.threadIds);
        return {tasks:body.threadIds.map((id)=>tasks.get(id)).filter(Boolean)};
      }
      if (path.endsWith('/creative-workspace')) return {revision:0,tasks:[]};
      if (path.endsWith('/tasks')) {
        const task = {...body,id:'task-'+tasks.size,projectId:'project',purpose:body.operation === 'advisory' ? 'advisory' : 'planner'};
        tasks.set(body.threadId,task);
        return task;
      }
      if (path.endsWith('/complete')) {
        if (Array.from(body.summary).length > 4000) throw new Error('summary too long');
        completions.push(body);
        return {status:'completed'};
      }
      return {};
    },
  };
}

export function rendererEvent() {
  const sender = new EventEmitter();
  sender.isDestroyed = () => false;
  sender.send = () => {};
  return {sender};
}
