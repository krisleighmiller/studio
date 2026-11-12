import type { SurfaceId } from './surfaces';
import type { JobRecord } from '../workflowTypes';

export type RequestOutcome = 'working' | 'queued' | 'running' | 'pending' | 'returned' | 'saved' | 'success' | 'error' | 'cancelled';
export type RequestReceipt = {
  id: string;
  projectRoot: string;
  surface: SurfaceId;
  label: string;
  context: string;
  submittedText: string;
  outcome: RequestOutcome;
  detail: string;
  active: boolean;
  key: string;
  jobId?: string;
  stop?: { isCurrent: () => boolean; run: () => void };
};

// UI receipts, not native jobs or authorization. Deliberately renderer-local:
// switching panels/projects retains them; restarting does not persist prompts.
const receipts = new Map<string, RequestReceipt[]>();
let sequence = 0;
function stable(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(stable);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => [k, stable(v)]));
  return value;
}

export function beginRequest(projectRoot: string, surface: SurfaceId, label: string, cmd: string, args: Record<string, unknown>): RequestReceipt | null {
  const list = receipts.get(projectRoot) ?? [];
  const key = JSON.stringify([cmd, stable(args)]);
  if (list.some(r => r.active && r.key === key)) return null;
  const text = ['title', 'notes', 'prompt', 'brief', 'instructions'].flatMap(k => typeof args[k] === 'string' && args[k] ? [`${k}: ${args[k]}`] : []).join('\n');
  const context = [args.sceneId != null ? `Scene ${args.sceneId}` : '', args.cut != null ? `Cut ${args.cut}` : ''].filter(Boolean).join(' · ');
  const receipt: RequestReceipt = {id: `studio-request-${++sequence}`, projectRoot, surface, label, context, submittedText: text, outcome: 'working', detail: 'Work is continuing. Keep Studio open while the host responds.', active: true, key};
  // Bound settled history, never evict an in-flight owner/deduplication guard.
  const retained = list.filter(r => r.active || list.filter(x => !x.active).slice(-19).includes(r));
  receipts.set(projectRoot, [...retained, receipt]);
  return receipt;
}

export function handoffRequestToJob(receipt: RequestReceipt, jobId: string): void {
  receipt.jobId = jobId;
  receipt.outcome = 'queued';
  receipt.detail = `Queued native job ${jobId}. Jobs owns authoritative progress.`;
  receipt.active = true;
}

export function syncRequestJobProgress(projectRoot: string, jobs: JobRecord[]): void {
  const byId = new Map(jobs.map(job => [job.id, job]));
  for (const receipt of receipts.get(projectRoot) ?? []) {
    if (!receipt.active || !receipt.jobId) continue;
    const job = byId.get(receipt.jobId);
    const status = job?.status?.toLowerCase();
    if (status === 'completed' && job?.output_path?.trim()) {
      receipt.outcome = 'saved';
      receipt.detail = `Output saved to ${job.output_path}.`;
      receipt.active = false;
      continue;
    }
    if (status !== 'running' && status !== 'in_progress') continue;
    receipt.outcome = 'running';
    receipt.detail = `Running native job ${receipt.jobId}. Jobs owns authoritative progress.`;
  }
}

export function settleRequest(receipt: RequestReceipt, outcome: Exclude<RequestOutcome, 'working' | 'queued' | 'running'>, detail: string): void {
  receipt.active = false;
  receipt.outcome = outcome;
  receipt.detail = detail;
}

export function activeRequestSummary(projectRoot: string | null): string | null {
  const active = (receipts.get(projectRoot ?? '') ?? []).filter(receipt => receipt.active).slice().reverse();
  if (!active.length) return null;
  const receipt = active[0];
  const job = receipt.outcome === 'queued' || receipt.outcome === 'running';
  const count = `${active.length} active ${job ? 'job' : 'request'}${active.length === 1 ? '' : 's'}`;
  const status = receipt.outcome === 'queued' ? 'Queued' : receipt.outcome === 'running' ? 'Running' : 'Working';
  return `${count} · ${status} — ${receipt.label}`;
}

