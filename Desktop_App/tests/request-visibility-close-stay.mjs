import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from '/source/Studio/Desktop_App/tests/ui-tracer.mjs';

const app = '/source/Studio/Desktop_App';
const out = `${app}/evidence/w1-25/runtime-366`;
const root = `${out}/disposable-project`;
const command = "docker exec studio-dev-workbench-1 bash -lc 'cd /source/Studio/Desktop_App && xvfb-run -a node tests/request-visibility-close-stay.mjs'";
const activeStatus = 'Submitted · Working — Generate script from notes';
const activeSummary = '1 active request · Working — Generate script from notes';
fs.rmSync(out, { recursive: true, force: true });
for (const dir of ['/runtime/w1-25-config', '/runtime/w1-25-cache', '/runtime/w1-25-data']) fs.rmSync(dir, { recursive: true, force: true });
fs.mkdirSync(root, { recursive: true });
fs.writeFileSync(`${root}/sentinel.txt`, 'Disposable offline W1-25 project\n');

const result = {
  check: 'request_visibility_close_stay',
  expected: 'Choosing Keep Studio open in the active-work close confirmation keeps the window open and preserves the same visible active status.',
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

try {
  await until('Vite dev server', async () => { try { return (await fetch('http://127.0.0.1:1420/')).ok; } catch { return false; } });
  electron = spawn(`${app}/node_modules/.bin/electron`, ['.', '--no-sandbox', '--disable-gpu', '--inspect=127.0.0.1:9818', '--remote-debugging-port=9819'], {
    cwd: app,
    stdio: ['ignore', electronLog, electronLog],
    env: {
      ...process.env,
      WORKBENCH_DEV: '1',
      WORKBENCH_HOST_BIN: '/runtime/target/debug/workbench-host',
      XDG_CONFIG_HOME: '/runtime/w1-25-config',
      XDG_CACHE_HOME: '/runtime/w1-25-cache',
      XDG_DATA_HOME: '/runtime/w1-25-data',
    },
  });
  const mt = await until('main target', async () => { try { return (await (await fetch('http://127.0.0.1:9818/json/list')).json())[0]; } catch { return null; } });
  main = await CDP.open(mt.webSocketDebuggerUrl);
  await main.call('Runtime.enable');
  const rt = await until('renderer target', async () => { try { return (await (await fetch('http://127.0.0.1:9819/json/list')).json()).find(target => target.type === 'page'); } catch { return null; } });
  cdp = await CDP.open(rt.webSocketDebuggerUrl);
  await until('ready', () => ev(`document.querySelector('#status-left')?.textContent?.startsWith('Ready —')`));

  await main.evaluate(`(() => {
    const {BrowserWindow,dialog,ipcMain,session}=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron');
    const win=BrowserWindow.getAllWindows()[0];
    win.setSize(1600,1200);
    dialog.showOpenDialog=async()=>({canceled:false,filePaths:[${JSON.stringify(root)}]});
    globalThis.__w125={calls:[],blocked:[],resolve:null,settled:false,closed:false};
    win.once('closed',()=>{globalThis.__w125.closed=true;});
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd==='workflow_generate_script'){
        globalThis.__w125.calls.push({cmd,payload});
        return await new Promise(resolve=>{globalThis.__w125.resolve=()=>{globalThis.__w125.settled=true;resolve({cancelled:true});};});
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w125.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await ev(`import('/src/settingsPanel.ts').then(m=>m.setSelectedChatModelFromSelect('offline-fixture::no-submit-model'))`);
  if (!await ev(`!!document.querySelector('.workflow-script-draft')`)) await click(visibleButton('Create script'));
  await until('script draft', () => ev(`document.querySelector('.workflow-script-draft')?.getAttribute('aria-busy') === 'false'`));
  await ev(`(() => { const n=document.querySelector('.workflow-script-draft [name="notes"]'); n.value='Disposable W1-25 unresolved close-warning prompt, no provider call.'; n.dispatchEvent(new Event('input',{bubbles:true})); })()`);
  await click(visibleButton('Generate script from notes'));
  await until('editable request prompt', () => ev(`!!document.querySelector('dialog[data-generation-prompt][open]')`));
  await click(visibleButton('Prepare request'));
  await until('native request submitted and unresolved', () => main.evaluate(`globalThis.__w125.calls.length === 1 && typeof globalThis.__w125.resolve === 'function' && !globalThis.__w125.settled`));
  await until('active status', () => ev(`document.querySelector('[data-request-outcome="working"] strong')?.textContent === ${JSON.stringify(activeStatus)}`));

  await main.evaluate(`process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').BrowserWindow.getAllWindows()[0].close()`);
  await until('visible active-work close confirmation', () => ev(`!!document.querySelector('dialog[data-active-work-close][open]')`));
  await click(visibleButton('Keep Studio open'));
  await until('active-work close confirmation dismissed', () => ev(`!document.querySelector('dialog[data-active-work-close]')`));
  await sleep(250);

  const readback = await ev(`(() => {
    const local=document.querySelector('[data-request-outcome="working"]');
    const global=document.querySelector('[data-global-request-indicator]');
    const pending=[...document.querySelectorAll('button')].find(e=>e.textContent?.trim()==='Generating script…');
    const visibleInViewport=e=>{if(!e?.getClientRects().length)return false;const r=e.getBoundingClientRect(),s=getComputedStyle(e);return r.top>=0&&r.left>=0&&r.bottom<=innerHeight&&r.right<=innerWidth&&s.visibility==='visible'&&s.display!=='none'&&Number(s.opacity)>0;};
    return {
      closeDialogPresent:!!document.querySelector('dialog[data-active-work-close]'),
      localStatus:local?.querySelector('strong')?.textContent?.trim()??null,
      localVisibleInInitialViewport:visibleInViewport(local)&&scrollX===0&&scrollY===0,
      globalIndicator:global?.textContent?.trim()??null,
      globalVisibleInInitialViewport:visibleInViewport(global)&&scrollX===0&&scrollY===0,
      pendingControl:{label:pending?.textContent?.trim()??null,disabled:pending?.disabled??null,ariaBusy:pending?.getAttribute('aria-busy')??null},
      focusedLabel:document.activeElement?.textContent?.trim()??null,
    };
  })()`);
  result.readback = {...readback, windowClosed: await main.evaluate('globalThis.__w125.closed')};
  result.hostCalls = await main.evaluate('globalThis.__w125.calls');
  result.network = { blockedRemoteRequests: await main.evaluate('globalThis.__w125.blocked'), hostSettled: await main.evaluate('globalThis.__w125.settled') };
  assert.equal(readback.closeDialogPresent, false);
  assert.equal(readback.localStatus, activeStatus);
  assert.equal(readback.localVisibleInInitialViewport, true);
  assert.equal(readback.globalIndicator, activeSummary);
  assert.equal(readback.globalVisibleInInitialViewport, true);
  assert.deepEqual(readback.pendingControl, {label:'Generating script…',disabled:true,ariaBusy:'true'});
  assert.equal(result.readback.windowClosed, false);
  assert.equal(result.hostCalls.length, 1);
  assert.equal(result.network.hostSettled, false);
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/active-work-close-stay.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: after choosing Keep Studio open, the window stayed open and the same local status "${readback.localStatus}" plus global status "${readback.globalIndicator}" remained visible while the request stayed unresolved.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), calls: await main.evaluate('globalThis.__w125?.calls'), blocked: await main.evaluate('globalThis.__w125?.blocked'), settled: await main.evaluate('globalThis.__w125?.settled'), closed: await main.evaluate('globalThis.__w125?.closed') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`{const e=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron'); for(const w of e.BrowserWindow.getAllWindows())w.destroy(); e.app.exit(0);}`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/active-work-close-stay.png`},null,2));
}
process.exitCode = result.exit;
