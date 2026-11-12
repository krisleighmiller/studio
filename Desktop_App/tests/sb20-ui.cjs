const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const root = path.resolve(__dirname, '..'), out = path.join(root, 'evidence/sb20');
const sleep = ms => new Promise(r => setTimeout(r, ms));
async function until(label, fn) { for(let i=0;i<100;i++){if(await fn())return;await sleep(80);}throw Error('Timeout: '+label); }
async function electron(){
 const {app,BrowserWindow}=require('electron');app.disableHardwareAcceleration();app.setPath('userData',process.env.SB20_PROFILE);await app.whenReady();
 const win=new BrowserWindow({width:1250,height:1050,webPreferences:{contextIsolation:true,nodeIntegration:false,sandbox:true}}), wc=win.webContents;
 const run=js=>wc.executeJavaScript(js), report={exit:1,snapshots:[],externalRequests:[]};
 wc.session.webRequest.onBeforeRequest((d,cb)=>{const ok=new URL(d.url).host===new URL(process.env.SB20_ORIGIN).host;if(!ok)report.externalRequests.push(d.url);cb({cancel:!ok});});
 const click=async expr=>{const p=await run(`(()=>{const e=${expr};if(!e||e.disabled)throw Error('Missing enabled target '+${JSON.stringify(expr)});e.scrollIntoView({block:'center',behavior:'instant'});const r=e.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};})()`);wc.sendInputEvent({type:'mouseDown',button:'left',clickCount:1,...p});wc.sendInputEvent({type:'mouseUp',button:'left',clickCount:1,...p});await sleep(120);};
 const label=s=>`Array.from(document.querySelectorAll('button')).find(b=>b.textContent===${JSON.stringify(s)})`;
 const nav=async s=>click(`Array.from(document.querySelectorAll('.studio-surface-nav button')).find(b=>b.textContent.includes(${JSON.stringify(s)}))`);
 const receipt=()=>run(`document.querySelector('[data-request-receipt]')?.textContent||''`);
 const shot=async name=>{await run(`document.querySelector('[data-request-receipt]')?.scrollIntoView({block:'start',behavior:'instant'})`);await sleep(70);const data=await run(`(()=>{const e=document.querySelector('[data-request-receipt]');return{receipt:e?.outerHTML||null,visible:!!e&&e.getBoundingClientRect().height>0,notes:document.querySelector('[name=notes]')?.value,script:document.querySelector('[name=script]')?.value};})()`);report.snapshots.push({name,...data});fs.writeFileSync(path.join(out,name+'.png'),(await wc.capturePage()).toPNG());};
 try{
  await win.loadURL(process.env.SB20_ORIGIN+'/sb20');await until('fixture',()=>run('window.fixtureReady'));win.show();win.focus();
  await nav('Script');await click(label('Create script'));
  await run(`(()=>{const e=document.querySelector('[name=notes]');e.value='Exact Studio prompt — rain on the window.';e.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await click(label('Generate script from notes'));assert.match(await receipt(),/Submitted.*Working/);await shot('working');
  // The current initiating control is disabled during the delayed call.
  assert.equal(await run(`${label('Generate script from notes')}.disabled`),true);
  await nav('Profile');assert.equal(await receipt(),'');await nav('Script');assert.match(await receipt(),/Exact Studio prompt/);
  await run(`window.finishRequest('success')`);await until('success',async()=>/Success/.test(await receipt()));await shot('success');
  assert.equal(await run(`window.calls.filter(c=>c.cmd==='workflow_generate_script').length`),1);
  await click(label('Generate script from notes'));await run(`window.finishRequest('error')`);await until('error',async()=>/Error/.test(await receipt()));await shot('error');
  await click(label('Generate script from notes'));await run(`window.finishRequest('cancel')`);await until('cancelled',async()=>/Cancelled/.test(await receipt()));await shot('cancelled');
  await nav('Production Notes');await click(label('Resume with saved audio'));await run(`window.finishRequest('pending')`);await until('pending',async()=>/Pending approval/.test(await receipt()));await shot('pending');
  assert.equal(await run('window.handoffs.length'),1);
  await click(label('Resume with saved audio'));await run(`window.replaceProject('/disposable/sb20/B')`);await nav('Production Notes');assert.equal(await receipt(),'');
  await click(label('Resume with saved audio'));await run(`window.finishRequest('pending','/disposable/sb20/B')`);await until('B pending',async()=>/Pending approval/.test(await receipt()));
  const before=await receipt();await run(`window.finishRequest('error','/disposable/sb20/A')`);await sleep(100);assert.equal(await receipt(),before);await shot('B-isolated');
  await run(`window.replaceProject('/disposable/sb20/A')`);await nav('Production Notes');assert.match(await receipt(),/Error.*Fixture host refused/s);await shot('A-restored');
  report.calls=await run('window.calls');report.handoffs=await run('window.handoffs');assert.equal(report.handoffs.length,2);assert.equal(report.externalRequests.length,0);report.exit=0;
 }catch(e){report.error=e.stack;report.ui=await run('document.body.innerText').catch(()=>null);}
 fs.writeFileSync(path.join(out,'electron-readback.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));win.destroy();app.exit(report.exit);
}
async function node(){
 const {createServer}=await import('vite');
 const server=await createServer({root,server:{host:'127.0.0.1',port:0,hmr:false},plugins:[{name:'sb20-fixture',configureServer(s){s.middlewares.use('/sb20',async(_q,res)=>{res.setHeader('Content-Type','text/html');res.end(await s.transformIndexHtml('/sb20','<!doctype html><html><body><main id="desk"></main><script type="module" src="/tests/fixtures/sb20-receipts.js"></script></body></html>'));});}}]});
 try{await server.listen();const origin='http://127.0.0.1:'+server.httpServer.address().port,profile=fs.mkdtempSync(path.join(process.env.TMPDIR||'/runtime','sb20-profile-'));const child=spawn(require('electron'),['--no-sandbox',__filename],{stdio:'inherit',cwd:root,env:{...process.env,SB20_ORIGIN:origin,SB20_PROFILE:profile}});const code=await new Promise((r,j)=>{child.on('error',j);child.on('exit',r);});assert.equal(code,0);}finally{await server.close();}
}
(process.versions.electron?electron():node()).catch(e=>{console.error(e);process.exit(1);});
