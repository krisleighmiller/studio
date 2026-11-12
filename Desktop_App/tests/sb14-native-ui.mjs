// Real application/native picker scenario using the existing owned Electron tracer.
import fs from 'node:fs';import path from 'node:path';import assert from 'node:assert/strict';import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {run, nativeOptions} from './helpers/native-runner.mjs';
import {appRoot} from './helpers/runtime.mjs';
let project,source,original,jobs;
const scenario={
 async setup({out,env}){
  project=path.join(out,'project');fs.mkdirSync(path.join(project,'Preproduction'),{recursive:true});
  const state=JSON.parse(fs.readFileSync(path.join(appRoot,'tests/fixtures/sb14-attach-initial-state.json')));
  fs.writeFileSync(path.join(project,'Preproduction/workflow_state.json'),JSON.stringify(state));jobs=state.jobs;
  source=path.join(out,'external.png');fs.copyFileSync(path.join(appRoot,'tests/fixtures/sb14-external.png'),source);original=fs.readFileSync(source);
  return {project};
 },
 async exercise({page,main,report,save,wait,out}){
  const read=()=>JSON.parse(fs.readFileSync(path.join(project,'Preproduction/workflow_state.json')));
  await main.evaluate(`(()=>{const w=__t01Electron.BrowserWindow.getAllWindows()[0];w.setSize(1380,1050);w.webContents.setBackgroundThrottling(false);w.show();w.focus();return true;})()`);
  const click=async expression=>{const p=await page.evaluate(`(()=>{const e=${expression};if(!e||e.disabled)throw Error('Missing enabled target');e.scrollIntoView({block:'center',behavior:'instant'});const r=e.getBoundingClientRect();const x=r.x+r.width/2,y=r.y+r.height/2;return {x,y,text:e.textContent,hit:document.elementFromPoint(x,y)?.textContent};})()`);await page.call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',clickCount:1,x:p.x,y:p.y});await page.call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',clickCount:1,x:p.x,y:p.y});return p;};
  await wait('scene look navigation',()=>page.evaluate(`Array.from(document.querySelectorAll('.studio-surface-nav button')).some(b=>b.textContent.includes('Scene Look')&&!b.disabled)`));
  await click(`Array.from(document.querySelectorAll('.studio-surface-nav button')).find(b=>b.textContent.includes('Scene Look'))`);
  await wait('empty board attach',()=>page.evaluate(`!!document.querySelector('[aria-label="New look slot role"]')`));
  report.sb14={before:read(),attachments:[],providerCalls:0};
  for(const name of ['empty','populated']){
   const before=read();const count=before.assets.length;
   const clicked=await click(`Array.from(document.querySelectorAll('.scene-look-workspace button')).find(b=>b.textContent==='Attach existing image')`);
   const picker=JSON.parse(execFileSync('/usr/bin/python3',[path.join(appRoot,'tests/sb14-picker.py'),source],{encoding:'utf8',timeout:20000}));
   const desktop=await main.evaluate(`__t01Electron.desktopCapturer.getSources({types:['screen'],thumbnailSize:{width:1380,height:1050}}).then(s=>s[0].thumbnail.toPNG().toString('base64'))`);
   fs.writeFileSync(path.join(out,name+'-picker.png'),Buffer.from(desktop,'base64'));
   await wait('copied/associated native image',()=>read().assets.length===count+1);
   const state=read(),asset=state.assets.at(-1),slot=state.scenes[0].look_board.slots[0];
   assert(slot.asset_ids.includes(asset.id));assert.equal(asset.status,'draft');assert.equal(asset.cut_id??null,null);assert.deepEqual(state.jobs,jobs);
   assert.deepEqual(fs.readFileSync(source),original);assert.deepEqual(fs.readFileSync(path.join(project,asset.path)),original);
   await wait('selected actual decoded image, UI settled',()=>page.evaluate(`(()=>{const i=document.querySelector('.scene-look-stage img');return i?.complete&&i.naturalWidth===32&&i.naturalHeight===48&&i.dataset.lookPath.endsWith(${JSON.stringify(asset.path)})&&!Array.from(document.querySelectorAll('.scene-look-workspace button')).find(b=>b.textContent==='Attach existing image')?.disabled;})()`));
   const visible=await page.evaluate(`(()=>{const i=document.querySelector('.scene-look-stage img');i.scrollIntoView({block:'center',behavior:'instant'});const r=i.getBoundingClientRect();return {path:i.dataset.lookPath,url:i.src,width:i.naturalWidth,height:i.naturalHeight,display:getComputedStyle(i).display,rect:{x:r.x,y:r.y,w:r.width,h:r.height},slot:document.querySelector('.scene-look-slot.active')?.textContent,jobsOpen:!!document.querySelector('.jobs-drawer')};})()`);
   assert(visible.rect.w>0&&visible.rect.h>0);assert.equal(visible.display,'block');
   fs.writeFileSync(path.join(out,name+'.png'),Buffer.from((await page.call('Page.captureScreenshot',{format:'png'})).data,'base64'));
   report.sb14.attachments.push({name,clicked,picker,asset,slot,visible,externalUnchanged:true,jobsUnchanged:true});save('sb14-readback.json',report.sb14);
  }
  assert.equal(report.requests.length,0);assert.equal(report.deniedNetwork.length,0);
  report.sb14.final=read();report.sb14.externalUnchanged=true;save('sb14-readback.json',report.sb14);
 }
};
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
 try {
  const args=process.argv.slice(2);
  const injectFailure=args.includes('--inject-failure');
  const positional=args.filter(arg=>arg!=='--inject-failure');
  assert(positional.length<=1 && positional.every(arg=>!arg.startsWith('--')), 'Usage: node tests/sb14-native-ui.mjs [NEW_OUTPUT_DIRECTORY] [--inject-failure]');
  process.exitCode=await run(positional[0] ? path.resolve(positional[0]) : process.env.STUDIO_TEST_OUT,scenario,{...nativeOptions(),injectFailure});
 } catch(error) { console.error(error);process.exitCode=1; }
}
