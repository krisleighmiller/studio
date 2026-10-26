#!/usr/bin/env node
/** A01 copied-media scenario on T01's owned real Electron/UI/host tracer. */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';

import { run } from './ui-tracer.mjs';

const project = '/home/kris/Content_Creation/Projects/Studio_Recovery_Black_Hole';
const evidence = '/home/kris/.hermes/profiles/studio/kanban-plans/current-roadmap/execution';
const sha = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
export function assemblyInputs(label, revision) {
  const name = revision ? 'revision' : 'baseline';
  return { production_path: 'prod.md', scenes: [{ id: 1, video: 'Scenes/Scene1/SC1.mp4' }, { id: 2, video: 'Scenes/Scene2/SC2.mp4' }],
    trim: 'estimated', caption_mode: 'none', overwrite: false,
    output_path: `Final/${label}/black_hole_opening_${name}.mp4`, work_dir: `.p01/work/${label}/${name}`,
    ...(revision ? { fade_in: false } : {}) };
}
function verifyInputs() {
  const manifest = JSON.parse(fs.readFileSync(path.join(evidence, 'P01/fixture-manifest.json')));
  return [...manifest.files,{path:manifest.state_path,sha256:manifest.state_sha256}].map(item => {
    const file = path.join(project, item.path); const stat = fs.statSync(file, { bigint: true });
    assert.equal(fs.realpathSync(file), file); assert.equal(sha(file), item.sha256);
    return { path: file, sha256: sha(file), size: String(stat.size), mtimeNs: String(stat.mtimeNs) };
  });
}
export function containedPath(root, relative) {
  assert(!path.isAbsolute(relative) && !relative.split('/').includes('..'), 'Relative confined path required');
  assert.equal(fs.realpathSync(root),root,'Fixture root symlink');
  const full=path.join(root,relative);
  let current=root;
  for(const part of relative.split('/').filter(Boolean)) {
    current=path.join(current,part);
    try { fs.lstatSync(current); } catch(e) { if(e.code==='ENOENT') continue; throw e; }
    assert.equal(fs.realpathSync(current),current,'Redirected fixture path');
  }
  return full;
}
export function assertHistoryReturn(history, original, result, output) {
  assert(history.some(m=>m.role==='user' && m.content===original),'Original task lost');
  assert(result && result.toolEvents?.includes('approve:assemble') && result.reply.startsWith('Assemble finished (host-run).') && result.reply.includes(output),'Observed current completion missing');
  assert(history.some(m=>m.role==='assistant' && m.content===result.reply),'Exact current execution result missing from history');
  assert(!history.some(m=>m.role==='user' && /^(continue|resume)$/i.test(m.content.trim())),'Invented user continuation');
}
function emptyWork(file) { return !fs.existsSync(file) || fs.readdirSync(file).length === 0; }
async function screenshot(page, out, name) {
  const shot = await page.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(path.join(out, name+'.png'), Buffer.from(shot.data, 'base64'));
}
async function submit(page, intent) {
  return page.evaluate(`(() => { const f=document.querySelector('#chat-form'), input=document.querySelector('#chat-input'), send=document.querySelector('#chat-send-btn'); if (!f||!input||!send||send.disabled) throw Error('Chat form unavailable'); input.value=${JSON.stringify(intent)}; input.dispatchEvent(new Event('input',{bubbles:true})); f.requestSubmit(send); return {form:f.id,submitter:send.id,intent:${JSON.stringify(intent)}}; })()`);
}
async function clickVisible(page, selector) {
  const box = await page.evaluate(`(() => { const e=${selector}; if(!e || e.disabled) throw Error('No enabled target'); e.scrollIntoView({block:'center'}); const r=e.getBoundingClientRect(), s=getComputedStyle(e); if(r.width<=0||r.height<=0||s.visibility==='hidden'||s.display==='none') throw Error('Target not visible'); return {x:r.x+r.width/2,y:r.y+r.height/2,text:e.textContent}; })()`);
  await page.call('Input.dispatchMouseEvent', { type:'mousePressed', button:'left', clickCount:1, x:box.x,y:box.y });
  await page.call('Input.dispatchMouseEvent', { type:'mouseReleased', button:'left', clickCount:1, x:box.x,y:box.y });
  return box;
}
export function scenario(label) {
  assert(/^[a-z0-9-]+$/.test(label));
  let stage = 0; let before;
  const baseline = assemblyInputs(label, false), revision = assemblyInputs(label, true);
  const intents = [
    `A01 local copied-media task: inspect script.md and prod.md, then assemble the selected Mom/Sam shots in order (script Scene 1 = production shots 1–2). Use Scenes/Scene1/SC1.mp4 source [0,4.8) and Scenes/Scene2/SC2.mp4 source [1,5.6), estimated trim, their embedded source_video_lipsync audio, no captions/music and overwrite=false. Save ${baseline.output_path} with work ${baseline.work_dir}. Omit fade_in and fade_out to preserve production defaults. Do not edit any source, reference, map or production notes. After my visible approval, report the actual render result and original task without rerunning it.`,
    `Revise that completed A01 Mom/Sam assembly by removing ONLY the opening host picture/audio fade: fade_in=false. Omit fade_out and preserve the current closing cut (false), exact selected takes/order/windows/estimated trim/embedded audio/no captions/no music/overwrite=false. Export separately as ${revision.output_path} with work ${revision.work_dir}. Do not edit prod.md, a map, source or reference files. After my fresh visible approval, report the actual revised result and original task without rerunning either export.`
  ];
  return {

    async setup({out, env, report}) {
      before = verifyInputs();
      report.assemblyScenarioIdentity={path:import.meta.filename,sha256:sha(import.meta.filename)};
      fs.writeFileSync(path.join(out,'inputs-before.json'),JSON.stringify(before,null,2));
      const runtime = containedPath(project, `.p01/runs/${label}`);
      // P01 provisioned first/independent run directories may exist empty only.
      assert(emptyWork(runtime), 'Refuse populated runtime reuse');
      for (const p of [baseline, revision]) {
        containedPath(project,p.output_path);containedPath(project,p.work_dir);
        assert(!fs.existsSync(path.join(project,p.output_path)), 'Refuse existing output');
        assert(emptyWork(path.join(project,p.work_dir)), 'Refuse populated work');
      }
      Object.assign(env,{ HOME:path.join(runtime,'home'), TMPDIR:path.join(runtime,'tmp'), XDG_CACHE_HOME:path.join(runtime,'cache'), XDG_DATA_HOME:path.join(runtime,'data') });
      for(const p of [env.HOME,env.XDG_CACHE_HOME,env.XDG_DATA_HOME]) fs.mkdirSync(p,{recursive:true});
      report.assembly = { label, inputs:[baseline,revision], intents, runtime };
      return {project,userData:path.join(runtime,'userData'),xdgConfig:path.join(runtime,'xdg')};
    },
    async plan({record, report, save}) {
      const n = ++stage;
      const action = n===1 ? {action:'inspect',op:'read_file',path:'script.md'}
        : n===2 ? {action:'inspect',op:'read_file',path:'prod.md'}
        : n===3 ? {action:'run_specialist',specialist:'assemble',inputs:baseline}
        : n===4 ? {action:'done',message:'A01_BASELINE_RETURNED: completed original Mom/Sam baseline task; actual assembly result retained in history.'}
        : n===5 ? {action:'run_specialist',specialist:'assemble',inputs:revision}
        : n===6 ? {action:'done',message:'A01_REVISION_RETURNED: completed original Mom/Sam task and opening-only revision; actual assembly results retained in history.'}
        : null;
      assert(action, `Unexpected duplicate planner request ${n}`);
      if(n===4 || n===6) {
        const history=record.body.messages, output=n===4?baseline.output_path:revision.output_path;
        const actual=report.observed.hostResponses?.find(r=>r.toolEvents?.includes('approve:assemble') && r.reply.includes(output));
        assertHistoryReturn(history,intents[0],actual,output);
        report.assembly.historyReturns ??= [];
        report.assembly.historyReturns.push({n,messages:history});
      }
      if(n===3) {
        const observed=JSON.stringify(record.body.messages.filter(m=>m.content.startsWith('Workbench host results (authoritative')));
        for(const binding of ['S01_a','S02_a','Mom/mom_vert.png','Sam/sam_vert.png','Oooh','funerals']) assert(observed.includes(binding),`Missing inspected script/reference binding ${binding}`);
      }
      (report.assembly.actions ??= []).push(action); save('assembly-actions.json',report.assembly.actions);
      return action;
    },
    async exercise({page,main,report,save,wait,out}) {
      report.assembly.playbackFixture = await main.evaluate(`(() => {const w=__t01Electron.BrowserWindow.getAllWindows().find(w=>w.webContents.getURL()==='http://127.0.0.1:1420/');w.webContents.setBackgroundThrottling(false);w.show();w.focus();return {visible:w.isVisible(),minimized:w.isMinimized(),backgroundThrottling:w.webContents.getBackgroundThrottling()};})()`);
      report.assembly.submissions=[]; report.assembly.approvals=[]; report.assembly.playback=[];
      let baselineIdentity;
      for(let i=0;i<2;i++) {
        const p=i===0?baseline:revision, name=i===0?'baseline':'revision';
        report.assembly.submissions.push(await submit(page,intents[i]));
        await wait(`${name} visible approval`,async()=>await page.evaluate(`Array.from(document.querySelectorAll('#chat-messages button')).some(e=>e.textContent==='Approve')`),60000);
        const card=report.observed.hostResponses?.find(r=>r.reply.includes(`black_hole_opening_${name}.mp4`) && r.reply.includes('[[workbench_approve:'));
        assert(card, 'Real host approval envelope missing');
        assert.deepEqual(card.toolEvents,i===0?['inspect:read_file','inspect:read_file','approval_required:assemble']:['approval_required:assemble']);
        assert(card.reply.includes(`edge fades: in=${i===0} out=false`),card.reply);
        assert(!fs.existsSync(path.join(project,p.output_path)), 'Render before approval');
        assert(emptyWork(path.join(project,p.work_dir)), 'Work artifact before approval');
        const beforeApproval={outputExists:false,workContents:fs.existsSync(path.join(project,p.work_dir))?fs.readdirSync(path.join(project,p.work_dir)):[],inputs:verifyInputs()};
        await screenshot(page,out,name+'-approval');
        const click=await clickVisible(page,`Array.from(document.querySelectorAll('#chat-messages button')).find(e=>e.textContent==='Approve')`);
        report.assembly.approvals.push({name,card,beforeApproval,click}); save('approvals.json',report.assembly.approvals);
        const marker=i===0?'A01_BASELINE_RETURNED':'A01_REVISION_RETURNED';
        await wait(`${name} render and automatic continuation`,async()=>await page.evaluate(`document.querySelector('#chat-messages').innerText.includes(${JSON.stringify(marker)}) && !document.querySelector('#chat-send-btn').disabled`),180000);
        assert.equal(stage,i===0?4:6,'No replay or extra planner action');
        assert.equal(report.observed.hostResponses.filter(r=>r.toolEvents?.includes('approval_required:assemble') && r.reply.includes(p.output_path)).length,1,'Duplicate approval');
        assert.equal(report.observed.hostResponses.filter(r=>r.toolEvents?.includes('approve:assemble') && r.reply.includes(p.output_path)).length,1,'Duplicate execution');
        const result=report.observed.hostResponses.find(r=>(r.toolEvents??[]).includes('approve:assemble') && r.reply.includes(`black_hole_opening_${name}.mp4`));
        assert(result,'Actual approve:assemble result missing'); assert.equal(result.continueSuggested,false); // Renderer resumes a completed approve:assemble from its authoritative event, not this legacy specialist flag.
        assert(fs.statSync(path.join(project,p.output_path)).size>0);
        if(i===0) baselineIdentity=sha(path.join(project,p.output_path));
        else assert.equal(sha(path.join(project,baseline.output_path)),baselineIdentity,'Baseline changed during revision');
        assert.equal(await page.evaluate(`document.querySelectorAll('#chat-messages button').length`),0,'Consumed approval still visible');
        await screenshot(page,out,name+'-terminal');
        // Actual existing Files/Explorer activation, then its existing HTML video.
        await page.evaluate(`document.querySelector('[data-surface="files"]').click()`);
        const full=path.join(project,p.output_path);
        for(const rel of ['Final',`Final/${label}`,p.output_path]) {
          const target=path.join(project,rel);
          await wait('Explorer row '+rel,async()=>await page.evaluate(`Array.from(document.querySelectorAll('.tree-item')).some(e=>e.dataset.path===${JSON.stringify(target)})`));
          const expanded = rel!==p.output_path && await page.evaluate(`Array.from(document.querySelectorAll('.tree-item')).find(e=>e.dataset.path===${JSON.stringify(target)})?.querySelector('.chevron')?.textContent==='▼'`);
          if(!expanded) await clickVisible(page,`Array.from(document.querySelectorAll('.tree-item')).find(e=>e.dataset.path===${JSON.stringify(target)})`);
        }
        await wait('Actual app video loaded',async()=>await page.evaluate(`document.querySelector('#media-video').readyState>=2 && document.querySelector('#video-title').textContent===${JSON.stringify(path.basename(full))}`),30000);
        const start=await page.evaluate(`(() => {const v=document.querySelector('#media-video');return {src:v.currentSrc,duration:v.duration,width:v.videoWidth,height:v.videoHeight,readyState:v.readyState,time:v.currentTime,muted:v.muted};})()`);
        assert(start.src.startsWith('workbench-media:'),'Not app media protocol');
        await page.evaluate(`document.querySelector('#media-video').play().then(()=>true)`);
        await wait('Video advances',async()=>await page.evaluate(`document.querySelector('#media-video').currentTime>1 && !document.querySelector('#media-video').paused`),15000);
        await screenshot(page,out,name+'-playback');
        await wait('Video plays to end',async()=>await page.evaluate(`document.querySelector('#media-video').ended`),20000);
        const end=await page.evaluate(`(() => {const v=document.querySelector('#media-video');return {time:v.currentTime,ended:v.ended,error:v.error?.message??null,frames:v.getVideoPlaybackQuality().totalVideoFrames,dropped:v.getVideoPlaybackQuality().droppedVideoFrames};})()`);
        assert.equal(end.error,null);assert(end.frames>100);
        report.assembly.playback.push({name,start,end});save('playback.json',report.assembly.playback);
      }
      assert.equal(report.requests.length,6);assert.deepEqual(report.deniedNetwork,[]);
      assert.equal(report.observed.hostResponses.length,6,'Unexpected host reply/replay');
      assert.notEqual(report.assembly.approvals[0].card.reply.match(/workbench_approve:([A-Za-z0-9]+)/)[1],report.assembly.approvals[1].card.reply.match(/workbench_approve:([A-Za-z0-9]+)/)[1],'Approval reused');
      report.assembly.baselinePreservedDuringRevision=baselineIdentity;
      const after=verifyInputs();assert.deepEqual(after,before);save('inputs-after.json',after);
      const state=path.join(project,'Preproduction/workflow_state.json');
      assert(fs.existsSync(state),'Native imported workflow state missing');
      save('native-workflow-state.json',JSON.parse(fs.readFileSync(state)));
      report.assembly.finalChat=await page.evaluate(`document.querySelector('#chat-messages').innerText`);
      save('assembly-result.json',report.assembly);
    }
  };
}
if(process.argv[1] && path.resolve(process.argv[1])===new URL(import.meta.url).pathname) {
  const [out,label]=process.argv.slice(2);
  try {process.exitCode=await run(out,false,scenario(label));}catch(e){console.error(e);process.exitCode=1;}
}
