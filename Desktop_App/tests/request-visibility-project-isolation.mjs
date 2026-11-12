import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from './helpers/ui.mjs';
import { appRoot as app, prepareOutput } from './helpers/runtime.mjs';

const out = prepareOutput('request-visibility-project-isolation');
const rootA = `${out}/project-a`;
const rootB = `${out}/project-b`;
const promptA = 'Disposable W1-26 Project A private prompt, no provider call.';
const command = [process.execPath, ...process.execArgv, ...process.argv.slice(1)].map(arg => JSON.stringify(arg)).join(' ');
for (const [root, label] of [[rootA, 'A'], [rootB, 'B']]) {
  fs.mkdirSync(root, { recursive: true });
  fs.writeFileSync(`${root}/sentinel.txt`, `Disposable offline W1-26 project ${label}\n`);
}

const result = {
  check: 'request_visibility_project_isolation',
  expected: "After delayed work starts in A and the user switches to B, B never displays A's prompt, progress, or approval.",
  actual: null,
  command,
  readback: null,
  hostCalls: null,
  network: null,
  exit: 1,
};
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const viteLog = fs.openSync(`${out}/vite.log`, 'w');
const vite = spawn(`${app}/node_modules/.bin/vite`, ['--host', '127.0.0.1', '--port', '1420', '--strictPort'], {
  cwd: app, stdio: ['ignore', viteLog, viteLog], env: process.env,
});
const electronLog = fs.openSync(`${out}/electron.log`, 'w');
let electron;
let main;
let cdp;
const ev = source => cdp.evaluate(source);
const visibleButton = label => `[...document.querySelectorAll('button')].find(e => e.textContent?.trim() === ${JSON.stringify(label)} && e.getClientRects().length)`;
async function click(source) {
  const point = await ev(`(() => { const e=${source}; if(!e || e.disabled || !e.getClientRects().length) throw Error('Missing visible enabled target'); const r=e.getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()`);
  await cdp.call('Input.dispatchMouseEvent', { type: 'mousePressed', ...point, button: 'left', clickCount: 1 });
  await cdp.call('Input.dispatchMouseEvent', { type: 'mouseReleased', ...point, button: 'left', clickCount: 1 });
  await sleep(180);
}
async function openAnotherProject() {
  await main.evaluate(`process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').BrowserWindow.getAllWindows()[0].webContents.send('menu','open-project')`);
}

