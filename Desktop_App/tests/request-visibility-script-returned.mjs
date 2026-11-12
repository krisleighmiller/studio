import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from '/source/Studio/Desktop_App/tests/ui-tracer.mjs';

const app = '/source/Studio/Desktop_App';
const out = `${app}/evidence/w1-20/runtime-361`;
const root = `${out}/disposable-project`;
const command = "docker exec studio-dev-workbench-1 bash -lc 'cd /source/Studio/Desktop_App && xvfb-run -a node tests/request-visibility-script-returned.mjs'";
const expectedStatus = 'Submitted · Draft returned — Generate script from notes';
const returnedScript = '# Disposable returned draft\n\n## Scene 1 — Kitchen\nAlex: This draft is not saved yet.\n';
fs.rmSync(out, { recursive: true, force: true });
for (const dir of ['/runtime/w1-20-config', '/runtime/w1-20-cache', '/runtime/w1-20-data']) fs.rmSync(dir, { recursive: true, force: true });
fs.mkdirSync(root, { recursive: true });
fs.writeFileSync(`${root}/sentinel.txt`, 'Disposable offline W1-20 project\n');

const result = {
  check: 'request_visibility_script_returned',
  expected: 'Resolving one Script request with draft text makes the visible status say the draft returned and still needs explicit Save script.',
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
  electron = spawn(`${app}/node_modules/.bin/electron`, ['.', '--no-sandbox', '--disable-gpu', '--inspect=127.0.0.1:9812', '--remote-debugging-port=9813'], {
    cwd: app,
    stdio: ['ignore', electronLog, electronLog],
    env: {
      ...process.env,
      WORKBENCH_DEV: '1',
      WORKBENCH_HOST_BIN: '/runtime/target/debug/workbench-host',
      XDG_CONFIG_HOME: '/runtime/w1-20-config',
      XDG_CACHE_HOME: '/runtime/w1-20-cache',
      XDG_DATA_HOME: '/runtime/w1-20-data',
    },
  });
  const mt = await until('main target', async () => { try { return (await (await fetch('http://127.0.0.1:9812/json/list')).json())[0]; } catch { return null; } });
  main = await CDP.open(mt.webSocketDebuggerUrl);
  await main.call('Runtime.enable');
  const rt = await until('renderer target', async () => { try { return (await (await fetch('http://127.0.0.1:9813/json/list')).json()).find(target => target.type === 'page'); } catch { return null; } });
  cdp = await CDP.open(rt.webSocketDebuggerUrl);
  await until('ready', () => ev(`document.querySelector('#status-left')?.textContent?.startsWith('Ready —')`));

  await main.evaluate(`(() => {
    const {BrowserWindow,dialog,ipcMain,session}=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron');
    BrowserWindow.getAllWindows()[0].setSize(1600,1200);
    dialog.showOpenDialog=async()=>({canceled:false,filePaths:[${JSON.stringify(root)}]});
    globalThis.__w120={calls:[],blocked:[],settled:false};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd==='workflow_generate_script'){
        globalThis.__w120.calls.push({cmd,payload});
        globalThis.__w120.settled=true;
        return {script:${JSON.stringify(returnedScript)}};
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w120.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await ev(`import('/src/settingsPanel.ts').then(m=>m.setSelectedChatModelFromSelect('offline-fixture::no-submit-model'))`);
  if (!await ev(`!!document.querySelector('.workflow-script-draft')`)) await click(visibleButton('Create script'));
  await until('script draft', () => ev(`document.querySelector('.workflow-script-draft')?.getAttribute('aria-busy') === 'false'`));
  await ev(`(() => { const n=document.querySelector('.workflow-script-draft [name="notes"]'); n.value='Disposable W1-20 returned-draft prompt, no provider call.'; n.dispatchEvent(new Event('input',{bubbles:true})); })()`);

  await click(visibleButton('Generate script from notes'));
  await until('editable request prompt', () => ev(`!!document.querySelector('dialog[data-generation-prompt][open]')`));
  await click(visibleButton('Prepare request'));
  await until('host returned draft', () => main.evaluate(`globalThis.__w120.calls.length === 1 && globalThis.__w120.settled`));
  await until('Script draft-returned receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="returned"] strong')?.textContent === ${JSON.stringify(expectedStatus)}`));

  const readback = await ev(`(() => {
    const workspace=document.querySelector('.workflow-main');
    const receipt=workspace.querySelector(':scope > .workflow-request-receipts [data-request-outcome="returned"]');
    const heading=receipt.querySelector('strong');
    const detail=receipt.querySelector('p');
    const draft=workspace.querySelector('.workflow-script-draft');
    const activeSurface=[...workspace.querySelectorAll('.studio-surface-nav button[aria-current="page"]')].map(e=>e.children[1]?.textContent?.trim());
    const style=getComputedStyle(receipt), r=receipt.getBoundingClientRect();
    return {
      activeSurface,
      status:heading.textContent,
      detail:detail.textContent,
      outcome:receipt.dataset.requestOutcome,
      role:receipt.getAttribute('role'),
      localToWorkspace:receipt.closest('.workflow-main')===workspace,
      receiptBeforeDraft:Boolean(draft && (receipt.compareDocumentPosition(draft)&Node.DOCUMENT_POSITION_FOLLOWING)),
      visible:receipt.getClientRects().length>0 && r.width>0 && r.height>0 && style.visibility==='visible' && style.display!=='none' && Number(style.opacity)>0,
      rect:{x:r.x,y:r.y,width:r.width,height:r.height,bottom:r.bottom,right:r.right},
      viewport:{width:innerWidth,height:innerHeight,scrollX,scrollY},
      scriptText:draft.querySelector('[name="script"]')?.value,
      saveScript:{label:[...draft.querySelectorAll('button')].find(e=>e.textContent?.trim()==='Save script')?.textContent ?? null,disabled:[...draft.querySelectorAll('button')].find(e=>e.textContent?.trim()==='Save script')?.disabled ?? null},
    };
  })()`);
  result.readback = readback;
  result.hostCalls = await main.evaluate('globalThis.__w120.calls');
  result.network = { blockedRemoteRequests: await main.evaluate('globalThis.__w120.blocked'), hostSettled: await main.evaluate('globalThis.__w120.settled') };
  assert.deepEqual(readback.activeSurface, ['Script']);
  assert.equal(readback.status, expectedStatus);
  assert.equal(readback.detail, 'Script returned. Review the draft and explicitly Save script to register it.');
  assert.equal(readback.outcome, 'returned');
  assert.equal(readback.role, 'status');
  assert.equal(readback.localToWorkspace, true);
  assert.equal(readback.receiptBeforeDraft, true);
  assert.equal(readback.visible, true);
  assert.equal(readback.scriptText, returnedScript);
  assert.deepEqual(readback.saveScript, {label:'Save script',disabled:false});
  assert.equal(result.hostCalls.length, 1);
  assert.equal(result.hostCalls[0].cmd, 'workflow_generate_script');
  assert.equal(result.network.hostSettled, true);
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  assert.equal(fs.existsSync(`${root}/Scripts`), false);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/script-draft-returned.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: Script displayed "${readback.status}" with the explicit Save script requirement, populated the unsaved draft, and made no remote request or script-file write.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), calls: await main.evaluate('globalThis.__w120?.calls'), blocked: await main.evaluate('globalThis.__w120?.blocked'), settled: await main.evaluate('globalThis.__w120?.settled') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.quit()`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/script-draft-returned.png`},null,2));
}
process.exitCode = result.exit;
