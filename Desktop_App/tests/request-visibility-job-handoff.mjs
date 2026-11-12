import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { CDP, until } from '/source/Studio/Desktop_App/tests/ui-tracer.mjs';

const app = '/source/Studio/Desktop_App';
const out = `${app}/evidence/w1-17/runtime-380`;
const root = `${out}/disposable-project`;
const command = "docker exec studio-dev-workbench-1 bash -lc 'cd /source/Studio/Desktop_App && xvfb-run -a node tests/request-visibility-job-handoff.mjs'";
const expectedWorking = 'Submitted · Working — Preparing assembly…';
const expectedQueued = 'Submitted · Queued job — Preparing assembly…';
const expectedIndicator = '1 active job · Queued — Preparing assembly…';
const statePath = `${root}/Preproduction/workflow_state.json`;
fs.rmSync(out, { recursive: true, force: true });
for (const dir of ['Preproduction', 'Scripts', 'Scene_Prompts/Scene1', 'Scenes/Scene1/Cut1']) fs.mkdirSync(`${root}/${dir}`, { recursive: true });
fs.writeFileSync(`${root}/Scripts/script.md`, '# Disposable story\n\n## Scene 1 — W1 native handoff\nAlex: This is a bounded offline assembly fixture.\n');
fs.writeFileSync(`${root}/Scene_Prompts/Scene1/Cut1.md`, 'source_scene: 1\ncut: 1\ntitle: Offline shot\nscene_type: b_roll\ncontinuity:\n  group: test\n  relation_to_previous: new_setup\nediting:\n  transition_in: cut\n  transition_out: cut\nspeech:\n  mode: none\nprompt: bounded offline fixture\n');
const ffmpeg = spawnSync('ffmpeg', ['-v', 'error', '-f', 'lavfi', '-i', 'color=c=blue:s=160x90:r=24:d=1', '-c:v', 'libx264', '-threads', '1', '-pix_fmt', 'yuv420p', `${root}/Scenes/Scene1/Cut1/take.mp4`]);
assert.equal(ffmpeg.status, 0, ffmpeg.stderr?.toString());


