const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),out=process.env.SB21_OUTPUT||path.join(root,'evidence/sb21');
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function until(label,fn){for(let i=0;i<100;i++){if(await fn())return;await sleep(60);}throw Error('Timeout '+label);}
async function electron(){
 const {app,BrowserWindow}=require('electron');app.disableHardwareAcceleration();app.setPath('userData',process.env.SB21_PROFILE);await app.whenReady();
 const win=new BrowserWindow({width:1280,height:1080,webPreferences:{contextIsolation:true,nodeIntegration:false,sandbox:true}}),wc=win.webContents;
 const run=js=>wc.executeJavaScript(js),report={exit:1,snapshots:[],actions:[],externalRequests:[]};
 wc.session.webRequest.onBeforeRequest((d,cb)=>{const ok=d.url.startsWith('data:')||new URL(d.url).host===new URL(process.env.SB21_ORIGIN).host;if(!ok)report.externalRequests.push(d.url);cb({cancel:!ok});});
 const label=s=>`Array.from(document.querySelectorAll('button')).find(b=>b.textContent===${JSON.stringify(s)})`;
 const click=async expr=>{const p=await run(`(()=>{const e=${expr};if(!e||e.disabled)throw Error('Missing enabled target '+${JSON.stringify(expr)});e.scrollIntoView({block:'center',behavior:'instant'});const r=e.getBoundingClientRect();const p={x:r.x+r.width/2,y:r.y+r.height/2};if(!e.contains(document.elementFromPoint(p.x,p.y)))throw Error('Target obscured');return p;})()`);wc.sendInputEvent({type:'mouseDown',button:'left',clickCount:1,...p});wc.sendInputEvent({type:'mouseUp',button:'left',clickCount:1,...p});await sleep(80);};
 const nav=async s=>click(`Array.from(document.querySelectorAll('.studio-surface-nav button')).find(b=>b.textContent.includes(${JSON.stringify(s)}))`);
 const dialog=()=>run(`!!document.querySelector('dialog[data-generation-prompt][open]')`);
 const capture=async name=>{const data=await run(`(()=>{const d=document.querySelector('dialog[data-generation-prompt]');return{title:d.querySelector('h2').textContent,prompt:d.querySelector('textarea').value,context:d.querySelector('pre').textContent,visible:d.getBoundingClientRect().height>0,active:document.activeElement?.name,modal:d.matches(':modal')};})()`);report.snapshots.push({name,...data});assert.equal(data.modal,true);assert.equal(data.visible,true);fs.writeFileSync(path.join(out,name+'.png'),(await wc.capturePage()).toPNG());return data;};
 const edit=async text=>{await click(`document.querySelector('dialog textarea')`);wc.sendInputEvent({type:'keyDown',keyCode:'A',modifiers:['control']});wc.sendInputEvent({type:'keyUp',keyCode:'A',modifiers:['control']});wc.insertText(text);await sleep(40);assert.equal(await run(`document.querySelector('dialog textarea').value`),text);};
 try{
  await win.loadURL(process.env.SB21_ORIGIN+'/sb21');await until('fixture',()=>run('window.fixtureReady'));win.show();win.focus();
  await nav('Script');await click(label('Create script'));await run(`(()=>{const e=document.querySelector('[name=notes]');e.value='Original local notes';e.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  const actions=[
   {name:'script',button:'Generate script from notes',cmd:'workflow_generate_script',field:'notes',context:'Original local notes'},
   {name:'character',nav:'Characters',button:'Create full 12-pose sheet',cmd:'start_character_model_sheet',field:'identity_text',context:'Characters/Mara/refs/face.png'},
   {name:'look',nav:'Scene Look',button:'Add alternate',cmd:'workflow_generate_look',field:'prompt',context:'Preproduction/Scene1/references/interior.png'},
   {name:'cut',nav:'Cut Sketches',button:'Draft sketches',cmd:'workflow_draft_sketches',field:'brief',context:'Preproduction/Scene1/references/interior.png'},
   {name:'render',nav:'Production Notes',button:'Render Cut 1 new take',cmd:'workflow_prepare_render',save:'workflow_save_production_prompt',field:'prompt',context:'Preproduction/Scene1/references/interior.png'},
  ];
  for(const a of actions){
   if(a.nav)await nav(a.nav);
   const start=await run('window.calls.length');await click(label(a.button));await until(a.name+' modal',dialog);
   const initial=await capture(a.name+'-prompt');assert.ok(initial.context.includes(a.context));assert.equal(await run('window.calls.length'),start);
   // Escape must cancel without even a preparation/save command.
   wc.sendInputEvent({type:'keyDown',keyCode:'Escape'});wc.sendInputEvent({type:'keyUp',keyCode:'Escape'});await until('escape',async()=>!await dialog());assert.equal(await run('window.calls.length'),start);
   await click(label(a.button));await until('reopened prompt',dialog);
   const text=`Edited ${a.name} exact text — preserve scar.\nNo hidden additions.`;await edit(text);await capture(a.name+'-edited');
   await click(label('Prepare request'));await until(a.name+' settled',async()=>!await dialog()&&!await run(`document.querySelector('.workflow-busy')!==null`));await sleep(130);
   const actual=await run(`window.calls.slice(${start})`),target=actual.find(c=>c.cmd===(a.save||a.cmd));
   assert.ok(target,JSON.stringify(actual));assert.equal(a.name==='character'?target.args.request.inputs[a.field]:target.args[a.field],text);
   assert.equal(actual.filter(c=>c.cmd===a.cmd).length,1);
   assert.equal(actual.some(c=>c.cmd==='director_approve'||c.cmd.startsWith('atlas_')),false);
   const approval=await run(`document.querySelector('dialog[data-operation-approval]')?.dataset.approvalId||null`);
   if(['look','cut','render'].includes(a.name))assert.ok(approval,'exact authorization modal required for '+a.name);
   if(approval){await click(label('Deny operation'));await sleep(80);}
   report.actions.push({...a,expected:text,actual,cancelCalls:0,prepares:1,approval});
  }
  // Editing a previously issued render goes through a fresh preparation, never old-ID approval.
  const old=report.actions.at(-1).approval;await click(label('Render Cut 1 new take'));await until('render edit',dialog);await edit('Second freshly prepared render');await click(label('Prepare request'));await sleep(150);assert.notEqual(await run(`document.querySelector('dialog[data-operation-approval]')?.dataset.approvalId`),old);await click(label('Deny operation'));
  await click(label('Render Cut 1 new take'));await until('replacement prompt',dialog);await run('window.replaceProject()');assert.equal(await dialog(),false);
  report.calls=await run('window.calls');report.handoffs=await run('window.handoffs');assert.equal(report.externalRequests.length,0);report.exit=0;
 }catch(e){report.error=e.stack;report.ui=await run('document.body.innerText').catch(()=>null);}
 fs.writeFileSync(path.join(out,'electron-readback.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));win.destroy();app.exit(report.exit);
}
async function node(){
 const {createServer}=await import('vite');const server=await createServer({root,server:{host:'127.0.0.1',port:0,hmr:false},plugins:[{name:'sb21-fixture',configureServer(s){s.middlewares.use('/sb21',async(_q,res)=>{res.setHeader('Content-Type','text/html');res.end(await s.transformIndexHtml('/sb21','<!doctype html><html><body><main id="desk"></main><script type="module" src="/tests/fixtures/sb21-prompts.js"></script></body></html>'));});}}]});
 try{await server.listen();const origin='http://127.0.0.1:'+server.httpServer.address().port,profile=fs.mkdtempSync(path.join(process.env.TMPDIR,'sb21-profile-'));const child=spawn(require('electron'),['--no-sandbox',__filename],{stdio:'inherit',cwd:root,env:{...process.env,SB21_ORIGIN:origin,SB21_PROFILE:profile}});const code=await new Promise((r,j)=>{child.on('error',j);child.on('exit',r);});assert.equal(code,0);}finally{await server.close();}
}
(process.versions.electron?electron():node()).catch(e=>{console.error(e);process.exit(1);});