try {
  await until('Vite dev server', async () => { try { return (await fetch('http://127.0.0.1:1420/')).ok; } catch { return false; } });
  electron = spawn(`${app}/node_modules/.bin/electron`, ['.', '--no-sandbox', '--disable-gpu', '--inspect=127.0.0.1:9820', '--remote-debugging-port=9821'], {
    cwd: app,
    stdio: ['ignore', electronLog, electronLog],
    env: {
      ...process.env,
      WORKBENCH_DEV: '1',
      WORKBENCH_HOST_BIN: process.env.WORKBENCH_HOST_BIN ?? '/runtime/target/debug/workbench-host',
      XDG_CONFIG_HOME: `${out}/xdg-config`,
      XDG_CACHE_HOME: `${out}/xdg-cache`,
      XDG_DATA_HOME: `${out}/xdg-data`,
    },
  });
  const mt = await until('main target', async () => { try { return (await (await fetch('http://127.0.0.1:9820/json/list')).json())[0]; } catch { return null; } });
  main = await CDP.open(mt.webSocketDebuggerUrl);
  await main.call('Runtime.enable');
  const rt = await until('renderer target', async () => { try { return (await (await fetch('http://127.0.0.1:9821/json/list')).json()).find(target => target.type === 'page'); } catch { return null; } });
  cdp = await CDP.open(rt.webSocketDebuggerUrl);
  await until('ready', () => ev(`document.querySelector('#status-left')?.textContent?.startsWith('Ready —')`));

  await main.evaluate(`(() => {
    const {BrowserWindow,dialog,ipcMain,session}=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron');
    BrowserWindow.getAllWindows()[0].setSize(1600,1200);
    globalThis.__w126={opens:0,calls:[],blocked:[],resolve:null,settled:false};
    dialog.showOpenDialog=async()=>({canceled:false,filePaths:[++globalThis.__w126.opens===1?${JSON.stringify(rootA)}:${JSON.stringify(rootB)}]});
    dialog.showMessageBox=async()=>({response:1,checkboxChecked:false});
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd==='workflow_generate_script'){
        globalThis.__w126.calls.push({cmd,payload});
        return await new Promise(resolve=>{globalThis.__w126.resolve=()=>{globalThis.__w126.settled=true;resolve({cancelled:true});};});
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w126.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project A applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(rootA))}`));
  await ev(`import('/src/settingsPanel.ts').then(m=>m.setSelectedChatModelFromSelect('offline-fixture::no-submit-model'))`);
  if (!await ev(`!!document.querySelector('.workflow-script-draft')`)) await click(visibleButton('Create script'));
  await until('script draft', () => ev(`document.querySelector('.workflow-script-draft')?.getAttribute('aria-busy') === 'false'`));
  await ev(`(() => { const n=document.querySelector('.workflow-script-draft [name="notes"]'); n.value=${JSON.stringify(promptA)}; n.dispatchEvent(new Event('input',{bubbles:true})); })()`);
  await click(visibleButton('Generate script from notes'));
  await until('editable request prompt', () => ev(`!!document.querySelector('dialog[data-generation-prompt][open]')`));
  await click(visibleButton('Prepare request'));
  await until('A request held', () => main.evaluate(`globalThis.__w126.calls.length===1 && typeof globalThis.__w126.resolve==='function' && !globalThis.__w126.settled`));
  await until('A local and global progress visible', () => ev(`document.querySelector('[data-request-outcome="working"]')?.textContent?.includes(${JSON.stringify(promptA)}) && document.querySelector('[data-global-request-indicator]')?.textContent?.includes('Working')`));

  await openAnotherProject();
  await until('project B applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(rootB))}`));
  await until('project B workflow rendered', () => ev(`!!document.querySelector('.workflow-main')`));
  await sleep(350);

  result.readback = await ev(`(() => {
    const body=document.body.innerText;
    const globals=[...document.querySelectorAll('[data-global-request-indicator]')].map(e=>e.textContent?.trim());
    const receipts=[...document.querySelectorAll('[data-request-receipt]')].map(e=>({outcome:e.getAttribute('data-request-outcome'),text:e.textContent?.trim()}));
    const approvals=[...document.querySelectorAll('dialog[open]')].map(e=>e.textContent?.trim());
    return {
      projectName:document.querySelector('#project-name')?.textContent?.trim()??null,
      promptAVisible:body.includes(${JSON.stringify(promptA)}),
      progressFromAVisible:body.includes('Generate script from notes') && (globals.some(x=>x?.includes('Working')) || receipts.some(x=>x.text?.includes('Working'))),
      approvalFromAVisible:approvals.some(x=>x?.includes(${JSON.stringify(promptA)}) || x?.includes('Authorize exact operation')),
      globalIndicators:globals,
      requestReceipts:receipts,
      openDialogs:approvals,
      scroll:{x:scrollX,y:scrollY},
      viewport:{width:innerWidth,height:innerHeight},
    };
  })()`);
  result.hostCalls = await main.evaluate('globalThis.__w126.calls');
  result.network = {
    blockedRemoteRequests: await main.evaluate('globalThis.__w126.blocked'),
    hostSettled: await main.evaluate('globalThis.__w126.settled'),
    openDialogCount: await main.evaluate('globalThis.__w126.opens'),
  };
  assert.equal(result.readback.projectName, path.basename(rootB));
  assert.equal(result.readback.promptAVisible, false);
  assert.equal(result.readback.progressFromAVisible, false);
  assert.equal(result.readback.approvalFromAVisible, false);
  assert.deepEqual(result.readback.globalIndicators, []);
  assert.deepEqual(result.readback.requestReceipts, []);
  assert.deepEqual(result.readback.openDialogs, []);
  assert.equal(result.hostCalls.length, 1);
  assert.equal(result.network.hostSettled, false);
  assert.equal(result.network.openDialogCount, 2);
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/project-b-isolated.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: Project B showed no Project A prompt, progress, approval, global indicator, or local receipt while A's one host request remained unresolved.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = {body:await ev('document.body.innerText'),calls:await main.evaluate('globalThis.__w126?.calls'),blocked:await main.evaluate('globalThis.__w126?.blocked'),settled:await main.evaluate('globalThis.__w126?.settled'),opens:await main.evaluate('globalThis.__w126?.opens')}; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`globalThis.__w126?.resolve?.(); {const e=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron'); for(const w of e.BrowserWindow.getAllWindows())w.destroy(); e.app.exit(0);}`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/project-b-isolated.png`},null,2));
}
process.exitCode = result.exit;
