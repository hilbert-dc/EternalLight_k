import React, { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import { RuntimeConnectionStatus } from "../../../src/RuntimeConnectionStatus";
import type { RuntimeStatus } from "../../../src/types";
import "../../../src/styles.css";

const tick = () => new Promise(resolve => setTimeout(resolve,20));
const check = (value,message) => { if (!value) throw Error(message); };
const until = async (predicate,message) => { for (let index=0;index<200 && !predicate();index+=1) await tick(); check(predicate(),message); };
let update, reply, retries = 0;
function Workspace() {
  const [status,setStatus] = useState<RuntimeStatus>({state:"offline",mode:"local",error:"fixture exit"});
  update = setStatus;
  return <><textarea aria-label="未保存正文" defaultValue="我的未保存正文" /><RuntimeConnectionStatus status={status} onStatus={setStatus} retryStartup={() => {
    retries += 1; return new Promise(resolve => { reply = resolve; });
  }} /></>;
}
(async () => {
  let root;
  try {
    root = createRoot(document.getElementById("root")); root.render(<StrictMode><Workspace /></StrictMode>);
    await until(() => document.querySelector("button"),"Recovery button missing");
    const editor = document.querySelector<HTMLTextAreaElement>("textarea");
    editor.focus(); editor.setSelectionRange(2,5);
    const reconnect = document.querySelector<HTMLButtonElement>("button"); reconnect.click(); reconnect.click();
    await tick(); check(retries === 1 && !document.querySelector("button"),"Retry was not single-flight under StrictMode");
    update({state:"slow",mode:"local",error:null}); await tick();
    check(document.querySelector('[role="status"]').textContent.includes("仍在等待") && !document.querySelector("button"),"Slow live service was presented as retryable failure");
    update({state:"online",mode:"local",error:null}); await tick();
    reply({state:"offline",mode:"local",error:"late reply from earlier request"}); await tick();
    check(document.querySelector('[role="status"]').textContent.includes("本地在线") && !document.querySelector("button"),"Late retry overwrote a newer online status");
    check(document.querySelector("textarea") === editor && editor.value === "我的未保存正文" && editor.selectionStart === 2 && editor.selectionEnd === 5,"Connection updates replaced draft or selection");
    window.testResult = {passed:true,retries,preservedOnline:true,preservedEditor:true,preservedSelection:true};
  } catch (error) { window.testResult = {passed:false,error:String(error),dom:document.body.innerHTML}; }
  finally { root?.unmount(); }
})();
