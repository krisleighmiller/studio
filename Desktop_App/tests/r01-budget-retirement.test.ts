import {beforeEach,describe,expect,it,vi} from 'vitest';
import {renderProjectProfile} from '../src/studio/surfaces/profileStage';
import {bindSurfaceHost,resetSurfaceSession,setProfileOpen} from '../src/studio/surfaces/session';
import type {SurfaceHost} from '../src/studio/surfaces/host';
import type {ProfileIntent} from '../src/studio/surfaces/surfaceIntents';
import {authorizeVoiceSpendWith} from '../src/voicePanel';
import {createSpendConfirmationHandler} from '../src/spendConfirmation';
import {buildJobsModel} from '../src/studio/jobsModel';
import {renderJobsView} from '../src/studio/jobsView';
const forbidden=/budget|accounting|ledger revision|USD|EUR|verified.*maximum|cost estimate|estimated charge|token (count|usage)/i;
beforeEach(()=>{resetSurfaceSession();setProfileOpen(true);});
describe('R01 ordinary workflow without accounting',()=>{
 it('Jobs keeps operational status/error/recovery but no monetary facts or tooltips',()=>{
 const m=buildJobsModel({jobs:[{id:'spend-original',kind:'image',status:'failed',provider:'fixture',cost_minor:121,estimated_cost_minor:333,currency:'USD',error:'fixture failure',spend_category:'media',request_metadata:{request_hash:'a'.repeat(64),prediction_id:'r01-original'}}],scenes:[],assets:[],filter:'all'});
 const view=renderJobsView(m,true,'all',()=>{});expect(view.textContent).toContain('fixture failure');expect(view.textContent).toContain('r01-original');expect(view.textContent).toContain('Check provider status');expect(view.textContent).not.toMatch(/Estimate |Actual |Currency |accounting|reconciliation|1\.21|3\.33|USD/);expect(view.querySelector('button.studio-job')?.title).not.toMatch(/estimate|actual|currency|accounting/i);
 });
 it.each([null,{currency:'EUR',limit_minor:0,hard_stop:true,policy:'strict_hard_stop'}])('has no budget settings even for legacy configured data %j',budget=>{
 const state={version:3,project_name:'R01',characters:[],scenes:[],assets:[],jobs:[],requirements:[],notes:[],budget,profile:{budget:'legacy preserve me',schedule:'R01 retained schedule',medium:'film'}};
 bindSurfaceHost({state,busy:null,error:null,report:null,activeRecipe:()=>null} as unknown as SurfaceHost);const intents:ProfileIntent[]=[];const view=renderProjectProfile(i=>intents.push(i));
 expect(view.querySelector('.workflow-budget-editor')).toBeNull();expect(view.querySelector('[name="budget"]')).toBeNull();expect(view.textContent).not.toMatch(forbidden);
 const form=view.querySelector('.workflow-profile-form')!;form.dispatchEvent(new Event('submit',{cancelable:true}));const saved=intents.find(i=>i.type==='save');expect(saved).toBeDefined();expect(saved).toMatchObject({profile:{budget:'legacy preserve me',schedule:'R01 retained schedule'}});
 });
 it('voice permission names the operation without accounting details and decline issues no token',async()=>{
 const invoke=vi.fn(async(c:string)=>c==='elevenlabs_quote_voice_spend'?{policy:'approval_required',requestHash:'a'.repeat(64),ledgerRevision:'opaque-revision',disclosure:'Authorize this voice operation.'}:'one-use');const confirm=vi.fn((_message:string)=>false);
 await expect(authorizeVoiceSpendWith({invoke,confirm,projectRoot:()=>'/project'},'elevenlabs_tts_preview','eleven_multilingual_v2',JSON.stringify({text:'hello',voice_id:'v'}))).rejects.toThrow(/cancelled/);expect(invoke).toHaveBeenCalledTimes(1);expect(confirm.mock.calls[0]?.[0]).not.toMatch(forbidden);expect(confirm.mock.calls[0]?.[0]).toContain('elevenlabs_tts_preview');
 });
 it('media permission carries only operation scope in the visible dialog and dispatches once',async()=>{
 const ask=vi.fn(async(_message:string,_options:unknown)=>true);const invoke=vi.fn(async(c:string)=>c==='confirm_unbounded_spend'?'one-use':'saved');const handle=createSpendConfirmationHandler({invoke,ask,projectRoot:()=>'/project',status:vi.fn()});
 await handle({projectRoot:'/project',provider:'unpriced-provider',category:'media',operation:'atlas_generate_image',model:'fixture-model',quote:{currency:'',requestHash:'a'.repeat(64),ledgerRevision:'opaque-revision',disclosure:'Authorize this operation.'},args:{model:'fixture-model',params:{prompt:'empty room'}}});
 expect(ask).toHaveBeenCalledTimes(1);expect(ask.mock.calls[0]?.[0]).toContain('atlas_generate_image');expect(ask.mock.calls[0]?.[0]).not.toMatch(forbidden);expect(invoke).toHaveBeenCalledTimes(2);
 });
});
