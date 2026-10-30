import assert from 'node:assert/strict';
import fs from 'node:fs';
import {spawn} from 'node:child_process';
import {CDP, until} from '../Desktop_App/tests/ui-tracer.mjs';
const app='/source/Studio/Desktop_App';
const out=process.env.STUDIO_SMOKE_OUT || '/runtime/ui-smoke';
fs.mkdirSync(out,{recursive:true});
const children=[];
function launch(bin,args,name){
 const log=fs.openSync(`${out}/${name}.log`,'w');
 const child=spawn(bin,args,{cwd:app,stdio:['ignore',log,log],env:{...process.env,XDG_CACHE_HOME:'/tmp/ui-cache',npm_config_cache:'/tmp/ui-npm'}});
 children.push(child);return child;
}
let cdp;
try {
 launch(`${app}/node_modules/.bin/vite`,['--host','127.0.0.1'],'vite');
 await until('vite ready',async()=>{try{return (await fetch('http://127.0.0.1:1420')).ok}catch{return false}},30000);
 launch(`${app}/node_modules/.bin/electron`,['.','--disable-gpu','--remote-debugging-address=127.0.0.1','--remote-debugging-port=9333'],'electron');
 const target=await until('Electron renderer',async()=>{try{return (await (await fetch('http://127.0.0.1:9333/json/list')).json()).find(x=>x.type==='page')}catch{return null}},30000);
 cdp=await CDP.open(target.webSocketDebuggerUrl);
 const ui=await until('Studio visible',async()=>{const v=await cdp.evaluate('({title:document.title,text:document.body.innerText,buttons:document.querySelectorAll("button").length})');return v.text?.length>100&&v.buttons>0?v:null},30000);
 assert.match(ui.text,/Studio|Workbench|Project/i);
 fs.writeFileSync(`${out}/ui.json`,JSON.stringify(ui,null,2));
 const screenshot=await cdp.call('Page.captureScreenshot',{format:'png'});
 fs.writeFileSync(`${out}/ui.png`,Buffer.from(screenshot.data,'base64'));
 // Connects to the real app bridge but never creates projects or calls providers.
 const bridge=await cdp.evaluate('({bridge:typeof window.workbench,ready:document.readyState})');
 assert.equal(bridge.bridge,'object');
 const processes=[];
 for(const pid of fs.readdirSync('/proc').filter(x=>/^\d+$/.test(x))){
   try {
     const args=fs.readFileSync(`/proc/${pid}/cmdline`,'utf8').replaceAll('\0',' ');
     if(!args.includes('/electron/dist/electron'))continue;
     const status=fs.readFileSync(`/proc/${pid}/status`,'utf8');
     const fields=Object.fromEntries(status.split('\n').filter(x=>/^(Uid|CapEff|CapPrm|Seccomp|Seccomp_filters|NoNewPrivs):/.test(x)).map(x=>x.split(/:\s*/)));
     processes.push({pid:Number(pid),args,...fields});
   }catch{}
 }
 const renderers=processes.filter(p=>p.args.includes('--type=renderer'));
 assert.ok(renderers.length>0,'sandbox renderer process exists');
 for(const p of renderers){assert.equal(p.CapEff,'0000000000000000');assert.equal(p.Seccomp,'2');assert.ok(Number(p.Seccomp_filters)>=2);assert.ok(!p.args.includes('--no-sandbox'));}
 fs.writeFileSync(`${out}/processes.json`,JSON.stringify(processes,null,2));
 console.log(JSON.stringify({ok:true,ui:{title:ui.title,buttons:ui.buttons},bridge,renderers:renderers.length,out}));
} finally {
 cdp?.close();for(const child of children.reverse()) child.kill('SIGTERM');
 await new Promise(r=>setTimeout(r,500));
 for(const child of children)if(child.exitCode===null)child.kill('SIGKILL');
}
