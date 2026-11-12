const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),out=path.join(root,'evidence/sb22');
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function until(label,fn){for(let i=0;i<100;i++){if(await fn())return;await sleep(60);}throw Error('Timeout '+label);}
async function electron(){
 const {app,BrowserWindow}=require('electron');app.disableHardwareAcceleration();app.setPath('userData',process.env.SB22_PROFILE);await app.whenReady();
 const win=new BrowserWindow({width:1250,height:1000,webPreferences:{contextIsolation:true,nodeIntegration:false,sandbox:true}}),wc=win.webContents;
 const run=js=>wc.executeJavaScript(js),report={exit:1,checks:[],snapshots:[],externalRequests:[]};
 wc.session.webRequest.onBeforeRequest((d,cb)=>{const ok=d.url.startsWith('data:')||new URL(d.url).host===new URL(process.env.SB22_ORIGIN).host;if(!ok)report.externalRequests.push(d.url);cb({cancel:!ok});});
 const label=s=>`Array.from(document.querySelectorAll('button')).find(b=>b.textContent===${JSON.stringify(s)})`;
 const click=async expr=>{const p=await run(`(()=>{const e=${expr};if(!e||e.disabled)throw Error('Missing enabled target '+${JSON.stringify(expr)});e.scrollIntoView({block:'center',behavior:'instant'});const r=e.getBoundingClientRect();const p={x:r.x+r.width/2,y:r.y+r.height/2};if(!e.contains(document.elementFromPoint(p.x,p.y)))throw Error('Target obscured');return p;})()`);wc.sendInputEvent({type:'mouseDown',button:'left',clickCount:1,...p});wc.sendInputEvent({type:'mouseUp',button:'left',clickCount:1,...p});await sleep(70);};
 const open=()=>run(`!!document.querySelector('dialog[data-operation-approval][open]')`);
 const prepare=async()=>{await run('window.prepare()');await until('approval modal',open);};
 const snapshot=async name=>{await run('new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');await sleep(160);const data=await run(`(()=>{const d=document.querySelector('dialog[data-operation-approval]');return{title:d?.querySelector('h2').textContent,body:d?.querySelector('pre').textContent,id:d?.dataset.approvalId,modal:!!d?.matches(':modal'),visible:!!d&&d.getBoundingClientRect().height>0,chatClosed:document.querySelector('#chat-panel').hidden,receipts:[...document.querySelectorAll('[data-request-receipt]')].map(e=>e.textContent)};})()`);report.snapshots.push({name,...data});fs.writeFileSync(path.join(out,name+'.png'),(await wc.capturePage()).toPNG());return data;};
 try{
  await win.loadURL(process.env.SB22_ORIGIN+'/sb22');await until('fixture',()=>run('window.fixtureReady'));win.show();win.focus();
  await prepare();const a=await snapshot('exact-operation');assert.equal(a.modal,true);assert.equal(a.visible,true);assert.match(a.body,/Scenes\/Scene1\/Cut1\/take.mp4/);assert.equal(a.chatClosed,true);
  // Capture a detached element as well as clicking the actual trusted control.
  await run(`window.oldApprove=${label('Approve exact operation')}`);await click(label('Approve exact operation'));await run('window.oldApprove.click()');await sleep(120);
  assert.equal(await run(`window.calls.filter(c=>c.cmd==='director_approve').length`),1);assert.equal(await open(),false);report.checks.push('trusted exact approval + detached duplicate = one dispatch');
  await prepare();await click(label('Deny operation'));assert.equal(await run(`window.calls.filter(c=>c.cmd==='director_cancel').length`),1);report.checks.push('deny = zero approval dispatch');
  await prepare();wc.sendInputEvent({type:'keyDown',keyCode:'Escape'});wc.sendInputEvent({type:'keyUp',keyCode:'Escape'});await until('escape closes',async()=>!await open());assert.equal(await run(`window.calls.filter(c=>c.cmd==='director_cancel').length`),2);report.checks.push('Escape = cancel');
  await run(`window.mode='expiry'`);await prepare();await click(label('Approve exact operation'));await until('expiry shown',()=>run(`document.body.textContent.includes('That approval expired. Ask for a new plan.')`));await snapshot('host-expiry');report.checks.push('host expiry error displayed without retry');
  await run(`window.mode='success'`);await prepare();await run(`window.oldApprove=${label('Approve exact operation')};window.replaceProject()`);await until('same root replacement',async()=>!await open());const count=await run('window.calls.length');await run('window.oldApprove.click()');await sleep(80);assert.equal(await run('window.calls.length'),count);report.checks.push('same-root reopen drops stale approval');
  await run(`window.mode='delay'`);await prepare();await click(label('Approve exact operation'));await until('Stop available',()=>run(`!!${label('Stop operation')}`));await click(label('Stop operation'));await run('window.release()');await sleep(130);await snapshot('stopped');assert.equal(await run(`window.calls.filter(c=>c.cmd==='director_cancel').length`),3);report.checks.push('Stop retained during delayed exact execution');
  report.calls=await run('window.calls');report.chatCalls=await run('window.chatCalls');assert.equal(report.chatCalls.length,0);assert.equal(report.externalRequests.length,0);assert.equal(report.calls.some(c=>c.cmd==='media_generate_confirmed'||c.cmd.startsWith('atlas_')),false);report.exit=0;
 }catch(e){report.error=e.stack;report.ui=await run('document.body.innerText').catch(()=>null);}
 fs.writeFileSync(path.join(out,'electron-readback.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));win.destroy();app.exit(report.exit);
}
async function node(){
 const {createServer}=await import('vite');const server=await createServer({root,server:{host:'127.0.0.1',port:0,hmr:false},plugins:[{name:'sb22-fixture',configureServer(s){s.middlewares.use('/sb22',async(_q,res)=>{res.setHeader('Content-Type','text/html');res.end(await s.transformIndexHtml('/sb22','<!doctype html><html><body><aside id="chat-panel" hidden>Saved Director conversation remains closed</aside><main id="desk"></main><script type="module" src="/tests/fixtures/sb22-approval.js"></script></body></html>'));});}}]});
 try{await server.listen();const origin='http://127.0.0.1:'+server.httpServer.address().port,profile=fs.mkdtempSync(path.join(process.env.TMPDIR,'sb22-profile-'));const child=spawn(require('electron'),['--no-sandbox',__filename],{stdio:'inherit',cwd:root,env:{...process.env,SB22_ORIGIN:origin,SB22_PROFILE:profile}});const code=await new Promise((r,j)=>{child.on('error',j);child.on('exit',r);});assert.equal(code,0);}finally{await server.close();}
}
(process.versions.electron?electron():node()).catch(e=>{console.error(e);process.exit(1);});
