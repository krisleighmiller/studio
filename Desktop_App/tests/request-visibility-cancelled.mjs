import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from './helpers/ui.mjs';
import { appRoot as app, prepareOutput } from './helpers/runtime.mjs';

const out = prepareOutput('request-visibility-cancelled');
const root = `${out}/disposable-project`;
const command = [process.execPath, ...process.execArgv, ...process.argv.slice(1)].map(arg => JSON.stringify(arg)).join(' ');
const expectedStatus = 'Submitted · Cancelled — Generate script from notes';
const expectedDetail = 'Native request cancelled; your draft was retained.';
fs.mkdirSync(root, { recursive: true });
fs.writeFileSync(`${root}/sentinel.txt`, 'Disposable offline W1-23 project\n');

const result = {
  check: 'request_visibility_cancelled',
  expected: 'After one actually submitted native Script request returns cancelled, the visible terminal status says Cancelled and never Success or Error.',
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
  electron = spawn(`${app}/node_modules/.bin/electron`, ['.', '--no-sandbox', '--disable-gpu', '--inspect=127.0.0.1:9816', '--remote-debugging-port=9817'], {
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
  const mt = await until('main target', async () => { try { return (await (await fetch('http://127.0.0.1:9816/json/list')).json())[0]; } catch { return null; } });
  main = await CDP.open(mt.webSocketDebuggerUrl);
  await main.call('Runtime.enable');
  const rt = await until('renderer target', async () => { try { return (await (await fetch('http://127.0.0.1:9817/json/list')).json()).find(target => target.type === 'page'); } catch { return null; } });
  cdp = await CDP.open(rt.webSocketDebuggerUrl);
  await until('ready', () => ev(`document.querySelector('#status-left')?.textContent?.startsWith('Ready —')`));

  await main.evaluate(`(() => {
    const {BrowserWindow,dialog,ipcMain,session}=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron');
    BrowserWindow.getAllWindows()[0].setSize(1600,1200);
    dialog.showOpenDialog=async()=>({canceled:false,filePaths:[${JSON.stringify(root)}]});
    globalThis.__w123={calls:[],blocked:[],resolve:null,settled:false};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd==='workflow_generate_script'){
        globalThis.__w123.calls.push({cmd,payload});
        return await new Promise(resolve=>{globalThis.__w123.resolve=()=>{globalThis.__w123.settled=true;resolve({cancelled:true});};});
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w123.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await ev(`import('/src/settingsPanel.ts').then(m=>m.setSelectedChatModelFromSelect('offline-fixture::no-submit-model'))`);
  if (!await ev(`!!document.querySelector('.workflow-script-draft')`)) await click(visibleButton('Create script'));
  await until('script draft', () => ev(`document.querySelector('.workflow-script-draft')?.getAttribute('aria-busy') === 'false'`));
  await ev(`(() => { const n=document.querySelector('.workflow-script-draft [name="notes"]'); n.value='Disposable W1-23 native cancellation prompt, no provider call.'; n.dispatchEvent(new Event('input',{bubbles:true})); })()`);

  await click(visibleButton('Generate script from notes'));
  await until('editable request prompt', () => ev(`!!document.querySelector('dialog[data-generation-prompt][open]')`));
  await click(visibleButton('Prepare request'));
  await until('native request actually submitted and held', () => main.evaluate(`globalThis.__w123.calls.length === 1 && typeof globalThis.__w123.resolve === 'function' && !globalThis.__w123.settled`));
  const pendingControl = await ev(`(() => { const b=${visibleButton('Generating script…')}; return {label:b?.textContent?.trim()??null,disabled:b?.disabled??null,ariaBusy:b?.getAttribute('aria-busy')??null}; })()`);
  assert.deepEqual(pendingControl, {label:'Generating script…',disabled:true,ariaBusy:'true'});
  await main.evaluate('globalThis.__w123.resolve()');
  await until('native cancelled response settled', () => main.evaluate('globalThis.__w123.settled'));
  await until('Script cancelled receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="cancelled"] strong')?.textContent === ${JSON.stringify(expectedStatus)}`));

  const readback = await ev(`(() => {
    const workspace=document.querySelector('.workflow-main');
    const receipt=workspace.querySelector(':scope > .workflow-request-receipts [data-request-outcome="cancelled"]');
    const heading=receipt.querySelector('strong'), detail=receipt.querySelector('p');
    const primaryStatuses=[...workspace.querySelectorAll(':scope > .workflow-request-receipts [data-request-receipt] > strong')].map(e=>e.textContent?.trim()??'');
    const retry=[...workspace.querySelectorAll('button')].find(e=>e.textContent?.trim()==='Generate script from notes');
    const style=getComputedStyle(receipt), r=receipt.getBoundingClientRect();
    return {
      activeSurface:[...workspace.querySelectorAll('.studio-surface-nav button[aria-current="page"]')].map(e=>[...e.children].find(c=>c.textContent?.trim()==='Script')?.textContent?.trim()).filter(Boolean),
      status:heading.textContent, detail:detail.textContent, outcome:receipt.dataset.requestOutcome, role:receipt.getAttribute('role'),
      visible:receipt.getClientRects().length>0&&r.width>0&&r.height>0&&style.visibility==='visible'&&style.display!=='none'&&Number(style.opacity)>0,
      inInitialViewport:r.top>=0&&r.left>=0&&r.bottom<=innerHeight&&r.right<=innerWidth&&scrollX===0&&scrollY===0,
      primaryStatuses,
      containsSuccess:primaryStatuses.some(text=>/\bSuccess\b/.test(text)),
      containsError:primaryStatuses.some(text=>/\bError\b/.test(text)),
      retry:{label:retry?.textContent?.trim()??null,disabled:retry?.disabled??null,ariaBusy:retry?.getAttribute('aria-busy')??null},
      globalIndicator:document.querySelector('[data-global-request-indicator]')?.textContent?.trim()??null,
      jobsExpanded:document.querySelector('.studio-jobs-toggle')?.getAttribute('aria-expanded')??null,
    };
  })()`);
  result.readback = {...readback, pendingControl};
  result.hostCalls = await main.evaluate('globalThis.__w123.calls');
  result.network = { blockedRemoteRequests: await main.evaluate('globalThis.__w123.blocked'), hostSettled: await main.evaluate('globalThis.__w123.settled') };
  assert.deepEqual(readback.activeSurface, ['Script']);
  assert.equal(readback.status, expectedStatus);
  assert.equal(readback.detail, expectedDetail);
  assert.equal(readback.outcome, 'cancelled');
  assert.equal(readback.role, 'status');
  assert.equal(readback.visible, true);
  assert.equal(readback.inInitialViewport, true);
  assert.equal(readback.containsSuccess, false);
  assert.equal(readback.containsError, false);
  assert.deepEqual(readback.retry, {label:'Generate script from notes',disabled:false,ariaBusy:'false'});
  assert.equal(readback.globalIndicator, null);
  assert.equal(readback.jobsExpanded, 'false');
  assert.equal(result.hostCalls.length, 1);
  assert.equal(result.hostCalls[0].cmd, 'workflow_generate_script');
  assert.equal(result.network.hostSettled, true);
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/native-cancelled-response.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: one actually submitted native Script request returned cancelled and displayed "${readback.status}" in the initial viewport, with no Success or Error status.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), calls: await main.evaluate('globalThis.__w123?.calls'), blocked: await main.evaluate('globalThis.__w123?.blocked'), settled: await main.evaluate('globalThis.__w123?.settled') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.exit(0)`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/native-cancelled-response.png`},null,2));
}
process.exitCode = result.exit;
