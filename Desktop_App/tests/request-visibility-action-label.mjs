import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from '/source/Studio/Desktop_App/tests/ui-tracer.mjs';

const app = '/source/Studio/Desktop_App';
const out = `${app}/evidence/w1-11/runtime-352`;
const root = `${out}/disposable-project`;
const command = "docker exec studio-dev-workbench-1 bash -lc 'cd /source/Studio/Desktop_App && xvfb-run -a node tests/request-visibility-action-label.mjs'";
const action = {
  control: 'Generate script from notes',
  confirmingControl: 'Prepare request',
  hostCommand: 'workflow_generate_script',
  receiptLabel: 'Generate script from notes',
};
const expectedPrimary = `Submitted · Working — ${action.receiptLabel}`;
fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(root, { recursive: true });
fs.writeFileSync(`${root}/sentinel.txt`, 'Disposable offline W1-11 project\n');

const result = {
  check: 'request_visibility_action_label',
  expected: `Submitting the named "${action.control}" action invokes ${action.hostCommand} and visibly names "${action.receiptLabel}" in the primary Working line.`,
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
      WORKBENCH_HOST_BIN: '/runtime/target/debug/workbench-host',
      XDG_CONFIG_HOME: '/runtime/w1-11-config',
      XDG_CACHE_HOME: '/runtime/w1-11-cache',
      XDG_DATA_HOME: '/runtime/w1-11-data',
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
    globalThis.__w111={calls:[],blocked:[],resolve:null,settled:false};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd===${JSON.stringify(action.hostCommand)}){
        globalThis.__w111.calls.push({cmd,payload});
        return await new Promise(resolve=>{globalThis.__w111.resolve=value=>{globalThis.__w111.settled=true;resolve(value);};});
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w111.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await ev(`import('/src/settingsPanel.ts').then(m=>m.setSelectedChatModelFromSelect('offline-fixture::no-submit-model'))`);
  if (!await ev(`!!document.querySelector('.workflow-script-draft')`)) await click(visibleButton('Create script'));
  await until('script draft', () => ev(`document.querySelector('.workflow-script-draft')?.getAttribute('aria-busy') === 'false'`));
  await ev(`(() => { const n=document.querySelector('.workflow-script-draft [name="notes"]'); n.value='Disposable W1-11 action-label prompt, no provider call.'; n.dispatchEvent(new Event('input',{bubbles:true})); })()`);

  await click(visibleButton(action.control));
  await until('editable request prompt', () => ev(`!!document.querySelector('dialog[data-generation-prompt][open]')`));
  await click(visibleButton(action.confirmingControl));
  await until('host request held', () => main.evaluate(`globalThis.__w111.calls.length === 1 && typeof globalThis.__w111.resolve === 'function' && !globalThis.__w111.settled`));
  await until('working receipt', () => ev(`document.querySelector('[data-request-receipt][data-request-outcome="working"]')?.querySelector('strong')?.textContent === ${JSON.stringify(expectedPrimary)}`));

  result.hostCalls = await main.evaluate('globalThis.__w111.calls');
  result.network = {
    blockedRemoteRequests: await main.evaluate('globalThis.__w111.blocked'),
    hostSettled: await main.evaluate('globalThis.__w111.settled'),
  };
  result.readback = await ev(`(() => {
    const receipt=document.querySelector('[data-request-receipt][data-request-outcome="working"]');
    const primary=receipt?.querySelector('strong');
    const r=receipt?.getBoundingClientRect();
    const style=receipt?getComputedStyle(receipt):null;
    return {
      primaryText:primary?.textContent??null,
      visible:!!receipt&&receipt.getClientRects().length>0&&r.width>0&&r.height>0&&style.visibility==='visible'&&style.display!=='none'&&Number(style.opacity)>0,
      inInitialViewport:!!r&&r.top>=0&&r.left>=0&&r.bottom<=innerHeight&&r.right<=innerWidth&&scrollX===0&&scrollY===0,
      outcome:receipt?.dataset.requestOutcome??null,
      jobsOpen:!!document.querySelector('[data-surface="jobs"]'),
    };
  })()`);

  assert.equal(result.hostCalls.length, 1);
  assert.equal(result.hostCalls[0].cmd, action.hostCommand);
  assert.equal(result.readback.primaryText, expectedPrimary);
  assert.equal(result.readback.primaryText.endsWith(`— ${action.receiptLabel}`), true);
  assert.equal(result.readback.visible, true);
  assert.equal(result.readback.inInitialViewport, true);
  assert.equal(result.readback.outcome, 'working');
  assert.equal(result.readback.jobsOpen, false);
  assert.equal(result.network.hostSettled, false);
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/named-action-visible.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: invoking ${result.hostCalls[0].cmd} from "${action.control}" displayed the visible primary status "${result.readback.primaryText}" in the initial viewport while the matching host request remained unresolved.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), calls: await main.evaluate('globalThis.__w111?.calls'), blocked: await main.evaluate('globalThis.__w111?.blocked'), settled: await main.evaluate('globalThis.__w111?.settled') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`globalThis.__w111?.resolve?.({cancelled:true}); process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.quit()`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/named-action-visible.png`},null,2));
}
process.exitCode = result.exit;
