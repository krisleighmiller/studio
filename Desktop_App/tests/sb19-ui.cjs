const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const { spawn, execFileSync } = require('node:child_process');
const root = path.resolve(__dirname, '..'), out = path.join(root, 'evidence/sb19');
const sleep = ms => new Promise(r => setTimeout(r, ms));
async function until(label, fn) { for (let i=0;i<150;i++) { if (await fn()) return; await sleep(80); } throw Error('Timeout: '+label); }
async function electron() {
 const { app, BrowserWindow, Menu, clipboard } = require('electron');
 app.disableHardwareAcceleration(); app.setPath('userData', process.env.SB19_PROFILE);
 await app.whenReady();
 const win = new BrowserWindow({width:1300,height:1050,webPreferences:{contextIsolation:true,nodeIntegration:false,sandbox:true}});
 const wc=win.webContents, run=js=>wc.executeJavaScript(js);
 const report={exit:1,fields:[],nativeMenus:[],externalRequests:[]};
 let menuCount=0;
 const observedMenu={buildFromTemplate(template){ const menu=Menu.buildFromTemplate(template); report.nativeMenus.push(template); menuCount++; return menu; }};
 const { installEditableContextMenu }=await import('../electron/editableContextMenu.mjs');
 installEditableContextMenu(win,observedMenu);
 Menu.setApplicationMenu(Menu.buildFromTemplate([{label:'Edit',submenu:[{role:'cut'},{role:'copy'},{role:'paste'},{role:'selectAll'}]}]));
 wc.session.webRequest.onBeforeRequest((d,cb)=>{const ok=new URL(d.url).host===new URL(process.env.SB19_ORIGIN).host;if(!ok)report.externalRequests.push(d.url);cb({cancel:!ok});});
 const click=async expr=>{const p=await run(`(()=>{const e=${expr};if(!e||e.disabled)throw Error('Missing enabled target: '+${JSON.stringify(expr)});e.scrollIntoView({block:'center',behavior:'instant'});const r=e.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+Math.min(r.height/2,15)};})()`);wc.sendInputEvent({type:'mouseDown',button:'left',clickCount:1,...p});wc.sendInputEvent({type:'mouseUp',button:'left',clickCount:1,...p});await sleep(150);};
 const label=s=>`Array.from(document.querySelectorAll('button')).find(b=>b.textContent===${JSON.stringify(s)})`;
 const right=async selector=>{const p=await run(`(()=>{const e=document.querySelector(${JSON.stringify(selector)});e.scrollIntoView({block:'center',behavior:'instant'});e.focus();const r=e.getBoundingClientRect();return {x:r.x+10,y:r.y+10};})()`);wc.sendInputEvent({type:'mouseDown',button:'right',clickCount:1,...p});wc.sendInputEvent({type:'mouseUp',button:'right',clickCount:1,...p});await sleep(200);};
 const nativeChoice=async index=>{execFileSync('/usr/bin/python3',['tests/sb19-menu-keys.py','Home',...Array(index).fill('Down'),'Return'],{timeout:5000});await sleep(150);};
 const value=s=>run(`document.querySelector(${JSON.stringify(s)}).value`);
 const prepare=async(s,text)=>run(`(()=>{const e=document.querySelector(${JSON.stringify(s)});e.value=${JSON.stringify(text)};e.dispatchEvent(new Event('input',{bubbles:true}));e.focus();e.select();})()`);
 const shortcut=async key=>{wc.sendInputEvent({type:'keyDown',keyCode:key,modifiers:['control']});wc.sendInputEvent({type:'keyUp',keyCode:key,modifiers:['control']});await sleep(120);};
 async function field(name,s) {
  const text='Clipboard '+name+' 雨', paste='Pasted '+name+' ☔';
  await prepare(s,text);const before=menuCount;await right(s);assert.equal(menuCount,before+1,name+' native menu');
  await nativeChoice(1);assert.equal(clipboard.readText(),text,'native Copy '+name);assert.equal(await value(s),text);
  await right(s);await nativeChoice(0);assert.equal(clipboard.readText(),text);assert.equal(await value(s),'','native Cut '+name);
  clipboard.writeText(paste);await right(s);await nativeChoice(2);assert.equal(await value(s),paste,'native Paste '+name);
  await prepare(s,text);await shortcut('C');assert.equal(clipboard.readText(),text);await shortcut('X');assert.equal(await value(s),'');clipboard.writeText(paste);await shortcut('V');assert.equal(await value(s),paste);
  report.fields.push({name,selector:s,expected:paste,actual:await value(s),nativeCopy:true,nativeCut:true,nativePaste:true,keyboardCopy:true,keyboardCut:true,keyboardPaste:true});
 }
 try {
  await win.loadURL(process.env.SB19_ORIGIN+'/sb19');await until('main mounted',()=>run('window.fixtureReady'));win.show();win.focus();
  await click(`document.querySelector('#home-open-project')`);await until('Create script',()=>run(`!!${label('Create script')}`));await click(label('Create script'));
  await field('script','[name="script"]');await field('notes','[name="notes"]');
  await click(label('Cancel'));
  await run(`(()=>{const e=document.querySelector('#shell-work-area');e.value='profile';e.dispatchEvent(new Event('change',{bubbles:true}));})()`);await sleep(200);
  await click(`Array.from(document.querySelectorAll('.studio-surface-nav button')).find(b=>b.textContent.includes('Characters'))`);
  await click(label('New character'));await field('character','.workflow-character-fields input');await field('identity-prompt','.workflow-character-fields label:last-child textarea');
  // Exercise the same delegated clipboard path inside a real HTML modal. The
  // task-specific operation modal is a later card, not fabricated acceptance here.
  await run('window.mountPromptModal()');
  await field('modal-production-prompt','#clipboard-modal textarea');await run(`document.querySelector('#clipboard-modal').close()`);
  const count=menuCount;await right('#center-panel-heading');assert.equal(menuCount,count);report.nonEditableNoMenu=true;
  await run(`(()=>{const d=document.createElement('textarea');d.id='readonly-fixture';d.readOnly=true;d.value='read only';document.body.append(d);})()`);await right('#readonly-fixture');assert.equal(menuCount,count);report.readonlyNoMenu=true;
  await click(`document.querySelector('.nav-item[data-surface="files"]')`);
  await click(`document.querySelector('#open-file-btn')`);
  await until('editor visible',()=>run(`document.querySelector('#editor').value==='Editor local text'`));
  await right('#editor');assert.equal(menuCount,count);assert.match(await run(`document.querySelector('#context-menu').textContent`),/Find/);report.editorCustomPreserved=true;
  await prepare('#editor','Editor clipboard 雨');await right('#editor');
  await click(`Array.from(document.querySelectorAll('#context-menu button')).find(b=>b.textContent==='Copy')`);await until('editor Copy',()=>clipboard.readText()==='Editor clipboard 雨');
  await right('#editor');await click(`Array.from(document.querySelectorAll('#context-menu button')).find(b=>b.textContent==='Cut')`);await until('editor Cut',async()=>await value('#editor')==='');
  clipboard.writeText('Editor pasted ☔');await right('#editor');await click(`Array.from(document.querySelectorAll('#context-menu button')).find(b=>b.textContent==='Paste')`);await until('editor Paste',async()=>await value('#editor')==='Editor pasted ☔');
  await prepare('#editor','Editor keyboard 雨');await shortcut('C');assert.equal(clipboard.readText(),'Editor keyboard 雨');await shortcut('X');assert.equal(await value('#editor'),'');clipboard.writeText('Editor keyboard pasted ☔');await shortcut('V');assert.equal(await value('#editor'),'Editor keyboard pasted ☔');report.editorClipboardAndKeyboard=true;
  await right('#file-tree');assert.equal(menuCount,count);assert.match(await run(`document.querySelector('#context-menu').textContent`),/New Folder/);report.explorerCustomPreserved=true;
  report.calls=await run('window.calls');assert.equal(report.externalRequests.length,0);report.exit=0;
 } catch(e){report.error=e.stack;report.ui=await run(`({text:document.body.innerText,buttons:Array.from(document.querySelectorAll('button')).map(b=>({text:b.textContent,id:b.id})),inputs:Array.from(document.querySelectorAll('input,textarea')).map(e=>({id:e.id,name:e.name,label:e.closest('label')?.textContent}))})`).catch(()=>null);}
 fs.writeFileSync(path.join(out,'electron-readback.json'),JSON.stringify(report,null,2));fs.writeFileSync(path.join(out,'electron.png'),(await wc.capturePage()).toPNG());console.log(JSON.stringify(report,null,2));win.destroy();app.exit(report.exit);
}
async function node(){
 const {createServer}=await import('vite');
 const server=await createServer({root,server:{host:'127.0.0.1',port:0,hmr:false},plugins:[{name:'sb19-fixture',configureServer(s){s.middlewares.use('/sb19',async(_req,res)=>{const html=fs.readFileSync(path.join(root,'index.html'),'utf8').replace('/src/main.ts','/tests/fixtures/sb19-clipboard.js');res.setHeader('Content-Type','text/html');res.end(await s.transformIndexHtml('/sb19',html));});}}]});
 try{await server.listen();const origin='http://127.0.0.1:'+server.httpServer.address().port;const profile=fs.mkdtempSync(path.join(process.env.TMPDIR||'/runtime','sb19-profile-'));const child=spawn(require('electron'),[__filename],{stdio:'inherit',cwd:root,env:{...process.env,SB19_ORIGIN:origin,SB19_PROFILE:profile}});const code=await new Promise((r,j)=>{child.on('error',j);child.on('exit',r);});assert.equal(code,0);}finally{await server.close();}
}
(process.versions.electron?electron():node()).catch(e=>{console.error(e);process.exit(1);});
