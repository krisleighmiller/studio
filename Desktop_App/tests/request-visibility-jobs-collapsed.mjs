import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from './helpers/ui.mjs';
import { appRoot as app, prepareOutput } from './helpers/runtime.mjs';

const out = prepareOutput('request-visibility-jobs-collapsed');
const root = `${out}/disposable-project`;
const command = [process.execPath, ...process.execArgv, ...process.argv.slice(1)].map(arg => JSON.stringify(arg)).join(' ');
const action = {
  control: 'Generate script from notes',
  confirmingControl: 'Prepare request',
  hostCommand: 'workflow_generate_script',
  receiptLabel: 'Generate script from notes',
};
const expectedPrimary = `Submitted · Working — ${action.receiptLabel}`;
const expectedIndicator = '1 active request · Working — Generate script from notes';
fs.mkdirSync(root, { recursive: true });
fs.writeFileSync(`${root}/sentinel.txt`, 'Disposable offline W1-16 project\n');

const result = {
  check: 'request_visibility_jobs_collapsed',
  expected: 'After Jobs is opened and collapsed during one active request, active work remains obvious outside Jobs in the initial viewport.',
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
  electron = spawn(`${app}/node_modules/.bin/electron`, ['.', '--no-sandbox', '--disable-gpu', '--inspect=127.0.0.1:9800', '--remote-debugging-port=9801'], {
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
  const mt = await until('main target', async () => { try { return (await (await fetch('http://127.0.0.1:9800/json/list')).json())[0]; } catch { return null; } });
  main = await CDP.open(mt.webSocketDebuggerUrl);
  await main.call('Runtime.enable');
  const rt = await until('renderer target', async () => { try { return (await (await fetch('http://127.0.0.1:9801/json/list')).json()).find(target => target.type === 'page'); } catch { return null; } });
  cdp = await CDP.open(rt.webSocketDebuggerUrl);
  await until('ready', () => ev(`document.querySelector('#status-left')?.textContent?.startsWith('Ready —')`));

  await main.evaluate(`(() => {
    const {BrowserWindow,dialog,ipcMain,session}=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron');
    BrowserWindow.getAllWindows()[0].setSize(1600,1200);
    dialog.showOpenDialog=async()=>({canceled:false,filePaths:[${JSON.stringify(root)}]});
    globalThis.__w116={calls:[],blocked:[],resolve:null,settled:false};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd===${JSON.stringify(action.hostCommand)}){
        globalThis.__w116.calls.push({cmd,payload});
        return await new Promise(resolve=>{globalThis.__w116.resolve=value=>{globalThis.__w116.settled=true;resolve(value);};});
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w116.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await click(visibleButton('Studio'));
  await until('Studio workspace', () => ev(`!!document.querySelector('.workflow-main')`));
  await ev(`import('/src/settingsPanel.ts').then(m=>m.setSelectedChatModelFromSelect('offline-fixture::no-submit-model'))`);
  if (!await ev(`!!document.querySelector('.workflow-script-draft')`)) await click(visibleButton('Create script'));
  await until('script draft', () => ev(`document.querySelector('.workflow-script-draft')?.getAttribute('aria-busy') === 'false'`));
  await ev(`(() => { const n=document.querySelector('.workflow-script-draft [name="notes"]'); n.value='Disposable W1-16 collapsed-Jobs prompt, no provider call.'; n.dispatchEvent(new Event('input',{bubbles:true})); })()`);

  await click(visibleButton(action.control));
  await until('editable request prompt', () => ev(`!!document.querySelector('dialog[data-generation-prompt][open]')`));
  await click(visibleButton(action.confirmingControl));
  await until('host request held', () => main.evaluate(`globalThis.__w116.calls.length === 1 && typeof globalThis.__w116.resolve === 'function' && !globalThis.__w116.settled`));
  await until('working receipt', () => ev(`document.querySelector('[data-request-receipt][data-request-outcome="working"]')?.querySelector('strong')?.textContent === ${JSON.stringify(expectedPrimary)}`));

  await until('global active request indicator', () => ev(`document.querySelector('[data-global-request-indicator]')?.textContent?.trim() === ${JSON.stringify(expectedIndicator)}`));
  await click(`document.querySelector('.studio-jobs-toggle')`);
  await until('Jobs opened', () => ev(`document.querySelector('.studio-jobs-toggle')?.getAttribute('aria-expanded') === 'true'`));
  await click(`document.querySelector('.studio-jobs-toggle')`);
  await until('Jobs collapsed', () => ev(`document.querySelector('.studio-jobs-toggle')?.getAttribute('aria-expanded') === 'false'`));

  result.hostCalls = await main.evaluate('globalThis.__w116.calls');
  result.network = {
    blockedRemoteRequests: await main.evaluate('globalThis.__w116.blocked'),
    hostSettled: await main.evaluate('globalThis.__w116.settled'),
  };
  result.readback = await ev(`(() => {
    const indicator=document.querySelector('[data-global-request-indicator]');
    const r=indicator?.getBoundingClientRect();
    const style=indicator?getComputedStyle(indicator):null;
    return {
      activeSurface:[...document.querySelectorAll('.studio-surface-nav button[aria-current="page"]')].map(e=>[...e.children].find(c=>c.textContent?.trim()==='Script')?.textContent?.trim()).filter(Boolean),
      indicatorText:indicator?.textContent?.trim()??null,
      visible:!!indicator&&indicator.getClientRects().length>0&&r.width>0&&r.height>0&&style.visibility==='visible'&&style.display!=='none'&&Number(style.opacity)>0,
      inInitialViewport:!!r&&r.top>=0&&r.left>=0&&r.bottom<=innerHeight&&r.right<=innerWidth&&scrollX===0&&scrollY===0,
      fontSize:style?.fontSize??null,
      localWorkingReceiptVisible:!!document.querySelector('[data-request-receipt][data-request-outcome="working"]'),
      localReceiptText:document.querySelector('[data-request-receipt][data-request-outcome="working"] strong')?.textContent??null,
      jobsExpanded:document.querySelector('.studio-jobs-toggle')?.getAttribute('aria-expanded')??null,
      jobsBodyVisible:!!document.querySelector('.workflow-jobs-body')?.getClientRects().length,
      jobsFiltersVisible:!!document.querySelector('.studio-job-filters')?.getClientRects().length,
      pendingControl:{
        label:[...document.querySelectorAll('button')].find(e=>e.textContent?.trim()==='Generating script…')?.textContent?.trim()??null,
        ariaBusy:[...document.querySelectorAll('button')].find(e=>e.textContent?.trim()==='Generating script…')?.getAttribute('aria-busy')??null,
        disabled:[...document.querySelectorAll('button')].find(e=>e.textContent?.trim()==='Generating script…')?.disabled??null,
      },
    };
  })()`);

  assert.equal(result.hostCalls.length, 1);
  assert.equal(result.hostCalls[0].cmd, action.hostCommand);
  assert.deepEqual(result.readback.activeSurface, ['Script']);
  assert.equal(result.readback.indicatorText, expectedIndicator);
  assert.equal(result.readback.visible, true);
  assert.equal(result.readback.inInitialViewport, true);
  assert.ok(Number.parseFloat(result.readback.fontSize) >= 14);
  assert.equal(result.readback.localWorkingReceiptVisible, true);
  assert.equal(result.readback.localReceiptText, expectedPrimary);
  assert.equal(result.readback.jobsExpanded, 'false');
  assert.equal(result.readback.jobsBodyVisible, false);
  assert.equal(result.readback.jobsFiltersVisible, false);
  assert.deepEqual(result.readback.pendingControl, { label: 'Generating script…', ariaBusy: 'true', disabled: true });
  assert.equal(result.network.hostSettled, false);
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/active-work-outside-collapsed-jobs.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: after Jobs was opened and collapsed, the initial viewport still showed "${result.readback.indicatorText}" and the local "${result.readback.localReceiptText}" receipt outside Jobs while the initiating control remained pending.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), calls: await main.evaluate('globalThis.__w116?.calls'), blocked: await main.evaluate('globalThis.__w116?.blocked'), settled: await main.evaluate('globalThis.__w116?.settled') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`globalThis.__w116?.resolve?.({cancelled:true}); process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.exit(0)`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/active-work-outside-collapsed-jobs.png`},null,2));
}
process.exitCode = result.exit;
