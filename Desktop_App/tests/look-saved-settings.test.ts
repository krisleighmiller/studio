import { expect, it, vi } from 'vitest';
const host=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('../src/host',()=>host);
import { getSavedSettings, loadAppSettings } from '../src/settingsPanel';
it('returns an isolated snapshot of persisted settings for request routing',async()=>{
  host.invoke.mockResolvedValue({apis:[],capabilityDefaults:{},adultCapabilityDefaults:{},adultContentEnabled:false,selectedChatModel:null});
  await loadAppSettings();
  const first=getSavedSettings(); first.capabilityDefaults['image.edit']={apiId:'unsaved',modelId:'unsaved'};
  expect(getSavedSettings().capabilityDefaults['image.edit']).toBeUndefined();
});
