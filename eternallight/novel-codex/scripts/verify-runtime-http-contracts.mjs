import ts from "typescript";
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

export function verifyHttpContracts({ source, inventory, validator, generatedNames, api = "runtime", routeSources }) {
 const ast=ts.createSourceFile("types.ts",source,ts.ScriptTarget.Latest,true);
 let runtime;
 const visit=node=>{if(ts.isPropertySignature(node)&&node.name.getText(ast)===api)runtime=node.type;if(ts.isInterfaceDeclaration(node)&&node.name.text===api)runtime=node;ts.forEachChild(node,visit);};visit(ast);
 if(!runtime||!ts.isTypeLiteralNode(runtime)&&!ts.isInterfaceDeclaration(runtime))throw Error("Missing Renderer runtime API");
 const declarations=new Map(runtime.members.map(member=>[member.name.getText(ast),member]));
 const entries=new Map();
 for(const entry of inventory.methods){if(entries.has(entry.method))throw Error(`Duplicate method ${entry.method}`);entries.set(entry.method,entry);}
 for(const [name,member] of declarations){
  const entry=entries.get(name);if(!entry)throw Error(`Runtime method ${name} has no wire ownership`);
  if(entry.kind==="host"){if(!entry.reason)throw Error(`Host method ${name} needs an explicit ownership reason`);continue;}
  if(entry.kind!=="http"||!entry.response||!entry.routes?.length)throw Error(`HTTP method ${name} lacks a response/route`);
  if(!(ts.isMethodSignature(member)?member.type?.getText(ast):ts.isFunctionTypeNode(member.type)?member.type.type.getText(ast):"")?.includes(`generated/responses/${entry.response}`))throw Error(`HTTP method ${name} does not consume its generated response`);
  if(!generatedNames.has(entry.response))throw Error(`Unexported HTTP contract ${entry.response}`);
  if(!new RegExp(`\\b${entry.response}\\b`).test(validator))throw Error(`No production validator for ${entry.response}`);
  for(const route of entry.routes){
   if(!/^(GET|POST|PUT|DELETE|PATCH)$/.test(route.verb)||!route.path.startsWith('/api/'))throw Error(`Invalid route ${name}`);
   if(routeSources){const registered=[...routeSources.matchAll(/\.route\(\s*"([^"]+)"/g)].map(x=>x[1]);const concrete=route.path.split('/');
    if(!registered.some(template=>{const parts=template.split('/');return parts.length===concrete.length&&parts.every((p,i)=>p.startsWith('{')||p===concrete[i]);}))throw Error(`No Runtime route for ${name}: ${route.path}`);
   }
  }
 }
 for(const name of entries.keys())if(!declarations.has(name))throw Error(`Stale HTTP method ${name}`);
 return declarations.size;
}

export function verifyContentFields(sources, inventory) {
 const allowed=new Map(inventory.fields.map(item=>[`${item.type}.${item.field}`,item.reason]));
 const found=new Set();
 for(const source of sources){
  for(const [,name,body] of source.matchAll(/pub struct (\w+)\s*\{([\s\S]*?)\n\}/g)){
   for(const [,field] of body.matchAll(/pub (?:r#)?(\w+):[^\n]*serde_json::Value[^\n]*/g)){
    const key=`${name}.${field}`;if(!allowed.get(key))throw Error(`Unreviewed dynamic HTTP content ${key}`);found.add(key);
   }
  }
 }
 for(const key of allowed.keys())if(!found.has(key))throw Error(`Stale dynamic HTTP content exception ${key}`);
 return found.size;
}

if(process.argv[1]===fileURLToPath(import.meta.url)){
 const root=fileURLToPath(new URL('../',import.meta.url));const read=name=>readFileSync(path.join(root,name),'utf8');
 const routeSources=readdirSync(path.join(root,'native/story-runtime/src')).filter(n=>n.endsWith('.rs')).map(n=>read(`native/story-runtime/src/${n}`)).join('\n');
 let count=0;
 for(const [file,source,api] of [['shared/runtime-http-contracts.json','src/types.ts','runtime'],['shared/multimedia-http-contracts.json','shared/multimedia-types.ts','MultimediaApi']]){
  const inventory=JSON.parse(read(file));
  const names=new Set(inventory.methods.filter(x=>x.response).map(x=>{read(`shared/generated/responses/${x.response}.ts`);return x.response;}));
  count+=verifyHttpContracts({source:read(source),api,inventory,validator:read('native/story-runtime/src/response_contract.rs'),generatedNames:names,routeSources});
 }
 const responseSources=readdirSync(path.join(root,'native/story-runtime/src/renderer_responses')).filter(n=>n.endsWith('.rs')).map(n=>read(`native/story-runtime/src/renderer_responses/${n}`));
 verifyContentFields(responseSources,JSON.parse(read('shared/runtime-http-content-fields.json')));
 console.log(`Verified ${count} Renderer Runtime/media method contracts.`);
}
