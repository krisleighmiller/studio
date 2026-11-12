import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from './helpers/ui.mjs';
import { appRoot as app, prepareOutput } from './helpers/runtime.mjs';

const out = prepareOutput('request-visibility-delayed-alive');
const root = `${out}/disposable-project`;
const command = [process.execPath, ...process.execArgv, ...process.argv.slice(1)].map(arg => JSON.stringify(arg)).join(' ');
const idleLabel = 'Generate script from notes';
const workingLabel = 'Generating script…';
const workingReceipt = 'Submitted · Working — Generate script from notes';
const delayMs = 3000;
fs.mkdirSync(root, { recursive: true });
fs.writeFileSync(`${root}/sentinel.txt`, 'Disposable offline W1-10 project\n');

const result = {
  check: 'request_visibility_delayed_alive',
  expected: `The same visible "${workingReceipt}" status remains active throughout a ${delayMs} ms unresolved host delay.`,
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
  electron = spawn(`${app}/node_modules/.bin/electron`, ['.', '--no-sandbox', '--disable-gpu', '--inspect=127.0.0.1:9790', '--remote-debugging-port=9791'], {
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
  const mt = await until('main target', async () => { try { return (await (await fetch('http://127.0.0.1:9790/json/list')).json())[0]; } catch { return null; } });
  main = await CDP.open(mt.webSocketDebuggerUrl);
  await main.call('Runtime.enable');
  const rt = await until('renderer target', async () => { try { return (await (await fetch('http://127.0.0.1:9791/json/list')).json()).find(target => target.type === 'page'); } catch { return null; } });
  cdp = await CDP.open(rt.webSocketDebuggerUrl);
  await until('ready', () => ev(`document.querySelector('#status-left')?.textContent?.startsWith('Ready —')`));

  await main.evaluate(`(() => {
    const {BrowserWindow,dialog,ipcMain,session}=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron');
    BrowserWindow.getAllWindows()[0].setSize(1600,1200);
    dialog.showOpenDialog=async()=>({canceled:false,filePaths:[${JSON.stringify(root)}]});
    globalThis.__w110={calls:[],blocked:[],resolve:null,settled:false};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd==='workflow_generate_script'){
        globalThis.__w110.calls.push({cmd,payload});
        return await new Promise(resolve=>{globalThis.__w110.resolve=value=>{globalThis.__w110.settled=true;resolve(value);};});
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w110.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await ev(`import('/src/settingsPanel.ts').then(m=>m.setSelectedChatModelFromSelect('offline-fixture::no-submit-model'))`);
  if (!await ev(`!!document.querySelector('.workflow-script-draft')`)) await click(visibleButton('Create script'));
  await until('script draft', () => ev(`document.querySelector('.workflow-script-draft')?.getAttribute('aria-busy') === 'false'`));
  await ev(`(() => { const n=document.querySelector('.workflow-script-draft [name="notes"]'); n.value='Disposable W1-10 delayed-alive prompt, no provider call.'; n.dispatchEvent(new Event('input',{bubbles:true})); })()`);

  await click(visibleButton('Generate script from notes'));
  await until('editable request prompt', () => ev(`!!document.querySelector('dialog[data-generation-prompt][open]')`));
  await click(visibleButton('Prepare request'));
  await until('host request held', () => main.evaluate(`globalThis.__w110.calls.length === 1 && typeof globalThis.__w110.resolve === 'function' && !globalThis.__w110.settled`));
  await until('working receipt', () => ev(`document.querySelector('[data-request-receipt][data-request-outcome="working"]')?.querySelector('strong')?.textContent === ${JSON.stringify(workingReceipt)}`));

  const readback = await ev(`(async() => {
    const receipt=document.querySelector('[data-request-receipt][data-request-outcome="working"]');
    const started=performance.now();
    const mutations=[];
    const observer=new MutationObserver(records=>{
      for(const record of records) {
        if([...record.removedNodes].some(node=>node===receipt||node.contains?.(receipt))) mutations.push('removed');
        if([...record.addedNodes].some(node=>node===receipt||node.contains?.(receipt))) mutations.push('added');
      }
    });
    observer.observe(document.body,{childList:true,subtree:true});
    const samples=[];
    while(performance.now()-started<${delayMs}) {
      const current=document.querySelector('[data-request-receipt][data-request-outcome="working"]');
      const style=current?getComputedStyle(current):null;
      const r=current?.getBoundingClientRect();
      samples.push({
        elapsedMs:Math.round(performance.now()-started),
        sameNode:current===receipt,
        connected:receipt.isConnected,
        primaryText:current?.querySelector('strong')?.textContent??null,
        visible:!!current&&current.getClientRects().length>0&&r.width>0&&r.height>0&&style.visibility==='visible'&&style.display!=='none'&&Number(style.opacity)>0,
        inViewport:!!r&&r.top>=0&&r.left>=0&&r.bottom<=innerHeight&&r.right<=innerWidth,
      });
      await new Promise(resolve=>setTimeout(resolve,100));
    }
    const current=document.querySelector('[data-request-receipt][data-request-outcome="working"]');
    const style=current?getComputedStyle(current):null;
    const r=current?.getBoundingClientRect();
    samples.push({elapsedMs:Math.round(performance.now()-started),sameNode:current===receipt,connected:receipt.isConnected,primaryText:current?.querySelector('strong')?.textContent??null,visible:!!current&&current.getClientRects().length>0&&r.width>0&&r.height>0&&style.visibility==='visible'&&style.display!=='none'&&Number(style.opacity)>0,inViewport:!!r&&r.top>=0&&r.left>=0&&r.bottom<=innerHeight&&r.right<=innerWidth});
    observer.disconnect();
    const control=[...document.querySelectorAll('.workflow-script-draft button')].find(e=>e.textContent?.trim()===${JSON.stringify(idleLabel)}||e.textContent?.trim()===${JSON.stringify(workingLabel)});
    return {
      delayMs:Math.round(performance.now()-started),
      sampleCount:samples.length,
      first:samples[0],
      last:samples.at(-1),
      allSameVisibleWorking:samples.every(sample=>sample.sameNode&&sample.connected&&sample.primaryText===${JSON.stringify(workingReceipt)}&&sample.visible&&sample.inViewport),
      mutations,
      control:{label:control.textContent.trim(),disabled:control.disabled,ariaBusy:control.getAttribute('aria-busy')},
    };
  })()`);
  result.readback = readback;
  result.hostCalls = await main.evaluate('globalThis.__w110.calls');
  result.network = { blockedRemoteRequests: await main.evaluate('globalThis.__w110.blocked'), hostSettled: await main.evaluate('globalThis.__w110.settled') };
  assert.ok(readback.delayMs >= delayMs);
  assert.ok(readback.sampleCount >= 2);
  assert.equal(readback.allSameVisibleWorking, true);
  assert.deepEqual(readback.mutations, []);
  assert.equal(readback.control.label, workingLabel);
  assert.equal(readback.control.disabled, true);
  assert.equal(readback.control.ariaBusy, 'true');
  assert.equal(result.hostCalls.length, 1);
  assert.equal(result.hostCalls[0].cmd, 'workflow_generate_script');
  assert.equal(result.network.hostSettled, false);
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/delayed-working-alive.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: the same visible "${workingReceipt}" DOM status remained connected and in the initial viewport across all ${readback.sampleCount} observations during a ${readback.delayMs} ms unresolved host delay; no replacement, removal, settlement, or remote request occurred.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), calls: await main.evaluate('globalThis.__w110?.calls'), blocked: await main.evaluate('globalThis.__w110?.blocked'), settled: await main.evaluate('globalThis.__w110?.settled') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`globalThis.__w110?.resolve?.({cancelled:true}); process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.exit(0)`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/delayed-working-alive.png`},null,2));
}
process.exitCode = result.exit;