const result = {
  check: 'request_visibility_job_handoff',
  expected: 'The same visible activity changes from request preparation to an authoritative queued native job without falsely finishing.',
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
      WORKBENCH_HOST_BIN: '/runtime/target/debug/workbench-host',
      XDG_CONFIG_HOME: '/runtime/w1-17-config',
      XDG_CACHE_HOME: '/runtime/w1-17-cache',
      XDG_DATA_HOME: '/runtime/w1-17-data',
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
    const nodeFs=process.getBuiltinModule('fs');
    BrowserWindow.getAllWindows()[0].setSize(1600,1200);
    dialog.showOpenDialog=async()=>({canceled:false,filePaths:[${JSON.stringify(root)}]});
    globalThis.__w117={calls:[],blocked:[],resolve:null,settled:false,authoritativeJob:null,fixtureReady:false};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd==='workflow_import' && !globalThis.__w117.fixtureReady){
        const answer=await original(...args);
        const scene=answer.state.scenes[0], cut=scene?.cuts?.[0];
        if(!scene || !cut) throw Error('Native import did not create the disposable scene/cut');
        cut.production_note_path='Scene_Prompts/Scene1/Cut1.md';
        cut.visual_source={strategy:'no_start_frame'};
        cut.takes=[{id:'take-1',path:'Scenes/Scene1/Cut1/take.mp4',status:'selected'}];
        cut.selected_take='take-1';
        answer.state.assets=[...(answer.state.assets||[]),{id:'take-1',path:'Scenes/Scene1/Cut1/take.mp4',role:'video_take',status:'present',scene_id:scene.id,cut_id:cut.id}];
        scene.animatic={id:'anim-1',approved:true,shots:[{id:'anim-shot-1',cut_id:cut.id,duration_seconds:1,trim_in_seconds:0,trim_out_seconds:0,transition:'cut'}]};
        nodeFs.writeFileSync(${JSON.stringify(statePath)},JSON.stringify(answer.state,null,2));
        globalThis.__w117.fixtureReady=true;
        return answer;
      }
      if(cmd==='workflow_prepare_assemble'){
        globalThis.__w117.calls.push({cmd,payload});
        await new Promise(resolve=>{globalThis.__w117.resolve=()=>{globalThis.__w117.settled=true;resolve();};});
        const answer=await original(...args);
        globalThis.__w117.authoritativeJob=answer.state.jobs.at(-1);
        return answer;
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w117.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await until('scene imported', () => ev(`document.querySelector('.workflow-scenes button')?.textContent?.includes('W1 native handoff')`));
  await click(`[...document.querySelectorAll('.studio-finish-actions button')].find(e => e.textContent?.trim() === 'Prepare assembly')`);
  await until('host request held', () => main.evaluate(`globalThis.__w117.calls.length === 1 && typeof globalThis.__w117.resolve === 'function' && !globalThis.__w117.settled`));
  await until('stage-local working receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="working"] strong')?.textContent === ${JSON.stringify(expectedWorking)}`));
  await main.evaluate('globalThis.__w117.resolve()');
  await until('queued native job receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="queued"] strong')?.textContent === ${JSON.stringify(expectedQueued)}`));
  await until('queued global indicator', () => ev(`document.querySelector('[data-global-request-indicator]')?.textContent?.trim() === ${JSON.stringify(expectedIndicator)}`));

  const readback = await ev(`(() => {
    const workspace=document.querySelector('.workflow-main');
    const receipt=workspace.querySelector(':scope > .workflow-request-receipts [data-request-outcome="queued"]');
    const heading=receipt.querySelector('strong');
    const detail=receipt.querySelector('p');
    const stage=workspace.querySelector('.workflow-production-notes');
    const activeSurface=[...workspace.querySelectorAll('.studio-surface-nav button[aria-current="page"]')].map(e=>[...e.children].find(c=>c.textContent?.trim()==='Production Notes')?.textContent?.trim()).filter(Boolean);
    const style=getComputedStyle(receipt), r=receipt.getBoundingClientRect();
    return {
      activeSurface,
      status:heading.textContent,
      indicatorText:document.querySelector('[data-global-request-indicator]')?.textContent?.trim() ?? null,
      jobId:receipt.dataset.requestJobId ?? null,
      detail:detail.textContent,
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
  result.hostCalls = await main.evaluate('globalThis.__w117.calls');
  result.network = { blockedRemoteRequests: await main.evaluate('globalThis.__w117.blocked'), hostSettled: await main.evaluate('globalThis.__w117.settled') };
  const persistedState = JSON.parse(fs.readFileSync(statePath, 'utf8'));
  const persistedJob = persistedState.jobs.find(job => job.id === readback.jobId);
  assert.deepEqual(readback.activeSurface, ['Production Notes']);
  assert.equal(readback.status, expectedQueued);
  assert.equal(readback.indicatorText, expectedIndicator);
  assert.match(readback.jobId, /^job_\d+$/);
  assert.equal(readback.detail, `Scene 1 · Queued native job ${readback.jobId}. Jobs owns authoritative progress.`);
  assert.equal(readback.outcome, 'queued');
  assert.equal(readback.role, 'status');
  assert.equal(readback.localToWorkspace, true);
  assert.equal(readback.receiptBeforeStage, true);
  assert.equal(readback.visible, true);
  assert.equal(result.hostCalls.length, 1);
  assert.equal(result.hostCalls[0].cmd, 'workflow_prepare_assemble');
  assert.equal(result.hostCalls[0].payload.sceneId, 1);
  assert.equal(result.network.hostSettled, true);
  assert.equal(persistedJob?.status, 'pending');
  assert.equal(persistedJob?.kind, 'assemble');
  assert.equal((await main.evaluate('globalThis.__w117.authoritativeJob')).id, persistedJob.id);
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/request-to-queued-job.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: the same visible activity changed to "${readback.status}" after the native host persisted queued assembly job ${readback.jobId}, without showing Success.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), nav: await ev(`[...document.querySelectorAll('.studio-surface-nav button')].map(e=>({text:e.textContent?.trim(),disabled:e.disabled,title:e.title,active:e.getAttribute('aria-current')}))`), calls: await main.evaluate('globalThis.__w117?.calls'), blocked: await main.evaluate('globalThis.__w117?.blocked'), settled: await main.evaluate('globalThis.__w117?.settled') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`globalThis.__w117?.resolve?.(); process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.exit(0)`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/request-to-queued-job.png`},null,2));
}
process.exitCode = result.exit;
