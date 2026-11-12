type BoundedQuote = {
  provider: string;
  category: string;
  operation: string;
  capability: string;
  model: string;
  currency: string;
  amountMinor: number;
  ledgerRevision: string;
};
type UnboundedQuote = { currency: string; requestHash: string; ledgerRevision: string; disclosure: string };
type Request = { quote: BoundedQuote | UnboundedQuote; provider: string; operation: string; model: string; projectRoot: string; args: Record<string, unknown>; epoch: number; ownerRequestId?: string; workflowBound: boolean };
type Invoke = (command: string, args: Record<string, unknown>) => Promise<unknown>;

const operations = new Set(["atlas_generate_image", "atlas_generate_video", "atlas_generate_audio"]);
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);

function parseRequest(value: unknown): Request | null {
  if (!record(value) || !record(value.quote) || !record(value.args)) return null;
  const q = value.quote;
  const bounded = typeof q.amountMinor === "number";
  const provider = bounded ? q.provider : value.provider;
  const operation = bounded ? q.operation : value.operation;
  const model = bounded ? q.model : value.model;
  if ([provider, operation, model, q.ledgerRevision].some((field) => typeof field !== "string" || !field) ||
      !operations.has(operation as string) ||
      (bounded ? typeof q.currency !== "string" || !q.currency || q.category !== "media" || typeof q.capability !== "string" ||
        !Number.isSafeInteger(q.amountMinor) || (q.amountMinor as number) <= 0 :
        value.category !== "media" || typeof q.requestHash !== "string" || !/^[0-9a-f]{64}$/.test(q.requestHash) ||
        typeof q.disclosure !== "string" || !q.disclosure) ||
      typeof value.projectRoot !== "string" || !value.projectRoot ||
      value.args.model !== model || "spend_confirmation" in value.args ||
      "spendConfirmation" in value.args) return null;
  return { quote: q as BoundedQuote | UnboundedQuote, provider: provider as string,
    operation: operation as string, model: model as string, projectRoot: value.projectRoot, args: value.args, epoch: 0, workflowBound: value.workflowContinuationBound === true, ownerRequestId: typeof value.ownerRequestId === "string" && value.ownerRequestId.length <= 128 ? value.ownerRequestId : undefined };
}

