// Separate Electron processes, same disposable Chromium profile/origin. No ordinary app restart.
const fs=require('node:fs'), path=require('node:path'), assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const root=path.resolve(__dirname,'..'), out=path.join(root,'evidence/sb07');
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function until(label,fn) {const end=Date.now()+20000;while(Date.now()<end){if(await fn()) return;await sleep(50);}throw new Error('Timeout: '+label);}
async function electron() {
  const {app,BrowserWindow}=require('electron');app.disableHardwareAcceleration();app.setPath('userData',process.env.SB07_USER_DATA);
  await app.whenReady();const win=new BrowserWindow({width:1250,height:1100,webPreferences:{contextIsolation:true,nodeIntegration:false}});
  const wc=win.webContents, evaluate=js=>wc.executeJavaScript(js);
  wc.on('console-message',(_event,level,message)=>console.log('renderer:',message));
  const result={phase:process.env.SB07_PHASE,externalRequests:[],exit:1};
  wc.session.webRequest.onBeforeRequest((details,callback)=>{const url=new URL(details.url);const allowed=url.origin===new URL(process.env.SB07_URL).origin||(url.protocol==='ws:'&&url.hostname==='127.0.0.1');if(!allowed)result.externalRequests.push(details.url);callback({cancel:!allowed});});
  try {
    await win.loadURL(process.env.SB07_URL);await until('mounted desk',()=>evaluate('window.fixtureReady'));
    if(result.phase==='write') {
      await evaluate(`clickLabel('Create script');for(const [name,value] of Object.entries(expected))editDraft(name,value);`);
      assert(await evaluate(`JSON.stringify(readDraft())===JSON.stringify(expected)`));
      result.savedStatus=await evaluate(`document.querySelector('[data-draft-recovery]').textContent`);
      assert(result.savedStatus.includes('Draft saved locally'));
      await evaluate(`openProject('/disposable/sb07/B')`);await evaluate(`clickLabel('Create script');editDraft('notes','Only B');`);
    } else {
      // Automatic first-open recovery is visible, not merely a stored JSON record.
      assert(await evaluate(`JSON.stringify(readDraft())===JSON.stringify(${result.phase==='final'?'({...expected,script:largeText})':'expected'})`));
      result.exactTextRecovered=true;
      result.recoveredStatus=await evaluate(`document.querySelector('[data-draft-recovery]').textContent`);
      assert(result.recoveredStatus.includes('Recovered local draft'));
      result.actual=await evaluate(`({title:readDraft().title,notes:readDraft().notes,script:readDraft().script.length<200?readDraft().script:'[large exact text verified]',scriptLength:readDraft().script.length})`);
      await evaluate(`openProject('/disposable/sb07/B')`);assert.equal(await evaluate(`readDraft().notes`),'Only B');
      result.projectIsolation=true;
      await evaluate(`openProject('/disposable/sb07/A')`);
      if(result.phase==='quota') {
        result.quota=await evaluate(`(()=>{let count=0;try{for(;count<1000;count++)localStorage.setItem('quota-'+count,'x'.repeat(100000));}catch(e){return {name:e.name,filled:count};}throw new Error('Expected actual quota');})()`);
        assert.equal(result.quota.name,'QuotaExceededError');
        await evaluate(`window.prior=localStorage.getItem('studio.local-script-draft.v1:/disposable/sb07/A');editDraft('script',largeText)`);
        result.failure=await evaluate(`({status:document.querySelector('[data-draft-recovery]').textContent,role:document.querySelector('[data-draft-recovery]').getAttribute('role'),exact:readDraft().script===largeText,durableUnchanged:localStorage.getItem('studio.local-script-draft.v1:/disposable/sb07/A')===prior})`);
        assert(result.failure.exact&&result.failure.durableUnchanged);assert.equal(result.failure.role,'alert');assert(result.failure.status.includes('only in memory'));
        await evaluate(`document.querySelector('[data-draft-recovery]').scrollIntoView({block:'center'})`);
        result.statusDoesNotOverlap=await evaluate(`document.querySelector('[data-draft-recovery]').getBoundingClientRect().top>document.querySelector('textarea[name="script"]').getBoundingClientRect().bottom`);
        assert(result.statusDoesNotOverlap);
        fs.writeFileSync(path.join(out,'recovery-failure.png'),(await wc.capturePage()).toPNG());
        await evaluate(`clickLabel('Cancel');clickLabel('Create script')`);assert(await evaluate(`readDraft().script===largeText`));
        await evaluate(`for(let i=0;i<1000;i++)localStorage.removeItem('quota-'+i);clickLabel('Retry draft recovery')`);
        await until('recovery retry',()=>evaluate(`document.querySelector('[data-draft-recovery]').textContent.includes('Draft saved locally')`));
        result.retryExact=await evaluate(`JSON.parse(localStorage.getItem('studio.local-script-draft.v1:/disposable/sb07/A')).script===largeText`);assert(result.retryExact);
        await evaluate(`document.querySelector('textarea[name="script"]').scrollIntoView({block:'center'})`);
        fs.writeFileSync(path.join(out,'recovered-form.png'),(await wc.capturePage()).toPNG());
      } else {
        await evaluate(`clickLabel('Discard draft')`);await until('discard settled',()=>evaluate(`!document.querySelector('form')`));
        await evaluate(`openProject('/disposable/sb07/A')`);await evaluate(`clickLabel('Create script')`);assert.equal(await evaluate(`readDraft().script`),'');
        result.discardExact=true;
      }
    }
    result.calls=await evaluate('window.calls');result.providerAttempts=await evaluate('window.providerAttempts');
    assert.equal(result.providerAttempts,0);assert.equal(result.externalRequests.length,0);assert(result.calls.every(c=>c.command==='workflow_import'));
    result.exit=0;
  }catch(error){result.error=error.stack;}
  fs.writeFileSync(path.join(out,`electron-${result.phase}.json`),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result,null,2));win.destroy();if(result.exit)app.exit(result.exit);else app.quit();
}
async function node() {
  const {createServer}=await import('vite');const server=await createServer({root,server:{host:'127.0.0.1',port:0,hmr:false}});
  const userData=fs.mkdtempSync(path.join(process.env.TMPDIR||'/runtime','sb07-profile-'));
  fs.writeFileSync(path.join(out,'profile-path.txt'),userData+'\n');
  try{await server.listen();const url=`http://127.0.0.1:${server.httpServer.address().port}/tests/fixtures/sb07-recovery.html`;
    for(const phase of ['write','quota','final']){
      const child=spawn(require('electron'),['--no-sandbox',__filename],{cwd:root,stdio:'inherit',env:{...process.env,SB07_URL:url,SB07_USER_DATA:userData,SB07_PHASE:phase}});
      const code=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',code=>resolve(code??1));});assert.equal(code,0,`Electron ${phase}`);
    }
  }finally{await server.close();}
}
(process.versions.electron?electron():node()).catch(error=>{console.error(error);process.exit(1)});
