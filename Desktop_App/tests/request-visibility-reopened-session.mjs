import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from './helpers/ui.mjs';
import { appRoot as app, prepareOutput } from './helpers/runtime.mjs';

const out = prepareOutput('request-visibility-reopened-session');
const rootA = `${out}/project-a`;
const rootB = `${out}/project-b`;
const pendingAction = 'Old W1-27 session approval must not become live.\n[[workbench_approve:W127OLD]]';
const command = [process.execPath, ...process.execArgv, ...process.argv.slice(1)].map(arg => JSON.stringify(arg)).join(' ');
for (const [root, label] of [[rootA, 'A'], [rootB, 'B']]) {
  fs.mkdirSync(root, { recursive: true });
  fs.writeFileSync(`${root}/sentinel.txt`, `Disposable offline W1-27 project ${label}\n`);
}
const result = { check:'request_visibility_reopened_session', expected:"After A's delayed preparation is retired by closing A and reopening the same root, releasing the old result leaves it historical and never presents its approval as live.", actual:null, command, readback:null, hostCalls:null, network:null, exit:1 };
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const viteLog = fs.openSync(`${out}/vite.log`, 'w');
const vite = spawn(`${app}/node_modules/.bin/vite`, ['--host','127.0.0.1','--port','1420','--strictPort'], {cwd:app,stdio:['ignore',viteLog,viteLog],env:process.env});
const electronLog = fs.openSync(`${out}/electron.log`, 'w');
let electron, main, cdp;
const ev = source => cdp.evaluate(source);
const visibleButton = label => `[...document.querySelectorAll('button')].find(e => e.textContent?.trim() === ${JSON.stringify(label)} && e.getClientRects().length)`;
async function click(source) {
  const point=await ev(`(()=>{const e=${source};if(!e||e.disabled||!e.getClientRects().length)throw Error('Missing visible enabled target');const r=e.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2}})()`);
  await cdp.call('Input.dispatchMouseEvent',{type:'mousePressed',...point,button:'left',clickCount:1});
  await cdp.call('Input.dispatchMouseEvent',{type:'mouseReleased',...point,button:'left',clickCount:1});
  await sleep(180);
}
async function openAnotherProject(){await main.evaluate(`process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').BrowserWindow.getAllWindows()[0].webContents.send('menu','open-project')`);}
async function chooseProfile(){
  await click(`[...document.querySelectorAll('.studio-surface-nav button')].find(e => [...e.children].some(c => c.textContent?.trim() === 'Profile'))`);
  await until('Profile stage',()=>ev(`document.querySelector('.workflow-main .workflow-project')?.getClientRects().length>0`));
}
try {
  await until('Vite dev server',async()=>{try{return(await fetch('http://127.0.0.1:1420/')).ok}catch{return false}});
  electron=spawn(`${app}/node_modules/.bin/electron`,['.','--no-sandbox','--disable-gpu','--inspect=127.0.0.1:9830','--remote-debugging-port=9831'],{cwd:app,stdio:['ignore',electronLog,electronLog],env:{...process.env,WORKBENCH_DEV:'1',WORKBENCH_HOST_BIN:process.env.WORKBENCH_HOST_BIN ?? '/runtime/target/debug/workbench-host',XDG_CONFIG_HOME:`${out}/xdg-config`,XDG_CACHE_HOME:`${out}/xdg-cache`,XDG_DATA_HOME:`${out}/xdg-data`}});
  const mt=await until('main target',async()=>{try{return(await(await fetch('http://127.0.0.1:9830/json/list')).json())[0]}catch{return null}}); main=await CDP.open(mt.webSocketDebuggerUrl); await main.call('Runtime.enable');
  const rt=await until('renderer target',async()=>{try{return(await(await fetch('http://127.0.0.1:9831/json/list')).json()).find(t=>t.type==='page')}catch{return null}}); cdp=await CDP.open(rt.webSocketDebuggerUrl);
  await until('ready',()=>ev(`document.querySelector('#status-left')?.textContent?.startsWith('Ready —')`));
  await main.evaluate(`(()=>{const{BrowserWindow,dialog,ipcMain,session}=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron');BrowserWindow.getAllWindows()[0].setSize(1600,1200);globalThis.__w127={opens:0,calls:[],blocked:[],release:null,settled:false,cancels:[]};dialog.showOpenDialog=async()=>({canceled:false,filePaths:[++globalThis.__w127.opens===1?${JSON.stringify(rootA)}:globalThis.__w127.opens===2?${JSON.stringify(rootB)}:${JSON.stringify(rootA)}]});dialog.showMessageBox=async()=>({response:1,checkboxChecked:false});const original=ipcMain._invokeHandlers.get('host:invoke');ipcMain.removeHandler('host:invoke');ipcMain.handle('host:invoke',async(...args)=>{const cmd=args[1],payload=args[2];if(cmd==='workflow_save_profile'){globalThis.__w127.calls.push({cmd,payload});await new Promise(resolve=>{globalThis.__w127.release=resolve});const answer=await original(...args);answer.pendingAction=${JSON.stringify(pendingAction)};globalThis.__w127.settled=true;return answer}if(cmd==='director_cancel'){globalThis.__w127.cancels.push(payload);return{cancelled:true}}return await original(...args)});session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');if(remote)globalThis.__w127.blocked.push(request.url);callback({cancel:remote})})})()`);
  await click(`document.querySelector('#home-open-project')`);
  await until('project A applied',()=>ev(`document.querySelector('#project-name')?.textContent===${JSON.stringify(path.basename(rootA))}`));
  if(!await ev(`!!document.querySelector('.workflow-main')`)) await click(visibleButton('Enter Studio'));
  await chooseProfile();
  await click(`[...document.querySelectorAll('.workflow-profile-form button')].find(e=>e.textContent?.trim()==='Save profile')`);
  await until('old A request held',()=>main.evaluate(`globalThis.__w127.calls.length===1&&typeof globalThis.__w127.release==='function'&&!globalThis.__w127.settled`));
  await until('old A working visible',()=>ev(`document.querySelector('[data-request-outcome="working"]')?.textContent?.includes('Saving production profile')`));
  await openAnotherProject();
  await until('project B applied',()=>ev(`document.querySelector('#project-name')?.textContent===${JSON.stringify(path.basename(rootB))}`));
  await until('project B workflow rendered',()=>ev(`!!document.querySelector('.workflow-main')`));
  await sleep(600);
  await openAnotherProject();
  await until('project A reopened',()=>ev(`document.querySelector('#project-name')?.textContent===${JSON.stringify(path.basename(rootA))}`));
  await until('reopened workflow rendered',()=>ev(`!!document.querySelector('.workflow-main')`));
  await chooseProfile();
  await main.evaluate(`globalThis.__w127.release()`);
  await until('old result settled',()=>main.evaluate(`globalThis.__w127.settled`));
  await sleep(500);
  result.readback=await ev(`(()=>{const body=document.body.innerText;const receipts=[...document.querySelectorAll('[data-request-receipt]')].map(e=>({outcome:e.dataset.requestOutcome,text:e.textContent?.trim()}));const dialogs=[...document.querySelectorAll('dialog[open]')].map(e=>e.textContent?.trim());return{projectName:document.querySelector('#project-name')?.textContent?.trim()??null,receipts,openDialogs:dialogs,oldApprovalTextVisible:body.includes('Old W1-27 session approval must not become live.'),livePendingReceipt:receipts.some(r=>r.outcome==='pending'),approvalModalVisible:Boolean(document.querySelector('dialog[data-operation-approval][open]')),globalIndicator:document.querySelector('[data-global-request-indicator]')?.textContent?.trim()??null,scroll:{x:scrollX,y:scrollY},viewport:{width:innerWidth,height:innerHeight}}})()`);
  result.hostCalls=await main.evaluate('globalThis.__w127.calls');
  result.network={blockedRemoteRequests:await main.evaluate('globalThis.__w127.blocked'),hostSettled:await main.evaluate('globalThis.__w127.settled'),cancelCalls:await main.evaluate('globalThis.__w127.cancels'),openDialogCount:await main.evaluate('globalThis.__w127.opens')};
  assert.equal(result.readback.projectName,path.basename(rootA));
  assert.equal(result.readback.livePendingReceipt,false);
  assert.equal(result.readback.approvalModalVisible,false);
  assert.equal(result.readback.oldApprovalTextVisible,false);
  assert.deepEqual(result.readback.openDialogs,[]);
  assert.equal(result.readback.globalIndicator,null);
  assert.equal(result.hostCalls.length,1);
  assert.equal(result.network.hostSettled,true);
  assert.equal(result.network.openDialogCount,3);
  assert.deepEqual(result.network.blockedRemoteRequests,[]);
  const shot=await cdp.call('Page.captureScreenshot',{format:'png'});fs.writeFileSync(`${out}/reopened-session-old-result-retired.png`,Buffer.from(shot.data,'base64'));
  result.actual='PASS: after closing A, reopening the same root, and releasing the old preparation, no live pending receipt, approval modal, global activity, or old approval text appeared.'; result.exit=0;
} catch(error){result.error=error.stack;try{result.diagnostic={body:await ev('document.body.innerText'),calls:await main.evaluate('globalThis.__w127?.calls'),blocked:await main.evaluate('globalThis.__w127?.blocked'),settled:await main.evaluate('globalThis.__w127?.settled'),cancels:await main.evaluate('globalThis.__w127?.cancels'),opens:await main.evaluate('globalThis.__w127?.opens')}}catch{}}
finally{fs.writeFileSync(`${out}/result.json`,JSON.stringify(result,null,2));if(main){try{await main.evaluate(`{const e=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron');for(const w of e.BrowserWindow.getAllWindows())w.destroy();e.app.exit(0)}`)}catch{}}cdp?.close();main?.close();if(electron?.exitCode===null)electron.kill('SIGTERM');if(vite.exitCode===null)vite.kill('SIGTERM');console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/reopened-session-old-result-retired.png`},null,2))}
process.exitCode=result.exit;
