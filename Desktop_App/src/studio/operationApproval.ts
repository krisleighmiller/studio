import './generationPrompt.css';

let cancelActive: (() => void) | null = null;
export function cancelOperationApproval(): void { cancelActive?.(); }
export function operationApprovalOpen(): boolean { return cancelActive !== null; }

/** Display host text verbatim as text, not executable markup. The host validates expiry/replay. */
export function approveExactOperation(text: string): Promise<boolean> {
  if (cancelActive) return Promise.resolve(false);
  return new Promise(resolve => {
    const previous = document.activeElement as HTMLElement | null;
    const dialog = document.createElement('dialog');
    dialog.className = 'studio-generation-prompt'; dialog.dataset.operationApproval = '';
    dialog.dataset.approvalId = /\[\[workbench_approve:([A-Za-z0-9]+)\]\]/.exec(text)?.[1] ?? '';
    dialog.setAttribute('aria-labelledby', 'studio-operation-title');
    const title = document.createElement('h2'); title.id = 'studio-operation-title'; title.textContent = 'Authorize exact operation';
    const help = document.createElement('p');
    help.textContent = 'Inspect these host-prepared inputs. This one-use approval cannot edit them. Deny, then prepare again to change the prompt. Paid media requires a separate confirmation; approving this card alone is not spending consent. The host refuses expired or previously consumed IDs.';
    const body = document.createElement('pre'); body.textContent = text.replace(/\[\[workbench_approve:[A-Za-z0-9]+\]\]/g, '');
    const deny = document.createElement('button'); deny.textContent = 'Deny operation';
    const approve = document.createElement('button'); approve.textContent = 'Approve exact operation';
    let settled = false;
    const finish = (accepted: boolean) => {
      if (settled) return; settled = true; cancelActive = null;
      approve.disabled = deny.disabled = true;
      dialog.close(); dialog.remove(); if(previous?.isConnected)previous.focus(); resolve(accepted);
    };
    cancelActive = () => finish(false);
    deny.onclick = () => finish(false); approve.onclick = () => finish(true);
    dialog.addEventListener('cancel', e => { e.preventDefault(); finish(false); });
    dialog.addEventListener('close', () => finish(false));
    dialog.append(title, help, body, deny, approve); document.body.append(dialog); dialog.showModal(); deny.focus();
  });
}
