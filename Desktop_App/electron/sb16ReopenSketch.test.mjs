import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';
const {HOST_COMMANDS}=createRequire(import.meta.url)('./commandAllowlist.cjs');
test('ordinary preload forwards exact reopen identity through admitted host IPC',async()=>{
 let bridge;const calls=[];
 vm.runInNewContext(readFileSync(new URL('./preload.cjs',import.meta.url),'utf8'),{
  require:()=>({contextBridge:{exposeInMainWorld:(name,value)=>{assert.equal(name,'workbench');bridge=value;}},ipcRenderer:{invoke:async(channel,cmd,args)=>{assert.equal(channel,'host:invoke');assert.equal(HOST_COMMANDS.has(cmd),true);calls.push({cmd,args});return {note:'transport fixture'};}}}),
  location:{href:'file:///fixture/dist/index.html'},URL,process:{platform:'linux'}
 });
 const args={projectRoot:'/fixture/sb16',sceneId:1,shotId:'shot_2'};
 await bridge.invoke('workflow_reopen_sketch',args);
 assert.deepEqual(calls,[{cmd:'workflow_reopen_sketch',args}]);
 assert.equal(HOST_COMMANDS.has('workflow_reopen_everything'),false);
});