/** The only renderer path to a paid media replay; the quote comes from the host event. */
export function createSpendConfirmationHandler(deps: {
  invoke: Invoke;
  ask: (message: string, options: { title: string; kind: "warning" }) => Promise<boolean>;
  projectRoot: () => string | null;
  projectEpoch?: () => number;
  ownerIsCurrent?: (ownerRequestId: string | undefined) => boolean;
  authorizeWorkflowMedia?: (request: { ownerRequestId?: string; operation: string; args: Record<string, unknown> }) => boolean | undefined;
  status: (message: string) => void;
  onSubmitted?: (result: unknown, isCurrent: () => boolean, ownerRequestId?: string) => void | Promise<void>;
  onProgress?: (isCurrent: () => boolean) => void | Promise<void>;
  onSettled?: (outcome: { success: boolean; result?: unknown; error?: unknown }, isCurrent: () => boolean, ownerRequestId?: string) => void | Promise<void>;
}): (payload: unknown) => Promise<void> {
  const MAX_QUEUED = 4;
  const queue: Request[] = [];
  const seen = new Set<string>();
  let active = false;

  const getEpoch = () => typeof deps.projectEpoch === "function" ? deps.projectEpoch() : 0;

  const keyOf = (request: Request): string => {
    const identity =
      "requestHash" in request.quote ? request.quote.requestHash : request.quote.ledgerRevision;
    let argsKey = "";
    try {
      argsKey = JSON.stringify(request.args ?? null) ?? "";
    } catch {
      argsKey = "<unserializable>";
    }
    return `${request.projectRoot}|${request.epoch}|${request.operation}|${request.model}|${identity}|${argsKey}`;
  };

  async function processOne(request: Request): Promise<void> {
    const { quote, projectRoot, args, provider, operation, model, epoch } = request;
    const unbounded = "requestHash" in quote;
    let dispatchAttempted = false;
    let settled = false;
    let pollTimer: ReturnType<typeof setTimeout> | null = null;

    const isCurrent = (): boolean => {
      const root = deps.projectRoot();
      const currentEpoch = getEpoch();
      return Boolean(root) && root === projectRoot && currentEpoch === epoch && (!request.workflowBound || Boolean(deps.ownerIsCurrent?.(request.ownerRequestId)));
    };

    try {
      if (!isCurrent()) return; // Never prompt for a retired workflow owner, even on same-root reopen.
      const scoped = request.workflowBound && isCurrent()
        ? deps.authorizeWorkflowMedia?.({ ownerRequestId: request.ownerRequestId, operation, args })
        : undefined;
      const accepted = scoped === undefined ? await deps.ask(
        `${provider} · ${operation} · ${model}\n\nAuthorize this single media generation?`,
        { title: "Confirm paid generation", kind: "warning" },
      ) : scoped;
      if (!accepted || !isCurrent()) {
        if (!accepted && isCurrent() && deps.onSettled && request.ownerRequestId) {
          await deps.onSettled({ success: false, error: new Error("Media generation denied; no provider request sent.") }, isCurrent, request.ownerRequestId);
        }
        if (!accepted) deps.status("Paid generation was not authorized; no provider request was sent.");
        else deps.status(`Paid generation (${operation} · ${model}) was dropped: the open project changed while its authorization was pending. It was not dispatched.`);
        return;
      }
      const token = unbounded ?
        await deps.invoke("confirm_unbounded_spend", {
          projectRoot, provider, category: "media", operation, model,
          requestHash: quote.requestHash, ledgerRevision: quote.ledgerRevision,
        }) :
        await deps.invoke("confirm_spend", {
          projectRoot, provider: quote.provider, category: quote.category,
          operation: quote.operation, capability: quote.capability, model: quote.model,
          amountMinor: quote.amountMinor, ledgerRevision: quote.ledgerRevision,
        });
      if (typeof token !== "string" || !token) throw new Error("Host did not issue a spend token");
      if (!isCurrent()) return; // Never dispatch into a replaced project session.
      dispatchAttempted = true;
      const startPoll = () => {
        const tick = async () => {
          if (settled || !isCurrent()) {
            if (pollTimer) { clearTimeout(pollTimer); pollTimer = null; }
            return;
          }
          try {
            if (deps.onProgress) {
              await deps.onProgress(() => !settled && isCurrent());
            }
          } catch { /* poll errors never resubmit or replace */ }
          if (!settled && isCurrent()) {
            pollTimer = setTimeout(tick, 500);
          } else {
            if (pollTimer) { clearTimeout(pollTimer); pollTimer = null; }
          }
        };
        pollTimer = setTimeout(tick, 500);
      };
      startPoll();
      const result = await deps.invoke("media_generate_confirmed", {
        projectRoot, operation, mediaArgs: args, token,
      });
      settled = true;
      if (pollTimer) { clearTimeout(pollTimer); pollTimer = null; }
      if (!isCurrent()) {
        // Scope changed after result; suppress final status/callbacks into replacement.
        return;
      }
      deps.status(`Media generation submitted: ${String(result)}`);
      if (deps.onSubmitted) {
        try {
          if (request.ownerRequestId !== undefined) await deps.onSubmitted(result, isCurrent, request.ownerRequestId);
          else await deps.onSubmitted(result, isCurrent);
        } catch {
          if (isCurrent()) deps.status(`Media generation submitted: ${String(result)} (Jobs refresh failed)`);
        }
      }
      if (deps.onSettled && isCurrent()) {
        try { await deps.onSettled({ success: true, result }, isCurrent); } catch { /* never replace outcome */ }
      }
    } catch (error) {
      settled = true;
      if (pollTimer) { clearTimeout(pollTimer); pollTimer = null; }
      if (!isCurrent()) {
        // Suppress stale final callbacks/status when scope changed.
        return;
      }
      const detail = error instanceof Error ? error.message : String(error);
      if (deps.onSettled && isCurrent()) {
        try {
          if (request.ownerRequestId) await deps.onSettled({ success: false, error }, isCurrent, request.ownerRequestId);
          else await deps.onSettled({ success: false, error }, isCurrent);
        } catch { /* never replace original outcome */ }
      }
      if (!isCurrent()) return;
      if (!dispatchAttempted && /stale/i.test(detail)) {
        deps.status(`Paid generation (${request.operation} · ${request.model}) was NOT dispatched: its authorization expired when another operation settled first. Re-run the operation from its Studio surface (Scene Look → Generate still → Approve) to get a fresh quote.`);
        return;
      }
      deps.status(dispatchAttempted
        ? `Paid generation outcome is uncertain: ${detail}. Do not retry automatically; inspect Jobs and check provider status when a prediction ID is available.`
        : `Paid generation blocked before dispatch: ${detail}`);
    }
  }

  async function drain(): Promise<void> {
    if (active) return;
    active = true;
    try {
      for (let next = queue.shift(); next; next = queue.shift()) {
        if (!isCurrentForQueue(next)) {
          seen.delete(keyOf(next));
          deps.status(`Paid generation (${next.operation} · ${next.model}) was discarded: it belongs to a different project. Re-run it from that project.`);
          continue;
        }
        await processOne(next);
        seen.delete(keyOf(next));
      }
    } finally {
      active = false;
    }
  }

  const isCurrentForQueue = (request: Request): boolean => {
    const root = deps.projectRoot();
    const currentEpoch = getEpoch();
    return Boolean(root) && root === request.projectRoot && currentEpoch === request.epoch;
  };

  return (payload) => {
    const request = parseRequest(payload);
    if (!request) return Promise.resolve();
    if (deps.projectRoot() !== request.projectRoot) {
      deps.status(`Ignored a paid-generation authorization for a different project (${request.operation}); nothing was dispatched.`);
      return Promise.resolve();
    }
    request.epoch = getEpoch();
    const key = keyOf(request);
    if (seen.has(key)) {
      deps.status(`Ignored a duplicate paid-generation authorization event (${request.operation} · ${request.model}); the original is still being handled.`);
      return Promise.resolve();
    }
    if (queue.length >= MAX_QUEUED) {
      deps.status(`Paid-generation authorization queue is full; ${request.operation} · ${request.model} was NOT dispatched and nothing was charged. Re-trigger the operation from its Studio surface once the current authorizations settle.`);
      return Promise.resolve();
    }
    seen.add(key);
    queue.push(request);
    if (active) {
      deps.status(`Queued a paid-generation authorization (${request.operation} · ${request.model}); it will be presented after the current one settles.`);
    }
    return drain();
  };
}
