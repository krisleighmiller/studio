import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
test('native menu exposes Finish/history, not an embedded-chat workflow or blank transcript export',()=>{
  const source=readFileSync(new URL('./main.mjs',import.meta.url),'utf8');
  const start=source.indexOf('function createMenu('),end=source.indexOf('\nfunction ',start+1);
  let template;const sent=[];
  const context={Buffer,isDev:false,Menu:{buildFromTemplate:t=>(template=t),setApplicationMenu:()=>{}},mainWindow:{webContents:{send:(channel,payload)=>sent.push([channel,payload])}},saveRecentProjects:()=>{}};
  vm.runInNewContext(source.slice(start,end),context);context.createMenu([]);
  const items=template.flatMap(item=>item.submenu??[]);
  assert.equal(items.some(item=>/Toggle Chat|Copy Chat Transcript|Save Chat Transcript/.test(item.label??'')),false);
  const history=items.find(item=>item.label==='Saved Conversations…');assert.ok(history);history.click();assert.deepEqual(sent.pop(),['menu','open-conversation-history']);
  const toggle=items.find(item=>item.label==='Toggle Finish and History');assert.ok(toggle);toggle.click();assert.deepEqual(sent.pop(),['menu','toggle-chat']);
});
