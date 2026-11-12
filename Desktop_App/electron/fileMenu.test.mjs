import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
// Execute the actual menu factory with the Electron boundary captured, not a
// duplicate template. The full native menu is additionally exercised in Xvfb.
test('native File entries emit file payloads and retain project/save commands with distinct accelerators',()=>{
 const source=readFileSync(new URL('./main.mjs',import.meta.url),'utf8');
 const start=source.indexOf('function createMenu('),end=source.indexOf('\nfunction ',start+1);
 let template;const sent=[];
 const context={Buffer,isDev:false,Menu:{buildFromTemplate:t=>(template=t),setApplicationMenu:()=>{}},mainWindow:{webContents:{send:(channel,payload)=>sent.push([channel,payload])}},saveRecentProjects:()=>{}};
 vm.runInNewContext(source.slice(start,end),context);context.createMenu([]);
 const file=template.find(item=>item.label.replace('&','')==='File').submenu;
 const expected=[['New File','new-file','CmdOrCtrl+N'],['Open File…','open-file','CmdOrCtrl+Shift+O'],['Close File','close-file','CmdOrCtrl+W'],['Open Project…','open-project','CmdOrCtrl+O'],['Save','save','CmdOrCtrl+S'],['Save As…','save-as','CmdOrCtrl+Shift+S']];
 for(const [label,payload,accelerator] of expected){const item=file.find(item=>item.label===label);assert.ok(item,`Discoverable ${label}`);assert.equal(item.accelerator,accelerator);item.click();assert.deepEqual(sent.pop(),['menu',payload]);}
 assert.ok(file.some(item=>item.label==='Open Recent'));assert.ok(file.some(item=>item.label==='Close Project'));
 const keys=template.flatMap(item=>item.submenu??[]).map(item=>item.accelerator).filter(Boolean);assert.equal(new Set(keys).size,keys.length);
});
