import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from './helpers/ui.mjs';
import { appRoot as app, prepareOutput } from './helpers/runtime.mjs';

const out = prepareOutput('request-visibility-model-sheet-local');
const root = `${out}/disposable-project`;
const characterName = 'W1 Model Sheet Character';
const characterId = 'w1_model_sheet_character';
const editedPrompt = 'Edited offline identity prompt for the held W1-07 request.';
const statePath = `${root}/Preproduction/workflow_state.json`;
const command = [process.execPath, ...process.execArgv, ...process.argv.slice(1)].map(arg => JSON.stringify(arg)).join(' ');
const expectedStatus = 'Submitted · Working — Preparing model sheet';
fs.mkdirSync(`${root}/Characters/${characterName}`, { recursive: true });
fs.mkdirSync(path.dirname(statePath), { recursive: true });
fs.writeFileSync(`${root}/Characters/${characterName}/character.yaml`, `profile_version: 1\nname: ${characterName}\ndescription: Disposable offline W1-07 fixture\nidentity_prompt: Original fixture identity prompt\n`);
fs.writeFileSync(statePath, JSON.stringify({
  version: 2,
  state_revision: 1,
  project_name: 'w1-model-sheet-local',
  scripts: [],
  characters: [{
    id: characterId,
    name: characterName,
    dir: `Characters/${characterName}`,
    profile_path: `Characters/${characterName}/character.yaml`,
    identity_ready: false,
    voice_ready: false,
    profile: {
      description: 'Disposable offline W1-07 fixture',
      appearance: ['adult with short dark hair'],
      identity_prompt: 'Original fixture identity prompt',
    },
    reference_asset_ids: [],
    asset_ids: [],
  }],
  scenes: [], assets: [], jobs: [], notes: [], settings: {},
}, null, 2));

const result = {
  check: 'request_visibility_model_sheet_local',
  expected: 'While start_character_model_sheet remains unresolved after the edited prompt is submitted, Characters shows a prominent model-sheet Working receipt.',
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
  const point = await ev(`(() => { const e=${source}; if(!e || e.disabled || !e.getClientRects().length) throw Error('Missing visible enabled target'); e.scrollIntoView({block:'center',behavior:'instant'}); const r=e.getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()`);
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
    globalThis.__w107={calls:[],blocked:[],resolve:null,settled:false};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd==='start_character_model_sheet'){
        globalThis.__w107.calls.push({cmd,payload});
        return await new Promise(resolve=>{globalThis.__w107.resolve=value=>{globalThis.__w107.settled=true;resolve(value);};});
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w107.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await click(`[...document.querySelectorAll('.studio-surface-nav button')].find(e => e.textContent?.trim().startsWith('Characters'))`);
  await until('Characters stage', () => ev(`document.querySelector('.workflow-character-card')?.getClientRects().length > 0`));
  await click(`[...document.querySelectorAll('.workflow-character-card .workflow-inspector-actions button')].find(e => e.textContent?.trim() === 'Create full 12-pose sheet')`);
  await until('prompt editor', () => ev(`document.querySelector('dialog[data-generation-prompt] textarea')?.getClientRects().length > 0`));
  await ev(`(() => { const e=document.querySelector('dialog[data-generation-prompt] textarea'); e.value=${JSON.stringify(editedPrompt)}; e.dispatchEvent(new Event('input',{bubbles:true})); })()`);
  await click(`document.querySelector('dialog[data-generation-prompt] button[type="submit"]')`);
  await until('host request held', () => main.evaluate(`globalThis.__w107.calls.length === 1 && typeof globalThis.__w107.resolve === 'function' && !globalThis.__w107.settled`));
  await until('Characters local working receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="working"] strong')?.textContent === ${JSON.stringify(expectedStatus)}`));

  const readback = await ev(`(() => {
    const workspace=document.querySelector('.workflow-main');
    const receipt=workspace.querySelector(':scope > .workflow-request-receipts [data-request-outcome="working"]');
    const heading=receipt.querySelector('strong');
    const detail=receipt.querySelector('p');
    const prompt=receipt.querySelector('pre');
    const stage=workspace.querySelector('.workflow-characters');
    const activeSurface=[...workspace.querySelectorAll('.studio-surface-nav button[aria-current="page"]')].map(e=>[...e.children].find(c=>c.textContent?.trim()==='Characters')?.textContent?.trim()).filter(Boolean);
    const style=getComputedStyle(receipt), r=receipt.getBoundingClientRect();
    return {
      activeSurface,
      status:heading.textContent,
      detail:detail.textContent,
      submittedText:prompt?.textContent ?? null,
      outcome:receipt.dataset.requestOutcome,
      role:receipt.getAttribute('role'),
      localToWorkspace:receipt.closest('.workflow-main')===workspace,
      receiptBeforeStage:Boolean(stage && (receipt.compareDocumentPosition(stage)&Node.DOCUMENT_POSITION_FOLLOWING)),
      visible:receipt.getClientRects().length>0 && r.width>0 && r.height>0 && style.visibility==='visible' && style.display!=='none' && Number(style.opacity)>0,
      rect:{x:r.x,y:r.y,width:r.width,height:r.height,bottom:r.bottom,right:r.right},
      viewport:{width:innerWidth,height:innerHeight,scrollX,scrollY},
    };
  })()`);
  result.readback = readback;
  result.hostCalls = await main.evaluate('globalThis.__w107.calls');
  result.network = { blockedRemoteRequests: await main.evaluate('globalThis.__w107.blocked'), hostSettled: await main.evaluate('globalThis.__w107.settled') };
  assert.deepEqual(readback.activeSurface, ['Characters']);
  assert.equal(readback.status, expectedStatus);
  assert.equal(readback.detail, 'Work is continuing. Keep Studio open while the host responds.');
  assert.equal(readback.submittedText, `prompt: ${editedPrompt}`);
  assert.equal(readback.outcome, 'working');
  assert.equal(readback.role, 'status');
  assert.equal(readback.localToWorkspace, true);
  assert.equal(readback.receiptBeforeStage, true);
  assert.equal(readback.visible, true);
  assert.equal(result.hostCalls.length, 1);
  assert.equal(result.hostCalls[0].cmd, 'start_character_model_sheet');
  assert.equal(result.hostCalls[0].payload.request.inputs.identity_text, editedPrompt);
  assert.equal(result.network.hostSettled, false);
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/model-sheet-local-working.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: Characters displayed the visible local status "${readback.status}" after submitting the edited prompt while exactly one start_character_model_sheet call remained unresolved; no remote request occurred.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), nav: await ev(`[...document.querySelectorAll('.studio-surface-nav button')].map(e=>({text:e.textContent?.trim(),disabled:e.disabled,title:e.title,active:e.getAttribute('aria-current')}))`), calls: await main.evaluate('globalThis.__w107?.calls'), blocked: await main.evaluate('globalThis.__w107?.blocked'), settled: await main.evaluate('globalThis.__w107?.settled') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`globalThis.__w107?.resolve?.({pendingAction:'[[workbench_approve:offlineW107]]'}); process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.exit(0)`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/model-sheet-local-working.png`},null,2));
}
process.exitCode = result.exit;