export function renderActiveRequestIndicator(projectRoot: string | null, onReturn: (surface: SurfaceId) => void): HTMLElement | null {
  const active = (receipts.get(projectRoot ?? '') ?? []).filter(receipt => receipt.active).slice().reverse();
  if (!active.length) return null;
  const indicator = document.createElement('section');
  indicator.className = 'workflow-banner';
  indicator.dataset.globalRequestIndicator = '';
  const receipt = active[0];
  indicator.setAttribute('role', 'button');
  indicator.tabIndex = 0;
  indicator.setAttribute('aria-label', `Return to ${receipt.surface} for ${receipt.label}`);
  const returnToOrigin = () => onReturn(receipt.surface);
  indicator.addEventListener('click', returnToOrigin);
  indicator.addEventListener('keydown', event => {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    returnToOrigin();
  });
  const heading = document.createElement('strong');
  heading.textContent = activeRequestSummary(projectRoot);
  indicator.append(heading);
  return indicator;
}

export function renderRequestReceipts(projectRoot: string | null, surface: SurfaceId): HTMLElement | null {
  const list = (receipts.get(projectRoot ?? '') ?? []).filter(r => r.surface === surface).slice().reverse();
  if (!list.length) return null;
  const region = document.createElement('section');
  region.className = 'workflow-request-receipts';
  // Adjacent forms use a shared negative-margin field hint.
  region.style.paddingBottom = '12px';
  region.setAttribute('aria-label', 'Request receipts');
  const render = (receipt: RequestReceipt) => {
    const card = document.createElement('article');
    card.className = `workflow-banner ${receipt.outcome === 'error' ? 'warn' : ''}`;
    card.dataset.requestReceipt = receipt.id;
    card.dataset.requestOutcome = receipt.outcome;
    if (receipt.jobId) card.dataset.requestJobId = receipt.jobId;
    card.setAttribute('role', receipt.outcome === 'error' ? 'alert' : 'status');
    card.setAttribute('aria-atomic', 'true');
    const heading = document.createElement('strong');
    const labels: Record<RequestOutcome, string> = {working: 'Working', queued: 'Queued job', running: 'Running job', pending: 'Pending approval', returned: 'Draft returned', saved: 'Output saved', success: 'Success', error: 'Error', cancelled: 'Cancelled'};
    heading.textContent = `Submitted · ${labels[receipt.outcome]} — ${receipt.label}`;
    const detail = document.createElement('p');
    detail.textContent = `${receipt.context ? `${receipt.context} · ` : ''}${receipt.detail}`;
    card.append(heading, detail);
    if (receipt.outcome === 'error') {
      const copy = document.createElement('button');
      copy.type = 'button';
      copy.textContent = 'Copy error';
      const feedback = document.createElement('span');
      feedback.setAttribute('role', 'status');
      copy.addEventListener('click', async () => {
        try {
          await navigator.clipboard.writeText(receipt.detail);
          feedback.textContent = ' Error copied.';
        } catch {
          feedback.textContent = ' Could not copy. Select the error text and copy it instead.';
        }
      });
      card.append(copy, feedback);
    }
    if (receipt.outcome === 'pending' && receipt.stop?.isCurrent()) {
      const stop = document.createElement('button'); stop.textContent = 'Stop operation';
      stop.onclick = () => receipt.stop?.run(); card.append(stop);
    }
    if (receipt.submittedText) {
      const text = document.createElement('pre');
      text.style.whiteSpace = 'pre-wrap'; text.style.overflowWrap = 'anywhere';
      text.textContent = receipt.submittedText;
      card.append(text);
    }
    return card;
  };
  region.append(render(list[0]));
  if (list.length > 1) {
    const history = document.createElement('details');
    const summary = document.createElement('summary'); summary.textContent = `Earlier requests (${list.length - 1})`;
    history.append(summary, ...list.slice(1).map(render)); region.append(history);
  }
  const hint = document.createElement('p');
  hint.className = 'workflow-field-hint'; hint.style.marginTop = '8px';
  hint.textContent = 'Receipts stay on this surface during this app session. Preparation success is not media completion; Jobs and native approval remain authoritative.';
  region.append(hint);
  return region;
}
