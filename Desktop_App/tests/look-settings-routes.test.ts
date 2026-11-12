import { expect, it } from 'vitest';
import { imageCapabilityRoutes, defaultSettings } from '../src/settingsTypes';

it('shows all Settings image slots with configured provider and model without changing settings', () => {
  const settings = defaultSettings();
  settings.apis = [{id:'api-a', name:'Provider A', kind:'atlas-cloud', baseUrl:'', apiKey:'', secretKey:'', enabled:true, catalog:[], catalogFetchedAt:null, pinnedLlmModelIds:[]}];
  settings.capabilityDefaults['image.image_to_image'] = {apiId:'api-a', modelId:'model-x'};
  const before = JSON.stringify(settings);
  const routes = imageCapabilityRoutes(settings);
  expect(routes.map(r => r.id)).toEqual(['image.edit','image.image_to_image','image.character_reference','image.text_to_image']);
  expect(routes[1].label).toContain('Provider A');
  expect(routes[1].label).toContain('model-x');
  expect(routes[0].label).toContain('not configured');
  expect(JSON.stringify(settings)).toBe(before);
});
