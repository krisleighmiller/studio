import { expect, it } from 'vitest';
import { renderStartFrameCard } from '../src/studio/surfaces/referenceStage';
import { bindSurfaceHost, resetSurfaceSession } from '../src/studio/surfaces/session';
import type { SurfaceHost } from '../src/studio/surfaces/host';
import type { SceneRecord } from '../src/workflowTypes';

it('passes explicitly selected images in order and selects none by default', () => {
  resetSurfaceSession();
  const scene = { id:1, title:'A', cuts:[{id:{source_scene:1,cut:1},takes:[]}] } as unknown as SceneRecord;
  bindSurfaceHost({busy:null,error:null,chatReady:true,state:{scenes:[scene],assets:[{id:'wardrobe',path:'wardrobe.png',status:'draft'},{id:'clinic',path:'clinic.png',status:'approved'}]},resolvePath:(p:string)=>p,mediaUrl:async(p:string)=>p,continuityDependents:()=>[]} as unknown as SurfaceHost);
  const intents:unknown[]=[];
  const view=renderStartFrameCard(scene,scene.cuts[0],i=>intents.push(i));
  const generate=[...view.querySelectorAll('button')].find(b=>b.textContent==='Generate start frame')!;
  generate.click(); expect(intents.pop()).toEqual({type:'generate',cut:1,referenceAssetIds:[]});
  const picker=view.querySelector<HTMLSelectElement>('select[aria-label="Reference image"]')!;
  const add=view.querySelector<HTMLButtonElement>('button[data-add-reference]')!;
  picker.value='clinic';add.click();picker.value='wardrobe';add.click();generate.click();
  expect(intents.pop()).toEqual({type:'generate',cut:1,referenceAssetIds:['clinic','wardrobe']});
});
