import { minorToMoney } from "./studio/money";

type Quote = {
  provider: string;
  category: string;
  operation: string;
  capability: string;
  model: string;
  currency: string;
  amountMinor: number;
  ledgerRevision: string;
};

type Request = { quote: Quote; projectRoot: string; args: Record<string, unknown> };
type Invoke = (command: string, args: Record<string, unknown>) => Promise<unknown>;

const operations = new Set(["atlas_generate_image", "atlas_generate_video", "atlas_generate_audio"]);
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);

function parseRequest(value: unknown): Request | null {
  if (!record(value) || !record(value.quote) || !record(value.args)) return null;
  const q = value.quote;
  const fields = [q.provider, q.category, q.operation, q.capability, q.model, q.currency, q.ledgerRevision];
  if (fields.some((field) => typeof field !== "string" || !field) ||
      q.category !== "media" || !operations.has(q.operation as string) ||
      !Number.isSafeInteger(q.amountMinor) || (q.amountMinor as number) <= 0 ||
      typeof value.projectRoot !== "string" || !value.projectRoot ||
      value.args.model !== q.model || "spend_confirmation" in value.args ||
      "spendConfirmation" in value.args) return null;
  return { quote: q as Quote, projectRoot: value.projectRoot, args: value.args };
}

/** The only renderer path to a paid media replay; the quote comes from the host event. */
export function createSpendConfirmationHandler(deps: {
  invoke: Invoke;
  ask: (message: string, options: { title: string; kind: "warning" }) => Promise<boolean>;
  projectRoot: () => string | null;
  status: (message: string) => void;
}): (payload: unknown) => Promise<void> {
  let pending = false;
  return async (payload) => {
    const request = parseRequest(payload);
    if (!request || pending || deps.projectRoot() !== request.projectRoot) return;
    pending = true;
    const { quote, projectRoot, args } = request;
    try {
      const accepted = await deps.ask(
        `${quote.operation} · ${quote.model}\n` +
        `Estimated charge: ${minorToMoney(quote.amountMinor)} ${quote.currency}\n` +
        `Ledger revision: ${quote.ledgerRevision}\n\nAuthorize this single media generation?`,
        { title: "Confirm paid generation", kind: "warning" },
      );
      if (!accepted || deps.projectRoot() !== projectRoot) return;
      const token = await deps.invoke("confirm_spend", {
        projectRoot, provider: quote.provider, category: quote.category,
        operation: quote.operation, capability: quote.capability, model: quote.model,
        amountMinor: quote.amountMinor, ledgerRevision: quote.ledgerRevision,
      });
      if (typeof token !== "string" || !token) throw new Error("Host did not issue a spend token");
      // Never retry this call: even if the response fails, the host may have consumed the token.
      const result = await deps.invoke("media_generate_confirmed", {
        projectRoot, operation: quote.operation, mediaArgs: args, token,
      });
      deps.status(`Media generation submitted: ${String(result)}`);
    } catch (error) {
      deps.status(`Paid generation blocked: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      pending = false;
    }
  };
}
