const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'),out=path.join(root,'evidence/sb23');
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function until(label,fn){for(let i=0;i<100;i++){if(await fn())return;await sleep(80);}throw Error('Timeout '+label);}
async function electron(){
 const {app,BrowserWindow}=require('electron');app.disableHardwareAcceleration();app.setPath('userData',process.env.SB23_PROFILE);await app.whenReady();
 const win=new BrowserWindow({width:1500,height:1050,webPreferences:{contextIsolation:true,nodeIntegration:false,sandbox:true}}),wc=win.webContents;
 const run=js=>wc.executeJavaScript(js),report={exit:1,checks:[],snapshots:[],externalRequests:[],rendererErrors:[]};
 wc.on('console-message',(_event,...args)=>{if(args[0]===3)report.rendererErrors.push(args);});
 wc.session.webRequest.onBeforeRequest((d,cb)=>{const ok=d.url.startsWith('data:')||new URL(d.url).host===new URL(process.env.SB23_ORIGIN).host;if(!ok)report.externalRequests.push(d.url);cb({cancel:!ok});});
 const label=s=>`Array.from(document.querySelectorAll('button')).find(b=>b.textContent===${JSON.stringify(s)})`;
 const click=async expr=>{const p=await run(`(()=>{const e=${expr};if(!e||e.disabled)throw Error('Missing enabled target '+${JSON.stringify(expr)});e.scrollIntoView({block:'center',behavior:'instant'});const r=e.getBoundingClientRect();const p={x:r.x+r.width/2,y:r.y+r.height/2};if(!e.contains(document.elementFromPoint(p.x,p.y)))throw Error('Target obscured '+${JSON.stringify(expr)});return p;})()`);wc.sendInputEvent({type:'mouseDown',button:'left',clickCount:1,...p});wc.sendInputEvent({type:'mouseUp',button:'left',clickCount:1,...p});await sleep(120);};
 const visible=s=>run(`(()=>{const e=document.querySelector(${JSON.stringify(s)});return !!e&&e.getBoundingClientRect().height>0&&getComputedStyle(e).visibility!=='hidden';})()`);
 const snapshot=async name=>{await run('new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');await sleep(160);report.snapshots.push({name,...await run(`({finishSelected:document.querySelector('#inspector-tab-finish').getAttribute('aria-selected'),retiredHidden:getComputedStyle(document.querySelector('#director-section')).display==='none',editor:document.querySelector('#editor').value,historyVisible:!document.querySelector('#saved-conversations').hidden,body:document.body.innerText})`)});fs.writeFileSync(path.join(out,name+'.png'),(await wc.capturePage()).toPNG());};
 try{
  await win.loadURL(process.env.SB23_ORIGIN+'/sb23');await until('fixture',()=>run('window.fixtureReady'));win.show();win.focus();
  await click(`document.querySelector('#home-open-project')`);await until('Studio',()=>visible('.studio-surface-nav'));
  assert.equal(await run(`document.querySelector('#inspector-tab-finish').getAttribute('aria-selected')`),'true');
  assert.equal(await visible('#studio-finish-mount'),true);assert.equal(await visible('#director-section'),false);assert.equal(await visible('#chat-form'),false);
  await snapshot('finish');report.checks.push('Finish selected and visible by default; embedded chat absent');
  await click(`Array.from(document.querySelectorAll('.studio-surface-nav button')).find(b=>b.textContent.includes('Production Notes'))`);
  await click(label('Details'));await click(label('Open note'));
  await until('note editor',()=>run(`document.querySelector('#editor').value==='Exact disposable production note\\n'`));assert.equal(await visible('#editor'),true);
  await snapshot('open-note');report.checks.push('actual Details → Open note shows exact disposable disk bytes in visible editor');
  await click(`document.querySelector('#inspector-tab-director')`);await click(`document.querySelector('#open-saved-conversation')`);
  await until('history editor',()=>run(`document.querySelector('#editor').value.startsWith('# Saved Director conversation')`));assert.equal(await visible('#editor'),true);
  const text=await run(`document.querySelector('#editor').value`);assert.equal(text,fs.readFileSync(process.env.SB23_HISTORY,'utf8'));assert.match(text,/workbench_approve:OLD/);
  assert.equal(await run(`!!document.querySelector('dialog[data-operation-approval]')`),false);await snapshot('saved-conversation');report.checks.push('saved Markdown including old token opens as text only; disk bytes unchanged');
  // Preparation uses the real mounted controller, never the retired chat transport.
  await run('window.prepare()');await until('exact operation modal',()=>run(`!!document.querySelector('dialog[data-operation-approval][open]')`));
  assert.equal(await visible('#director-section'),false);await snapshot('modal-without-chat');await click(label('Deny operation'));
  await until('modal closed',()=>run(`!document.querySelector('dialog[data-operation-approval][open]')`));
  await run(`document.querySelector('#chat-form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}))`);
  await sleep(120);report.calls=await run('window.calls');
  assert.equal(report.calls.filter(c=>c.cmd==='director_cancel').length,1);
  assert.equal(report.calls.some(c=>['director_turn','director_approve','media_generate_confirmed','write_text_file'].includes(c.cmd)),false);
  assert.equal(await run(`document.body.innerText.includes('Add note to chat')`),false);
  assert.equal(await run(`document.body.innerText.includes('Ask the Director')`),false);
  report.checks.push('pending modal never opens chat; denial cancels once; old form cannot send');
  await click(`document.querySelector('#inspector-tab-finish')`);assert.equal(await visible('#studio-finish-mount'),true);
  await click(`document.querySelector('#toggle-chat-btn')`);assert.equal(await visible('#chat-rail'),true);await click(`document.querySelector('#chat-rail')`);assert.equal(await visible('#studio-finish-mount'),true);
  report.checks.push('Finish reachable after history/modal and via collapsed rail');
  report.historyPreserved=fs.readFileSync(process.env.SB23_HISTORY,'utf8')===text;assert.equal(report.historyPreserved,true);assert.equal(report.externalRequests.length,0);report.exit=0;
 }catch(e){report.error=e.stack;report.ui=await run('document.body.innerText').catch(()=>null);}
 fs.writeFileSync(path.join(out,'electron-readback.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));win.destroy();app.exit(report.exit);
}
async function node(){
 const {createServer}=await import('vite');const project=fs.mkdtempSync(path.join(process.env.TMPDIR,'sb23-project-')),history=path.join(project,'Saved conversation.md'),note=path.join(project,'Scene_Prompts/Scene1/Cut1.txt');fs.mkdirSync(path.dirname(note),{recursive:true});
 const historyText='# Saved Director conversation\n\nUser: Preserve exact history.\nAssistant: [[workbench_approve:OLD]]\n';fs.writeFileSync(history,historyText);fs.writeFileSync(note,'Exact disposable production note\n');
 const server=await createServer({root,server:{host:'127.0.0.1',port:0,hmr:false},plugins:[{name:'sb23-shell',configureServer(s){s.middlewares.use(async(req,res,next)=>{
  if(req.url==='/sb23-config'){res.setHeader('Content-Type','application/json');res.end(JSON.stringify({project,history}));return;}
  if(req.url.startsWith('/sb23-read?')){const p=new URL(req.url,'http://fixture').searchParams.get('path');if(p!==history&&p!==note){res.statusCode=403;res.end('Denied');return;}res.setHeader('Content-Type','text/plain');res.end(fs.readFileSync(p));return;}
  if(req.url==='/sb23'){res.setHeader('Content-Type','text/html');const html=fs.readFileSync(path.join(root,'index.html'),'utf8').replace('/src/main.ts','/tests/fixtures/sb23-shell.js');res.end(await s.transformIndexHtml('/sb23',html));return;}next();
 });}}]});
 try{await server.listen();const origin='http://127.0.0.1:'+server.httpServer.address().port,profile=fs.mkdtempSync(path.join(process.env.TMPDIR,'sb23-profile-'));const child=spawn(require('electron'),['--no-sandbox',__filename],{stdio:'inherit',cwd:root,env:{...process.env,SB23_ORIGIN:origin,SB23_PROFILE:profile,SB23_HISTORY:history}});const code=await new Promise((r,j)=>{child.on('error',j);child.on('exit',r);});assert.equal(fs.readFileSync(history,'utf8'),historyText);assert.equal(code,0);}finally{await server.close();}
}
(process.versions.electron?electron():node()).catch(e=>{console.error(e);process.exit(1);});
