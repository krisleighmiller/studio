import { describe, expect, it, vi } from "vitest";
import { createSpendConfirmationHandler } from "../src/spendConfirmation";

const payload = () => ({
  projectRoot: "/project",
  quote: {
    provider: "atlas-cloud", category: "media", operation: "atlas_generate_image",
    capability: "image.text_to_image", model: "image-model", currency: "USD",
    amountMinor: 123, ledgerRevision: "revision-a",
  },
  args: { model: "image-model", params: { prompt: "portrait" } },
});

function setup(accept = true) {
  const invoke = vi.fn(async (command: string): Promise<string> => {
    if (command === "confirm_spend") return "one-use-token";
    if (command === "media_generate_confirmed") return "saved";
    throw Error("unexpected command");
  });
  const ask = vi.fn(async () => accept);
  const status = vi.fn();
  const handle = createSpendConfirmationHandler({ invoke, ask, status, projectRoot: () => "/project" });
  return { invoke, ask, status, handle };
}

describe("paid media confirmation handoff (host mocked; no provider calls)", () => {
  it("notifies the application after confirmed dispatch so Jobs can refresh", async () => {
    const invoke = vi.fn(async (command: string): Promise<string> => {
      if (command === "confirm_spend") return "one-use-token";
      if (command === "media_generate_confirmed") return "saved";
      throw Error("unexpected command");
    });
    const onSubmitted = vi.fn(async () => undefined);
    const handle = createSpendConfirmationHandler({
      invoke,
      ask: vi.fn(async () => true),
      status: vi.fn(),
      projectRoot: () => "/project",
      onSubmitted,
    });

    await handle(payload());

    expect(onSubmitted).toHaveBeenCalledWith("saved");
  });

  it("shows the host quote and replays once with exact scope and token", async () => {
    const { handle, invoke, ask, status } = setup();
    await handle(payload());
    expect(ask).toHaveBeenCalledWith(expect.stringContaining("1.23 USD"), expect.anything());
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke).toHaveBeenNthCalledWith(1, "confirm_spend", {
      projectRoot: "/project", provider: "atlas-cloud", category: "media",
      operation: "atlas_generate_image", capability: "image.text_to_image", model: "image-model",
      amountMinor: 123, ledgerRevision: "revision-a",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "media_generate_confirmed", {
      projectRoot: "/project", operation: "atlas_generate_image",
      mediaArgs: payload().args, token: "one-use-token",
    });
    expect(status).toHaveBeenCalledWith("Media generation submitted: saved");
  });

  it("reports rejection and does not call the host after rejection or on an untrusted/mismatched quote", async () => {
    const rejected = setup(false);
    await rejected.handle(payload());
    expect(rejected.invoke).not.toHaveBeenCalled();
    expect(rejected.status).toHaveBeenCalledWith("Paid generation was not authorized; no provider request was sent.");
    const invalid = setup();
    await invalid.handle({ ...payload(), quote: { ...payload().quote, amountMinor: undefined } });
    await invalid.handle({ ...payload(), args: { model: "different" } });
    expect(invalid.ask).not.toHaveBeenCalled();
    expect(invalid.invoke).not.toHaveBeenCalled();
  });

  it("does not replay when the host rejects a stale confirmation or token", async () => {
    const { handle, invoke, status } = setup();
    invoke.mockImplementation(async (command: string) => {
      if (command === "confirm_spend") throw Error("stale ledger revision");
      throw Error("provider must not be called");
    });
    await handle(payload());
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(status).toHaveBeenCalledWith("Paid generation blocked before dispatch: stale ledger revision");

    invoke.mockClear();
    invoke.mockImplementation(async (command: string) => {
      if (command === "confirm_spend") return "expired-token";
      throw Error("stale confirmation token");
    });
    await handle(payload());
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(status).toHaveBeenCalledWith(expect.stringContaining("Paid generation outcome is uncertain"));
    expect(status).toHaveBeenCalledWith(expect.stringContaining("Do not retry automatically"));
  });

  it("drops a quote when the project changes during the confirmation dialog", async () => {
    let root = "/project";
    const invoke = vi.fn();
    const handle = createSpendConfirmationHandler({
      invoke, ask: async () => { root = "/other"; return true; },
      projectRoot: () => root, status: vi.fn(),
    });
    await handle(payload());
    expect(invoke).not.toHaveBeenCalled();
  });
  it("requires a fresh one-use authorization for two consecutive paid operations", async () => {
    let issued = 0;
    const invoke = vi.fn(async (command: string): Promise<string> => {
      if (command === "confirm_unbounded_spend") return `token-${++issued}`;
      if (command === "media_generate_confirmed") return "saved";
      throw Error("unexpected command");
    });
    const handle = createSpendConfirmationHandler({
      invoke,
      ask: vi.fn(async () => true),
      status: vi.fn(),
      projectRoot: () => "/project",
    });
    const unbounded = (hash: string) => ({
      ...payload(),
      provider: "atlas-cloud",
      category: "media",
      operation: "atlas_generate_image",
      model: "image-model",
      quote: {
        currency: "USD",
        requestHash: hash.repeat(64),
        ledgerRevision: `revision-${hash}`,
        disclosure: "No verified maximum exists for this request; charge may exceed budget.",
      },
    });

    await handle(unbounded("a"));
    await handle(unbounded("b"));

    expect(invoke).toHaveBeenCalledTimes(4);
    expect(invoke).toHaveBeenNthCalledWith(2, "media_generate_confirmed", expect.objectContaining({ token: "token-1" }));
    expect(invoke).toHaveBeenNthCalledWith(4, "media_generate_confirmed", expect.objectContaining({ token: "token-2" }));
  });

  it("discloses unknown maximum and binds approval to the exact request hash", async () => {
    const { handle, invoke, ask } = setup();
    invoke.mockImplementation(async (command: string) =>
      command === "confirm_unbounded_spend" ? "one-use-token" : "saved");
    await handle({ ...payload(), provider: "atlas-cloud", category: "media",
      operation: "atlas_generate_image", model: "image-model",
      quote: { currency: "USD", requestHash: "a".repeat(64), ledgerRevision: "revision-a",
        disclosure: "No verified maximum exists for this request; charge may exceed budget." },
    });
    expect(ask).toHaveBeenCalledWith(expect.stringContaining("NOT a guaranteed hard stop"), expect.anything());
    expect(invoke).toHaveBeenNthCalledWith(1, "confirm_unbounded_spend", {
      projectRoot: "/project", provider: "atlas-cloud", category: "media",
      operation: "atlas_generate_image", model: "image-model",
      requestHash: "a".repeat(64), ledgerRevision: "revision-a",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "media_generate_confirmed", expect.objectContaining({token: "one-use-token"}));
  });
});
