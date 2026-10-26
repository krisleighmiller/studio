#!/usr/bin/env node
/** A02 actual Electron/host terminal-provider-failure and approval lifecycle. */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import { execFileSync } from 'node:child_process';
import { run } from './ui-tracer.mjs';

const evidence='/home/kris/.hermes/profiles/studio/kanban-plans/current-roadmap/execution/A02';
const [out]=process.argv.slice(2);
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
let server; const requests=[]; let polls=0; let serverError; let planStage=0;
const prediction='a02-terminal-failed-original';
async function click(page, expression) {
  const box=await page.evaluate(`(()=>{const e=${expression};if(!e||e.disabled)throw Error('Missing enabled target: '+${JSON.stringify(expression)});e.scrollIntoView({block:'center'});const r=e.getBoundingClientRect();if(!r.width||!r.height)throw Error('Hidden target');return {x:r.x+r.width/2,y:r.y+r.height/2,text:e.textContent};})()`);
  for(const type of ['mousePressed','mouseReleased'])await page.call('Input.dispatchMouseEvent',{type,button:'left',clickCount:1,x:box.x,y:box.y});
  return box;
}
const button=text=>`Array.from(document.querySelectorAll('button')).find(e=>e.textContent.trim()===${JSON.stringify(text)})`;
async function shot(page,name){const s=await page.call('Page.captureScreenshot',{format:'png'});fs.writeFileSync(path.join(out,name+'.png'),Buffer.from(s.data,'base64'));}
const scenario={
  async setup({out,env,report,trackServer}) {
    const project=path.join(out,'project');fs.mkdirSync(path.join(project,'Preproduction'),{recursive:true});
    const state={version:2,project_name:'A02 isolated failure fixture',settings:{},characters:[],assets:[],jobs:[],scripts:[],scenes:[{id:1,title:'A02 empty room',stable_id:'a02-scene',cuts:[],stages:{},look_board:{slots:[{id:'s1_look_01',role:'interior_reference',title:'Empty room',prompt:'An empty blue room without people.'}]}}]};
    fs.writeFileSync(path.join(project,'Preproduction/workflow_state.json'),JSON.stringify(state));
    fs.writeFileSync(path.join(project,'README.md'),'A02 synthetic loopback fixture. No original project inputs.\n');
    fs.writeFileSync(path.join(project,'prod.md'),'### Scene 1\n```yaml\nspeech:\n  mode: none\n```\n');
    execFileSync('/usr/bin/ffmpeg',['-v','error','-f','lavfi','-i','color=blue:s=90x160:r=24:d=1','-f','lavfi','-i','sine=frequency=440:sample_rate=48000:duration=1','-c:v','libx264','-pix_fmt','yuv420p','-c:a','aac','-shortest',path.join(project,'source.mp4')],{timeout:15000});
    const bin=path.join(out,'fixture-bin');fs.mkdirSync(bin);
    fs.writeFileSync(path.join(bin,'ffmpeg'),`#!/bin/sh\nif test -f '${out}/fail-ffmpeg'; then printf 'A02 injected local encoder failure\\n' >&2; exit 42; fi\nexec /usr/bin/ffmpeg "$@"\n`,{mode:0o700});
    env.PATH=bin+':'+env.PATH;
    server=http.createServer((req,res)=>{(async()=>{
      let body='';for await(const b of req){body+=b;if(body.length>100000)throw Error('Request limit');}
      const item={method:req.method,path:req.url,bytes:body.length,body:body?JSON.parse(body):null,time:Date.now()};requests.push(item);
      fs.writeFileSync(path.join(out,'provider-requests.json'),JSON.stringify(requests,null,2));
      let response;
      if(req.method==='GET'&&req.url==='/api/v1/models')response={data:[{id:'a02/image',name:'A02 image',type:'image'}]};
      else if(req.method==='POST'&&req.url==='/api/v1/model/generateImage')response={data:{id:prediction,status:'starting'}};
      else if(req.method==='GET'&&req.url===`/api/v1/model/prediction/${prediction}`)response={data:{id:prediction,status:++polls===1?'processing':'failed',outputs:[]}};
      else {res.writeHead(403);res.end('A02 FAIL CLOSED: unsupported provider route');return;}
      item.response=response;fs.writeFileSync(path.join(out,'provider-requests.json'),JSON.stringify(requests,null,2));res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify(response));
    })().catch(e=>{serverError=e;res.destroy();});});
    trackServer(server);
    await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(0,'127.0.0.1',resolve);});
    report.ports.media=server.address().port;
    env.WORKBENCH_ATLAS_BASE_URL=`http://127.0.0.1:${server.address().port}/api/v1`;
    Object.assign(env,{HOME:path.join(out,'home'),XDG_CACHE_HOME:path.join(out,'cache'),XDG_DATA_HOME:path.join(out,'data')});
    for(const p of [env.HOME,env.XDG_CACHE_HOME,env.XDG_DATA_HOME])fs.mkdirSync(p,{recursive:true});
    return {project,hostBin:path.join(evidence,'fixture-workbench-host'),configureSettings(settings){settings.capabilityDefaults={'image.text_to_image':{apiId:settings.apis[0].id,modelId:'a02/image'}};settings.apis[0].catalog.push({id:'a02/image',displayName:'A02 Image',type:'Image'});}};
  },
  async plan({report}){
    const n=++planStage;
    assert(n<=4,'No automatic replay or retry');
    if(n===2){assert(report.observed.hostResponses.some(r=>r.toolEvents?.includes('approve:assemble')));return {action:'done',message:'A02_SUCCESS_RETURNED'};}
    const label=n===1?'success':n===3?'error':'ipc';
    return {action:'run_specialist',specialist:'assemble',inputs:{production_path:'prod.md',scenes:[{id:1,video:'source.mp4'}],trim:'full',caption_mode:'none',output_path:`Final/${label}.mp4`,work_dir:`Final/work-${label}`,fade_in:false,fade_out:false,overwrite:false}};
  },
  async exercise({page,main,report,save,wait,project}){
    await main.evaluate(`(()=>{const w=__t01Electron.BrowserWindow.getAllWindows()[0];w.webContents.setBackgroundThrottling(false);w.show();w.focus();return true;})()`);
    report.buildInfo=await page.evaluate(`window.workbench.invoke('host_build_info',{})`);assert.equal(report.buildInfo.mock_provider,true);
    await page.evaluate(`globalThis.__a02Quotes=[];window.workbench.onSpendConfirmationRequired(p=>__a02Quotes.push(p));true`);
    report.initialUI=await page.evaluate('document.body.innerText');save('initial-ui.json',report.initialUI);
    // Navigate through the existing Studio Scene Look surface, not a host command.
    await wait('Scene Look Generate still available',async()=>await page.evaluate(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent.trim()==='Generate still')`));
    await page.evaluate(`Array.from(document.querySelectorAll('details')).forEach(e=>e.open=true)`);
    await click(page,button('Generate still'));
    await wait('visible real host look approval',async()=>await page.evaluate(`!!(${button('Approve')})`));
    assert.equal(requests.filter(r=>r.method==='POST').length,0,'No submit before card approval');await shot(page,'look-approval');
    await click(page,button('Approve'));
    // The second authorization is an actual native warning. Use XTest Return,
    // never override dialog.ask or inject a spend replay.
    await wait('real spend boundary',()=>report.observed.hostResponses?.some(r=>r.toolEvents?.includes('approve:awaiting-spend-authorization')));
    assert.equal(requests.filter(r=>r.method==='POST').length,0);
    execFileSync('/usr/bin/python3',[path.join(report.root,'tests/ui-tracer-native.py'),'keys'],{timeout:15000,env:{...process.env,...report.environment}});
    const statePath=path.join(project,'Preproduction/workflow_state.json');const state=()=>JSON.parse(fs.readFileSync(statePath));
    await wait('failed workflow job and failed spend attempt persisted',()=>state().jobs.some(j=>j.kind==='scene_look'&&j.status==='failed')&&state().jobs.some(j=>j.spend_category&&j.status==='failed'),60000);
    await sleep(2200);if(serverError)throw serverError;
    const failed=state();save('failed-state.json',failed);assert.equal(failed.assets.length,0);
    const jobs=failed.jobs;assert.equal(jobs.length,2);assert(jobs.every(j=>j.status==='failed'));
    const spend=jobs.find(j=>j.spend_category);assert.equal(spend.request_metadata.prediction_id,prediction);assert.equal(spend.actual_verified,false);
    for(const job of jobs)if(job.output_path)assert(!fs.existsSync(path.join(project,job.output_path)),'Unexpected output');
    assert.equal(requests.filter(r=>r.method==='POST').length,1);assert.equal(polls,2,'Polling must stop at terminal failure');
    save('failure-ui.json',await page.evaluate('document.body.innerText'));await shot(page,'failed-terminal');
    // Reopen via the same existing native Recent item to reload authoritative
    // failed jobs; the spend error path does not currently auto-refresh the desk.
    await main.evaluate(`(()=>{const w=__t01Electron.BrowserWindow.getAllWindows()[0];w.show();w.focus();const recent=__t01Electron.Menu.getApplicationMenu().items.find(i=>i.label==='File').submenu.items.find(i=>i.label==='Open Recent').submenu;recent.popup({window:w});return true;})()`);
    execFileSync('/usr/bin/python3',[path.join(report.root,'tests/ui-tracer-native.py'),'keys'],{timeout:15000,env:{...process.env,...report.environment}});
    await wait('reopened authoritative failed jobs',async()=>await page.evaluate(`document.querySelector('.studio-jobs-toggle')?.textContent.includes('2 failed')`));
    assert.equal(polls,2,'Reopen cannot automatically poll/resubmit');
    assert.deepEqual(state().jobs,failed.jobs);
    if(await page.evaluate(`document.querySelector('.studio-jobs-toggle')?.getAttribute('aria-expanded')==='false'`))await click(page,`document.querySelector('.studio-jobs-toggle')`);
    save('jobs-ui.json',await page.evaluate(`document.querySelector('.studio-jobs').innerText`));
    const recovery=`Array.from(document.querySelectorAll('button.studio-job')).find(e=>e.textContent.includes(${JSON.stringify(prediction)})&&e.querySelector('.studio-job-action')?.textContent==='Check provider status')`;
    await wait('original-ID recovery row',async()=>await page.evaluate(`!!(${recovery})`));
    const before=polls;await click(page,recovery);await wait('read-only provider lookup',()=>polls===before+1);
    assert.equal(requests.filter(r=>r.method==='POST').length,1);save('recovery-state.json',state());
    assert.deepEqual(state().jobs,failed.jobs);assert.equal(report.requests.length,0);
    const jobsText=await page.evaluate(`document.querySelector('.studio-jobs').innerText`);
    assert(jobsText.includes('Cancel: Provider support unknown · unavailable in Studio'));
    assert(jobsText.includes('Automatic retry disabled · read-only status recovery available'));
    assert(jobsText.includes('Unsupported · provider outcome unverified; original request unavailable'));
    assert.equal(await page.evaluate(`Array.from(document.querySelectorAll('.studio-jobs button')).some(e=>/^(collect|cancel|resume|retry)$/i.test(e.textContent.trim()))`),false);
    await shot(page,'recovered-original-id');
    report.lifecycle=[];
    for(const kind of ['success','error','ipc']){
      await page.evaluate(`(()=>{const f=document.querySelector('#chat-form'),i=document.querySelector('#chat-input');i.value=${JSON.stringify('A02 local assembly lifecycle test: ')}+${JSON.stringify(kind)};i.dispatchEvent(new Event('input',{bubbles:true}));f.requestSubmit(document.querySelector('#chat-send-btn'));return true;})()`);
      await wait(kind+' approval',async()=>await page.evaluate(`!!(${button('Approve')})`),60000);
      assert(!fs.existsSync(path.join(project,`Final/${kind}.mp4`)));
      const card=report.observed.hostResponses.at(-1);assert(card.reply.includes('[[workbench_approve:'));
      if(kind==='error')fs.writeFileSync(path.join(out,'fail-ffmpeg'),'fixture encoder failure');
      if(kind==='ipc')fs.appendFileSync(path.join(project,'prod.md'),'\nA02 post-approval input mutation.\n');
      await shot(page,kind+'-card');await click(page,button('Approve'));
      await wait(kind+' terminal UI',async()=>await page.evaluate(`!document.querySelector('#chat-send-btn').disabled && document.querySelector('#chat-messages').innerText.includes(${JSON.stringify(kind==='success'?'A02_SUCCESS_RETURNED':kind==='error'?'Host could not execute the approved action':'Approval failed:')})`),120000);
      if(kind==='success')assert(fs.statSync(path.join(project,'Final/success.mp4')).size>0);
      else assert(!fs.existsSync(path.join(project,`Final/${kind}.mp4`)));
      if(kind==='error'){assert(report.observed.hostResponses.at(-1).toolEvents.includes('approve:error'));fs.unlinkSync(path.join(out,'fail-ffmpeg'));}
      const beforeRerender=await page.evaluate(`document.querySelector('#chat-messages').innerText`);
      await click(page,`document.querySelector('#new-chat-tab')`);await click(page,`document.querySelector('#chat-tabs .tab')`);
      assert.equal(await page.evaluate(`!!(${button('Approve')})`),false,'Consumed approval resurrected on rerender');
      assert((await page.evaluate(`document.querySelector('#chat-messages').innerText`)).includes(kind==='success'?'A02_SUCCESS_RETURNED':kind==='error'?'Host could not execute the approved action':'Approval failed:'));
      report.lifecycle.push({kind,card,beforeRerender,afterRerender:await page.evaluate(`document.querySelector('#chat-messages').innerText`)});save('approval-lifecycle.json',report.lifecycle);await shot(page,kind+'-terminal');
    }
    assert.equal(planStage,4);assert.equal(requests.filter(r=>r.method==='POST').length,1);assert.equal(polls,3);
    const native=key=>execFileSync('/usr/bin/python3',[path.join(report.root,'tests/ui-tracer-native.py'),'keys',key],{timeout:15000,env:{...process.env,...report.environment}});
    // A changed ledger invalidates the displayed native quote. This fixture
    // mutation changes no numeric limit and never relaxes a strict ceiling.
    for(const fresh of [false,true]){
      await page.evaluate(`Array.from(document.querySelectorAll('details')).forEach(e=>e.open=true)`);
      await click(page,button('Generate still'));await wait('new look approval',async()=>await page.evaluate(`!!(${button('Approve')})`));
      await click(page,button('Approve'));
      const quoteCount=fresh?3:2;
      await wait('fresh native quote '+quoteCount,async()=>await page.evaluate(`__a02Quotes.length===${quoteCount}`));
      const quotes=await page.evaluate('__a02Quotes');save('spend-quotes.json',quotes);
      if(!fresh){const changed=state();changed.budget={currency:'USD',limit_minor:null,hard_stop:true,policy:'strict_hard_stop'};fs.writeFileSync(statePath,JSON.stringify(changed));native('Return');
        await wait('stale quote rejected visibly',async()=>await page.evaluate(`document.querySelector('#status-left').innerText.includes('NOT dispatched')`));
      }else{assert.notEqual(quotes[1].quote.ledgerRevision,quotes[2].quote.ledgerRevision);native('Escape');
        await wait('fresh authorization declined',async()=>await page.evaluate(`document.querySelector('#status-left').innerText.includes('not authorized')`));}
      assert.equal(requests.filter(r=>r.method==='POST').length,1,'No submit from stale/declined quote');
    }
    await main.evaluate(`(()=>{const w=__t01Electron.BrowserWindow.getAllWindows()[0];w.show();w.focus();__t01Electron.Menu.getApplicationMenu().items.find(i=>i.label==='File').submenu.items.find(i=>i.label==='Open Recent').submenu.popup({window:w});return true;})()`);
    execFileSync('/usr/bin/python3',[path.join(report.root,'tests/ui-tracer-native.py'),'keys'],{timeout:15000,env:{...process.env,...report.environment}});
    await sleep(800);
    assert.equal(await page.evaluate(`!!(${button('Approve')})`),false,'Reopen resurrected a consumed card');
    assert.equal(planStage,4);assert.equal(polls,3);assert.equal(requests.filter(r=>r.method==='POST').length,1);
    save('final-state.json',state());save('final-ui.json',await page.evaluate('document.body.innerText'));await shot(page,'final-ui');
    assert.deepEqual(report.deniedNetwork,[]);save('a02-stage-result.json',{verdict:'PASS',polls,requests:requests.length,spendId:spend.id});
  }
};
try{process.exitCode=await run(out,false,scenario);}catch(e){console.error(e);process.exitCode=1;}
finally{if(server){server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}fs.writeFileSync(path.join(out,'provider-cleanup.json'),JSON.stringify({listening:server?.listening??false,error:serverError?.stack??null}));}
