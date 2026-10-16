import { minorToMoney } from "./studio/money";

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
type Request = { quote: BoundedQuote | UnboundedQuote; provider: string; operation: string; model: string; projectRoot: string; args: Record<string, unknown> };
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
  if ([provider, operation, model, q.currency, q.ledgerRevision].some((field) => typeof field !== "string" || !field) ||
      !operations.has(operation as string) ||
      (bounded ? q.category !== "media" || typeof q.capability !== "string" ||
        !Number.isSafeInteger(q.amountMinor) || (q.amountMinor as number) <= 0 :
        value.category !== "media" || typeof q.requestHash !== "string" || !/^[0-9a-f]{64}$/.test(q.requestHash) ||
        typeof q.disclosure !== "string" || !q.disclosure) ||
      typeof value.projectRoot !== "string" || !value.projectRoot ||
      value.args.model !== model || "spend_confirmation" in value.args ||
      "spendConfirmation" in value.args) return null;
  return { quote: q as BoundedQuote | UnboundedQuote, provider: provider as string,
    operation: operation as string, model: model as string, projectRoot: value.projectRoot, args: value.args };
}

/** The only renderer path to a paid media replay; the quote comes from the host event. */
export function createSpendConfirmationHandler(deps: {
  invoke: Invoke;
  ask: (message: string, options: { title: string; kind: "warning" }) => Promise<boolean>;
  projectRoot: () => string | null;
  status: (message: string) => void;
  onSubmitted?: (result: unknown) => void | Promise<void>;
}): (payload: unknown) => Promise<void> {
  // Authorization events can arrive while a previous dialog is still open.
  // Never drop them silently: distinct requests queue (bounded), duplicates
  // and overflow are rejected with an observable status and a resume path.
  const MAX_QUEUED = 4;
  const queue: Request[] = [];
  const seen = new Set<string>();
  let active = false;

  const keyOf = (request: Request): string => {
    // Bounded quotes share a ledger revision until something settles, so the
    // revision alone cannot distinguish two different operations quoted at
    // the same revision. The request args (model, params, save_path) are the
    // operation's identity for dedup purposes: an exact re-emission of the
    // same event serializes identically, a distinct operation does not.
    const identity =
      "requestHash" in request.quote ? request.quote.requestHash : request.quote.ledgerRevision;
    let argsKey = "";
    try {
      argsKey = JSON.stringify(request.args ?? null) ?? "";
    } catch {
      argsKey = "<unserializable>";
    }
    return `${request.projectRoot}|${request.operation}|${request.model}|${identity}|${argsKey}`;
  };

  async function processOne(request: Request): Promise<void> {
    const { quote, projectRoot, args, provider, operation, model } = request;
    const unbounded = "requestHash" in quote;
    let dispatchAttempted = false;
    try {
      const accepted = await deps.ask(
        `${operation} · ${model}\n` +
        (unbounded ? `${quote.disclosure}\nThis is NOT a guaranteed hard stop.\n` :
          `Estimated charge (not a verified maximum): ${minorToMoney(quote.amountMinor)} ${quote.currency}\n`) +
        `Ledger revision: ${quote.ledgerRevision}\n\nAuthorize this single media generation?`,
        { title: "Confirm paid generation", kind: "warning" },
      );
      if (!accepted || deps.projectRoot() !== projectRoot) {
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
      // Never retry this call: even if the response fails, the host may have consumed the token.
      dispatchAttempted = true;
      const result = await deps.invoke("media_generate_confirmed", {
        projectRoot, operation, mediaArgs: args, token,
      });
      deps.status(`Media generation submitted: ${String(result)}`);
      if (deps.onSubmitted) {
        try {
          await deps.onSubmitted(result);
        } catch {
          deps.status(`Media generation submitted: ${String(result)} (Jobs refresh failed)`);
        }
      }
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      if (!dispatchAttempted && /stale/i.test(detail)) {
        // Another completed operation changed the ledger revision after this
        // quote was issued. The host rejected the quote; nothing was charged
        // or dispatched, and only a fresh quote can proceed.
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
        // Revalidate before dispatch: a queued quote is only ever presented
        // against the project it was issued for, and confirm_spend itself
        // rejects a stale ledger revision or reused token.
        if (deps.projectRoot() !== next.projectRoot) {
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

  return (payload) => {
    const request = parseRequest(payload);
    if (!request) return Promise.resolve();
    if (deps.projectRoot() !== request.projectRoot) {
      deps.status(`Ignored a paid-generation authorization for a different project (${request.operation}); nothing was dispatched.`);
      return Promise.resolve();
    }
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
