import {mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import {afterEach, expect, it, vi} from 'vitest';
import * as desk from '../src/workflowDesk';
import {getSurfaceHost,resetSurfaceSession} from '../src/studio/surfaces/session';
const host=vi.hoisted(()=>({invoke:vi.fn(),ask:vi.fn(),openDialog:vi.fn(),toMediaUrlAsync:vi.fn()}));
vi.mock('../src/host',()=>host);
const native=JSON.parse(readFileSync('tests/fixtures/sb16-native-reopen.json','utf8'));
afterEach(()=>{desk.clearWorkflowDesk();resetSurfaceSession();document.body.replaceChildren();vi.resetAllMocks();});
function button(root:Element,label:string){const b=[...root.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===label);expect(b,label).toBeDefined();expect(b!.disabled).toBe(false);return b!;}
function card(root:Element){return root.querySelector('.workflow-sketch-card')!;}
async function mount(){const root=document.createElement('main');document.body.append(root);const noop=vi.fn();desk.bindWorkflowDesk({root,projectRoot:()=>'/fixture/sb16',onOpenPath:noop,onAddToChat:noop,onOpenVoice:noop,onComposeChat:noop,onSelectionChange:noop,chatModel:()=>null,onStatus:noop,onPendingPlan:noop});await desk.loadWorkflowDesk('/fixture/sb16');const nav=[...root.querySelectorAll<HTMLButtonElement>('.studio-surface-nav button')].find(b=>b.textContent?.includes('Cut Sketches'))!;expect(nav.disabled).toBe(false);nav.click();return root;}
it('requires explicit confirmed reopen before a separate remove and retains the survivor on reload',async()=>{
 let current=structuredClone(native.approved);const calls:{cmd:string,args:unknown}[]=[];
 host.invoke.mockImplementation(async(cmd:string,args:Record<string,unknown>)=>{calls.push({cmd,args});if(cmd==='workflow_import')return current;if(cmd==='workflow_reopen_sketch'){expect(args).toMatchObject({sceneId:1,shotId:native.ids[0]});current=structuredClone(native.reopened);return current;}if(cmd==='workflow_remove_cut'){if(current.state.scenes[0].animatic.approved)throw Error('Cut 1 has source, media, production, or approved work. Nothing was removed.');current=structuredClone(native.removed);return current;}throw Error(`Forbidden ${cmd}`);});
 host.ask.mockResolvedValue(true);const root=await mount();
 button(card(root),'Remove cut').click();await vi.waitFor(()=>expect(getSurfaceHost().error).toContain('approved work'));expect(root.textContent).toContain('Nothing was removed');
 host.ask.mockResolvedValueOnce(false);button(card(root),'Reopen sketch for revision').click();await vi.waitFor(()=>expect(host.ask).toHaveBeenCalledTimes(2));expect(calls.filter(c=>c.cmd==='workflow_reopen_sketch')).toHaveLength(0);
 button(card(root),'Reopen sketch for revision').click();await vi.waitFor(()=>expect(calls.filter(c=>c.cmd==='workflow_reopen_sketch')).toHaveLength(1));await vi.waitFor(()=>expect(root.textContent).not.toContain('Reopen sketch for revision'));
 expect(host.ask).toHaveBeenLastCalledWith(expect.stringContaining("clears the scene's sketch approval"),expect.anything());
 expect(root.querySelectorAll('.workflow-sketch-card')).toHaveLength(2);expect(getSurfaceHost().error).toBeNull();
 button(card(root),'Remove cut').click();await vi.waitFor(()=>expect(root.querySelectorAll('.workflow-sketch-card')).toHaveLength(1));expect(root.textContent).toContain('Empty sketch 2');
 current=structuredClone(native.imported);desk.clearWorkflowDesk();await desk.loadWorkflowDesk('/fixture/sb16');
 const reopenedNav=[...root.querySelectorAll<HTMLButtonElement>('.studio-surface-nav button')].find(b=>b.textContent?.includes('Cut Sketches'))!;expect(reopenedNav.disabled).toBe(false);reopenedNav.click();
 expect(root.querySelectorAll('.workflow-sketch-card')).toHaveLength(1);expect(card(root).querySelector('h3')?.textContent).toContain('Empty sketch 2');expect(root.textContent).not.toContain('Empty sketch 1');
 expect(current.state.scenes[0].cuts[0].stable_id).toBe(native.ids[1]);expect(current.state.scenes[0].cuts).toHaveLength(1);
 mkdirSync('evidence/sb16',{recursive:true});
 writeFileSync('evidence/sb16/ui-readback.json',JSON.stringify({calls,confirmations:host.ask.mock.calls,headings:[...root.querySelectorAll('h3')].map(x=>x.textContent),provider_requests:0},null,2));
});
it('shows protected-work refusal without claiming a reopen succeeded',async()=>{
 host.ask.mockResolvedValue(true);host.invoke.mockImplementation(async(cmd:string)=>{if(cmd==='workflow_import')return structuredClone(native.approved);if(cmd==='workflow_reopen_sketch')throw Error(native.real_copy.refusals[0].error);throw Error(`Forbidden ${cmd}`);});
 const root=await mount();button(card(root),'Reopen sketch for revision').click();await vi.waitFor(()=>expect(getSurfaceHost().error).toContain('source, media, production'));
 expect(root.textContent).toContain('Nothing was removed');expect(root.querySelectorAll('.workflow-sketch-card')).toHaveLength(2);expect(button(card(root),'Reopen sketch for revision')).toBeDefined();
});
it('does not apply stale confirmation to a same-root reopened project',async()=>{
 let accept!:(value:boolean)=>void;host.ask.mockImplementation(()=>new Promise<boolean>(resolve=>accept=resolve));host.invoke.mockImplementation(async(cmd:string)=>{if(cmd==='workflow_import')return structuredClone(native.approved);throw Error(`Forbidden ${cmd}`);});
 const root=await mount();button(card(root),'Reopen sketch for revision').click();desk.clearWorkflowDesk();await desk.loadWorkflowDesk('/fixture/sb16');accept(true);await new Promise(resolve=>setTimeout(resolve,0));expect(host.invoke.mock.calls.map(c=>c[0])).toEqual(['workflow_import','workflow_import']);
});
