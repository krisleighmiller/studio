// Isolated synthetic fixture ONLY. No catalog fetch or generation commands.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {spawn} from 'node:child_process';
import {CDP,until} from '../Desktop_App/tests/helpers/ui.mjs';
assert.equal(process.env.STUDIO_KEYRING_FIXTURE,'1');
assert.equal(process.env.HOME,'/studio-home');
const app='/source/Studio/Desktop_App',out='/tmp/keyring-ui';
fs.mkdirSync(out,{recursive:true});
const children=[];
function launch(bin,args,name){
 const log=fs.openSync(`${out}/${name}.log`,'w');
 const p=spawn(bin,args,{cwd:app,stdio:['ignore',log,log],env:{...process.env,XDG_CACHE_HOME:'/tmp/ui-cache'}});
 children.push(p);return p;
}
let cdp;
try{
 launch(`${app}/node_modules/.bin/vite`,['--host','127.0.0.1'],'vite');
 await until('vite',async()=>{try{return(await fetch('http://127.0.0.1:1420')).ok}catch{return false}},30000);
 launch(`${app}/node_modules/.bin/electron`,['.','--disable-gpu','--remote-debugging-address=127.0.0.1','--remote-debugging-port=9333'],'electron');
 const target=await until('renderer',async()=>{try{return(await(await fetch('http://127.0.0.1:9333/json/list')).json()).find(t=>t.type==='page')}catch{return null}},30000);
 cdp=await CDP.open(target.webSocketDebuggerUrl);
 await until('settings button',()=>cdp.evaluate('!!document.querySelector("#settings-btn")'),30000);
 await until('Studio ready',()=>cdp.evaluate('document.body.innerText.length>100'),30000);
 await new Promise(r=>setTimeout(r,1800));
 await cdp.evaluate('document.querySelector("#settings-btn").click()');
 await until('settings card',()=>cdp.evaluate('!!document.querySelector(".provider-card [data-field=apiKey]")'),30000);
 if(process.argv[2]==='save'){
  await cdp.evaluate(`(()=>{const card=document.querySelector('.provider-card');for(const [f,v] of Object.entries({apiKey:'studio-ui-only-noncredential-2026',baseUrl:'http://127.0.0.1:9',name:'UI dummy only'})){const e=card.querySelector('[data-field='+f+']');e.value=v;e.dispatchEvent(new Event('input',{bubbles:true}));}const enabled=card.querySelector('[data-field=enabled]');if(enabled.checked)enabled.click();document.querySelector('#settings-view [data-action=save]').click();return true})()`);
  await until('saved status',()=>cdp.evaluate('/Settings saved/.test(document.body.innerText)'),30000);
  console.log('PASS actual Settings UI Save with isolated dummy API key');
  await cdp.evaluate('document.querySelector("#settings-view [data-action=reload]").click()');
 }
 await until('saved-key placeholder',()=>cdp.evaluate(`(()=>{const e=document.querySelector('.provider-card [data-field=apiKey]');return e?.value===''&&e.placeholder.includes('Saved on this computer')})()`),30000);
 const result=await cdp.evaluate(`(async()=>{const s=await window.workbench.invoke('load_settings');return {hasKey:s.apis[0].hasKey,blank:s.apis[0].apiKey==='',disabled:!s.apis[0].enabled}})()`);
 assert.deepEqual(result,{hasKey:true,blank:true,disabled:true});
 console.log('PASS actual Settings UI reload/restart: saved-key placeholder, hasKey=true, key blank, API disabled');
 const disk=`${process.env.XDG_CONFIG_HOME}/video-workbench/config/settings.json`;
 const raw=fs.readFileSync(disk,'utf8');assert.ok(!raw.includes('studio-ui-only-noncredential-2026'));
 assert.equal(fs.statSync(disk).mode&0o777,0o600);
 console.log('PASS Electron-owned settings: no plaintext dummy key, mode 0600');
}catch(e){console.error(e);process.exitCode=1;for(const name of ['electron','vite'])try{console.error(fs.readFileSync(`${out}/${name}.log`,'utf8'))}catch{}}
finally{cdp?.close();for(const p of children.reverse())p.kill('SIGTERM');await new Promise(r=>setTimeout(r,600));for(const p of children)if(p.exitCode===null)p.kill('SIGKILL');}
