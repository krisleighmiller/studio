import { expect, it, vi } from 'vitest';
import { createSpendConfirmationHandler } from '../src/spendConfirmation';
const payload=()=>({projectRoot:'/fixture',provider:'atlas-cloud',category:'media',operation:'atlas_generate_audio',model:'fixture',ownerRequestId:'ownerone',workflowContinuationBound:true,quote:{currency:'USD',requestHash:'a'.repeat(64),ledgerRevision:'rev',disclosure:'Exact operation'},args:{model:'fixture'}});
it('does not mint or dispatch a bound media permission after its UI owner was abandoned',async()=>{
 const invoke=vi.fn(async()=> 'must not dispatch');
 const h=createSpendConfirmationHandler({invoke,ask:async()=>true,projectRoot:()=>'/fixture',status:vi.fn(),ownerIsCurrent:()=>false});
 await h(payload());
 expect(invoke).not.toHaveBeenCalled();
});
it('rechecks bound ownership after permission issuance before sending paid media',async()=>{
 let live=true;let resolve!:(s:string)=>void;const token=new Promise<string>(r=>resolve=r);
 const invoke=vi.fn(async(cmd:string)=>cmd==='confirm_unbounded_spend'?token:'must not dispatch');
 const h=createSpendConfirmationHandler({invoke,ask:async()=>true,projectRoot:()=>'/fixture',status:vi.fn(),ownerIsCurrent:()=>live});
 const work=h(payload());for(let i=0;i<8;i++)await Promise.resolve();
 expect(invoke).toHaveBeenCalledTimes(1);live=false;resolve('permit');await work;
 expect(invoke.mock.calls.filter(([c])=>c==='media_generate_confirmed')).toHaveLength(0);
});
