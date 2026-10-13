const { app, BrowserWindow } = require("electron");
const path = require("node:path");
const fs = require("node:fs/promises");
app.setPath("userData",path.join(path.dirname(process.argv[2]),"user-data"));
app.whenReady().then(async()=>{
 const window=new BrowserWindow({width:800,height:1000,show:false,webPreferences:{backgroundThrottling:false}});
 await window.loadFile(process.argv[2]);
 let checking=false;
 const timer=setInterval(async()=>{if(checking)return;checking=true;try{
  const result=await window.webContents.executeJavaScript("window.testResult");
  if(result){clearInterval(timer);if(result.passed&&process.env.NOVEL_QUALITY_PREVIEW)await fs.writeFile(process.env.NOVEL_QUALITY_PREVIEW,(await window.webContents.capturePage()).toPNG());console.log(JSON.stringify(result));app.exit(result.passed?0:1);}
 }finally{checking=false;}},20);
 setTimeout(()=>app.exit(1),20000);
});
