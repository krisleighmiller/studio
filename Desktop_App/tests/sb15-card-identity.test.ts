import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import {afterEach, expect, it, vi} from 'vitest';
import * as desk from '../src/workflowDesk';
import {getSurfaceHost, resetSurfaceSession} from '../src/studio/surfaces/session';
import type {WorkflowImportResult} from '../src/workflowTypes';
const host = vi.hoisted(() => ({invoke:vi.fn(), toMediaUrlAsync:vi.fn(), ask:vi.fn(), openDialog:vi.fn()}));
vi.mock('../src/host', () => host);
const native = JSON.parse(readFileSync('tests/fixtures/sb15-native-card.json', 'utf8'));
afterEach(() => {desk.clearWorkflowDesk(); resetSurfaceSession(); document.body.replaceChildren(); vi.resetAllMocks();});
function click(root:Element, label:string) {
 const b=[...root.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===label);
 expect(b, label).toBeDefined(); expect(b!.disabled, label).toBe(false); b!.click();
}
function card(root:Element, title:string) {
 const found=[...root.querySelectorAll<HTMLElement>('.workflow-sketch-card')].find(c=>c.querySelector('h3')?.textContent?.includes(title));
 expect(found, title).toBeDefined(); return found!;
}
it('saves Cut 2 from its own card, without selection, and carries its stable ID after reorder/reopen', async()=>{
 let current:WorkflowImportResult=structuredClone(native.initial);
 const calls:{cmd:string;args:Record<string,unknown>}[]=[];
 host.invoke.mockImplementation(async(cmd:string,args:Record<string,unknown>)=>{
  calls.push({cmd,args});
  if(cmd==='workflow_import')return structuredClone(current);
  if(cmd==='workflow_save_sketch') {
   expect(args.shotId).toBe(native.ids[1]);
   current=structuredClone(current.state.scenes[0].cuts[0].stable_id===native.ids[1]?native.reopened:native.direct);
   return current;
  }
  if(cmd==='workflow_reorder_cuts') {expect(args.shotIds).toEqual([native.ids[1],native.ids[0]]);current=structuredClone(native.reordered);return current;}
  throw Error(`Forbidden command ${cmd}`);
 });
 const root=document.createElement('main');document.body.append(root);const noop=vi.fn();
 desk.bindWorkflowDesk({root,projectRoot:()=>'/fixture/sb15',onOpenPath:noop,onAddToChat:noop,onOpenVoice:noop,onComposeChat:noop,onSelectionChange:noop,chatModel:()=>null,onStatus:noop,onPendingPlan:noop});
 const open=async()=>{await desk.loadWorkflowDesk('/fixture/sb15');const nav=[...root.querySelectorAll<HTMLButtonElement>('.studio-surface-nav button')].find(b=>b.textContent?.includes('Cut Sketches'))!;expect(nav).toBeDefined();expect(nav.disabled).toBe(false);nav.click();};
 await open();expect(desk.workflowSelection().cut).toBe(1);
 const second=card(root,'Original Cut 2');
 const title=second.querySelector<HTMLInputElement>('input')!;title.value='Card Two Direct';
 click(second,'Save sketch');
 await vi.waitFor(()=>expect(card(root,'Card Two Direct')).toBeDefined());
 expect(calls.find(c=>c.cmd==='workflow_save_sketch')!.args).toMatchObject({cut:2,shotId:native.ids[1],sketch:{title:'Card Two Direct'}});
 expect(card(root,'Original Cut 1')).toBeDefined();
 click(card(root,'Card Two Direct'),'Move earlier');
 await vi.waitFor(()=>expect(card(root,'Card Two Direct').querySelector('h3')?.textContent).toContain('Cut 1'));
 desk.clearWorkflowDesk();await open();
 const moved=card(root,'Card Two Direct');moved.querySelector<HTMLInputElement>('input')!.value='Card Two After Reopen';click(moved,'Save sketch');
 await vi.waitFor(()=>expect(card(root,'Card Two After Reopen')).toBeDefined());
 const saves=calls.filter(c=>c.cmd==='workflow_save_sketch');expect(saves).toHaveLength(2);expect(saves[1].args).toMatchObject({cut:1,shotId:native.ids[1]});
 expect(card(root,'Original Cut 1').querySelector('h3')?.textContent).toContain('Cut 2');
 expect(current.state.scenes[0].cuts.find(c=>c.stable_id===native.ids[0])!.sketch).toEqual(native.initial.state.scenes[0].cuts[0].sketch);
 expect(getSurfaceHost().error).toBeNull();expect(noop).not.toHaveBeenCalledWith(expect.stringContaining('approval'));
 mkdirSync('evidence/sb15',{recursive:true});writeFileSync('evidence/sb15/ui-readback.json',JSON.stringify({calls,headings:[...root.querySelectorAll('.workflow-sketch-card h3')].map(e=>e.textContent),provider_requests:0,original_cut_1_sketch:current.state.scenes[0].cuts.find(c=>c.stable_id===native.ids[0])!.sketch},null,2));
});
