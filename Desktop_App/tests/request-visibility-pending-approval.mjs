import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from './helpers/ui.mjs';
import { appRoot as app, prepareOutput } from './helpers/runtime.mjs';

const out = prepareOutput('request-visibility-pending-approval');
const root = `${out}/disposable-project`;
const command = [process.execPath, ...process.execArgv, ...process.argv.slice(1)].map(arg => JSON.stringify(arg)).join(' ');
const expectedPending = 'Submitted · Pending approval — Saving production profile';
const pendingAction = 'Save the exact disposable production profile; no media execution or provider request. [[workbench_approve:W119PENDING]]';
fs.mkdirSync(`${root}/Scripts`, { recursive: true });
fs.writeFileSync(`${root}/Scripts/story.md`, '# Disposable story\n\n## Scene 1 — Kitchen\nAlex: This is an offline W1-06 fixture.\n');

const result = {
  check: 'request_visibility_pending_approval',
  expected: 'Returning one pending action with no saved media makes the primary status say Pending approval and never Success.',
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
async function click(source) {
  const point = await ev(`(() => { const e=${source}; if(!e || e.disabled || !e.getClientRects().length) throw Error('Missing visible enabled target'); const r=e.getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()`);
  await cdp.call('Input.dispatchMouseEvent', { type: 'mousePressed', ...point, button: 'left', clickCount: 1 });
  await cdp.call('Input.dispatchMouseEvent', { type: 'mouseReleased', ...point, button: 'left', clickCount: 1 });
  await sleep(180);
}

try {
  await until('Vite dev server', async () => { try { return (await fetch('http://127.0.0.1:1420/')).ok; } catch { return false; } });
  electron = spawn(`${app}/node_modules/.bin/electron`, ['.', '--no-sandbox', '--disable-gpu', '--inspect=127.0.0.1:9810', '--remote-debugging-port=9811'], {
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
  const mt = await until('main target', async () => { try { return (await (await fetch('http://127.0.0.1:9810/json/list')).json())[0]; } catch { return null; } });
  main = await CDP.open(mt.webSocketDebuggerUrl);
  await main.call('Runtime.enable');
  const rt = await until('renderer target', async () => { try { return (await (await fetch('http://127.0.0.1:9811/json/list')).json()).find(target => target.type === 'page'); } catch { return null; } });
  cdp = await CDP.open(rt.webSocketDebuggerUrl);
  await until('ready', () => ev(`document.querySelector('#status-left')?.textContent?.startsWith('Ready —')`));

  await main.evaluate(`(() => {
    const {BrowserWindow,dialog,ipcMain,session}=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron');
    BrowserWindow.getAllWindows()[0].setSize(1600,1200);
    dialog.showOpenDialog=async()=>({canceled:false,filePaths:[${JSON.stringify(root)}]});
    globalThis.__w119={calls:[],blocked:[],settled:false,returnedState:null};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd==='workflow_save_profile'){
        globalThis.__w119.calls.push({cmd,payload});
        const answer=await original(...args);
        answer.pendingAction=${JSON.stringify(pendingAction)};
        globalThis.__w119.settled=true;
        globalThis.__w119.returnedState=answer.state;
        return answer;
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w119.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await until('scene imported', () => ev(`document.querySelector('.workflow-scenes button')?.textContent?.includes('Kitchen')`));
  await click(`[...document.querySelectorAll('.studio-surface-nav button')].find(e => [...e.children].some(c => c.textContent?.trim() === 'Profile'))`);
  await until('Profile stage', () => ev(`document.querySelector('.workflow-main .workflow-project')?.getClientRects().length > 0`));
  await click(`[...document.querySelectorAll('.workflow-profile-form button')].find(e => e.textContent?.trim() === 'Save profile')`);
  await until('host returned pending action', () => main.evaluate(`globalThis.__w119.calls.length === 1 && globalThis.__w119.settled`));
  await until('pending approval receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="pending"] strong')?.textContent === ${JSON.stringify(expectedPending)}`));
  await until('exact-operation modal', () => ev(`document.querySelector('dialog[data-operation-approval][open]')?.getClientRects().length > 0`));

  const readback = await ev(`(() => {
    const workspace=document.querySelector('.workflow-main');
    const receipt=workspace.querySelector(':scope > .workflow-request-receipts [data-request-outcome="pending"]');
    const heading=receipt.querySelector('strong');
    const detail=receipt.querySelector('p');
    const stage=workspace.querySelector('.workflow-project');
    const activeSurface=[...workspace.querySelectorAll('.studio-surface-nav button[aria-current="page"]')].map(e=>[...e.children].find(c=>c.textContent?.trim()==='Profile')?.textContent?.trim()).filter(Boolean);
    const style=getComputedStyle(receipt), r=receipt.getBoundingClientRect();
    return {
      activeSurface,
      status:heading.textContent,
      detail:detail.textContent,
      outcome:receipt.dataset.requestOutcome,
      role:receipt.getAttribute('role'),
      localToWorkspace:receipt.closest('.workflow-main')===workspace,
      receiptBeforeStage:Boolean(stage && (receipt.compareDocumentPosition(stage)&Node.DOCUMENT_POSITION_FOLLOWING)),
      visible:receipt.getClientRects().length>0 && r.width>0 && r.height>0 && style.visibility==='visible' && style.display!=='none' && Number(style.opacity)>0,
      rect:{x:r.x,y:r.y,width:r.width,height:r.height,bottom:r.bottom,right:r.right},
      viewport:{width:innerWidth,height:innerHeight,scrollX,scrollY},
      modal:{
        title:document.querySelector('dialog[data-operation-approval] h2')?.textContent ?? null,
        body:document.querySelector('dialog[data-operation-approval] pre')?.textContent ?? null,
        approvalId:document.querySelector('dialog[data-operation-approval]')?.dataset.approvalId ?? null,
        visible:Boolean(document.querySelector('dialog[data-operation-approval][open]')?.getClientRects().length),
      },
      primaryStatuses:[...workspace.querySelectorAll(':scope > .workflow-request-receipts article > strong')].map(e=>e.textContent),
    };
  })()`);
  result.readback = readback;
  result.hostCalls = await main.evaluate('globalThis.__w119.calls');
  result.network = { blockedRemoteRequests: await main.evaluate('globalThis.__w119.blocked'), hostSettled: await main.evaluate('globalThis.__w119.settled') };
  result.returnedState = await main.evaluate('globalThis.__w119.returnedState');
  assert.deepEqual(readback.activeSurface, ['Profile']);
  assert.equal(readback.status, expectedPending);
  assert.equal(readback.detail, 'Inspect the exact host operation in the authorization modal. No media execution authorized.');
  assert.equal(readback.outcome, 'pending');
  assert.equal(readback.role, 'status');
  assert.equal(readback.localToWorkspace, true);
  assert.equal(readback.receiptBeforeStage, true);
  assert.equal(readback.visible, true);
  assert.equal(result.hostCalls.length, 1);
  assert.equal(result.hostCalls[0].cmd, 'workflow_save_profile');
  assert.equal(result.network.hostSettled, true);
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  assert.equal(readback.modal.title, 'Authorize exact operation');
  assert.equal(readback.modal.body, pendingAction.replace(/\[\[workbench_approve:[A-Za-z0-9]+\]\]/g, ''));
  assert.equal(readback.modal.approvalId, 'W119PENDING');
  assert.equal(readback.modal.visible, true);
  assert.equal(readback.primaryStatuses.some(text => text?.includes('Success')), false);
  assert.equal((result.returnedState.assets ?? []).some(asset => /\.(png|jpe?g|webp|gif|mp4|mov|webm|wav|mp3)$/i.test(asset.path ?? '')), false);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/pending-approval-not-success.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: the primary Profile status read "${readback.status}" while the exact-operation modal was open, no primary status said Success, and no saved media existed.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), nav: await ev(`[...document.querySelectorAll('.studio-surface-nav button')].map(e=>({text:e.textContent?.trim(),disabled:e.disabled,title:e.title,active:e.getAttribute('aria-current')}))`), calls: await main.evaluate('globalThis.__w119?.calls'), blocked: await main.evaluate('globalThis.__w119?.blocked'), settled: await main.evaluate('globalThis.__w119?.settled') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.exit(0)`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/pending-approval-not-success.png`},null,2));
}
process.exitCode = result.exit;
