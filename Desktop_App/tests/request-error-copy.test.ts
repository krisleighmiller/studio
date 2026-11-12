import { expect, it, vi } from 'vitest';
import { beginRequest, settleRequest, renderRequestReceipts } from '../src/studio/requestReceipt';

it('copies the exact error from a failed trim request without selecting text', async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', { value: {writeText}, configurable: true });
  const receipt = beginRequest('/copy-error-fixture', 'production_notes', 'Saving selected-take edit', 'workflow_set_shot_trim', {sceneId:1})!;
  settleRequest(receipt, 'error', 'Edit window exceeds the selected source; adjust duration or trim');
  const root = renderRequestReceipts('/copy-error-fixture', 'production_notes')!;
  const button = [...root.querySelectorAll('button')].find(item => item.textContent === 'Copy error');
  expect(button).toBeDefined();
  button!.click();
  await vi.waitFor(() => expect(writeText).toHaveBeenCalledWith('Edit window exceeds the selected source; adjust duration or trim'));
});
