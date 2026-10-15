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

  it("does not call the host after rejection or on an untrusted/mismatched quote", async () => {
    const rejected = setup(false);
    await rejected.handle(payload());
    expect(rejected.invoke).not.toHaveBeenCalled();
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
    expect(status).toHaveBeenCalledWith("Paid generation blocked: stale ledger revision");

    invoke.mockClear();
    invoke.mockImplementation(async (command: string) => {
      if (command === "confirm_spend") return "expired-token";
      throw Error("stale confirmation token");
    });
    await handle(payload());
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(status).toHaveBeenCalledWith("Paid generation blocked: stale confirmation token");
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
});
