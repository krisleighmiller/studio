import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from '/source/Studio/Desktop_App/tests/ui-tracer.mjs';

const app = '/source/Studio/Desktop_App';
const out = `${app}/evidence/w1-04/runtime-345`;
const root = `${out}/disposable-project`;
const command = "docker exec studio-dev-workbench-1 bash -lc 'cd /source/Studio/Desktop_App && xvfb-run -a node tests/request-visibility-live-status.mjs'";
const expectedAnnouncement = 'Submitted · Working — Generate script from notes';
fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(root, { recursive: true });
fs.writeFileSync(`${root}/sentinel.txt`, 'Disposable offline W1-04 project\n');

const result = {
  check: 'request_visibility_live_status',
  expected: `One delayed request mounts exactly one atomic status live region whose working announcement is exactly "${expectedAnnouncement}".`,
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
      WORKBENCH_HOST_BIN: '/runtime/target/debug/workbench-host',
      XDG_CONFIG_HOME: '/runtime/w1-04-config',
      XDG_CACHE_HOME: '/runtime/w1-04-cache',
      XDG_DATA_HOME: '/runtime/w1-04-data',
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
    globalThis.__w104={calls:[],blocked:[],resolve:null};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd==='workflow_generate_script'){
        globalThis.__w104.calls.push({cmd,payload});
        return await new Promise(resolve=>{globalThis.__w104.resolve=resolve;});
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w104.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await ev(`import('/src/settingsPanel.ts').then(m=>m.setSelectedChatModelFromSelect('offline-fixture::no-submit-model'))`);
  if (!await ev(`!!document.querySelector('.workflow-script-draft')`)) await click(visibleButton('Create script'));
  await until('script draft', () => ev(`document.querySelector('.workflow-script-draft')?.getAttribute('aria-busy') === 'false'`));
  await ev(`(() => {
    globalThis.__w104MountedStatuses=[];
    globalThis.__w104Observer=new MutationObserver(records=>{
      for(const record of records) for(const node of record.addedNodes) {
        if(!(node instanceof Element)) continue;
        const statuses=[...(node.matches?.('[data-request-receipt][role="status"]')?[node]:[]),...node.querySelectorAll?.('[data-request-receipt][role="status"]')??[]];
        for(const status of statuses) globalThis.__w104MountedStatuses.push({text:status.querySelector('strong')?.textContent??'',atomic:status.getAttribute('aria-atomic')});
      }
    });
    globalThis.__w104Observer.observe(document.body,{childList:true,subtree:true});
    const n=document.querySelector('.workflow-script-draft [name="notes"]');
    n.value='Disposable W1-04 delayed prompt, no provider call.';
    n.dispatchEvent(new Event('input',{bubbles:true}));
  })()`);

  await click(visibleButton('Generate script from notes'));
  await until('editable request prompt', () => ev(`!!document.querySelector('dialog[data-generation-prompt][open]')`));
  await click(visibleButton('Prepare request'));
  await until('host request held', () => main.evaluate(`globalThis.__w104.calls.length === 1 && typeof globalThis.__w104.resolve === 'function'`));
  await until('working status', () => ev(`document.querySelector('[data-request-receipt][data-request-outcome="working"]')?.querySelector('strong')?.textContent === ${JSON.stringify(expectedAnnouncement)}`));
  await sleep(250);

  const readback = await ev(`(() => {
    globalThis.__w104Observer.disconnect();
    const status=document.querySelector('[data-request-receipt][data-request-outcome="working"]');
    return {
      mountedStatuses:globalThis.__w104MountedStatuses,
      mountedCount:globalThis.__w104MountedStatuses.length,
      role:status.getAttribute('role'),
      ariaAtomic:status.getAttribute('aria-atomic'),
      primaryText:status.querySelector('strong')?.textContent,
      liveRegionText:status.innerText,
      connected:status.isConnected,
    };
  })()`);
  result.readback = readback;
  result.hostCalls = await main.evaluate('globalThis.__w104.calls');
  result.network = { blockedRemoteRequests: await main.evaluate('globalThis.__w104.blocked') };
  assert.equal(readback.connected, true);
  assert.equal(readback.role, 'status');
  assert.equal(readback.ariaAtomic, 'true');
  assert.equal(readback.primaryText, expectedAnnouncement);
  assert.equal(readback.mountedCount, 1);
  assert.deepEqual(readback.mountedStatuses, [{ text: expectedAnnouncement, atomic: 'true' }]);
  assert.equal(result.hostCalls.length, 1);
  assert.equal(result.hostCalls[0].cmd, 'workflow_generate_script');
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  result.actual = `PASS: one mounted role=status live region had aria-atomic=true and exposed the exact working announcement "${readback.primaryText}" once while the single host request remained unresolved.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), mounted: await ev('globalThis.__w104MountedStatuses'), calls: await main.evaluate('globalThis.__w104?.calls'), blocked: await main.evaluate('globalThis.__w104?.blocked') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`globalThis.__w104?.resolve?.({cancelled:true}); process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.quit()`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`},null,2));
}
process.exitCode = result.exit;
