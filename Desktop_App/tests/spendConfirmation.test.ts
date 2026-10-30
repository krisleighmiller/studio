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

    expect(onSubmitted).toHaveBeenCalledWith("saved", expect.any(Function));
  });

  it("shows the host quote and replays once with exact scope and token", async () => {
    const { handle, invoke, ask, status } = setup();
    await handle(payload());
    expect(ask).toHaveBeenCalledWith(expect.stringContaining("atlas_generate_image"), expect.anything());
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
    expect(status).toHaveBeenCalledWith(expect.stringContaining("authorization expired"));

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
    expect(ask).toHaveBeenCalledWith(expect.stringContaining("Authorize this single media generation"), expect.anything());
    expect(invoke).toHaveBeenNthCalledWith(1, "confirm_unbounded_spend", {
      projectRoot: "/project", provider: "atlas-cloud", category: "media",
      operation: "atlas_generate_image", model: "image-model",
      requestHash: "a".repeat(64), ledgerRevision: "revision-a",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "media_generate_confirmed", expect.objectContaining({token: "one-use-token"}));
  });

  it("queues a second distinct authorization while the first dialog is open", async () => {
    let releaseFirst!: (v: boolean) => void;
    const ask = vi
      .fn<() => Promise<boolean>>()
      .mockImplementationOnce(() => new Promise((res) => { releaseFirst = res; }))
      .mockImplementationOnce(async () => true);
    const issued: string[] = [];
    const invoke = vi.fn(async (command: string): Promise<string> => {
      if (command === "confirm_spend") return "token-a";
      if (command === "confirm_unbounded_spend") return "token-b";
      if (command === "media_generate_confirmed") { issued.push("dispatched"); return "saved"; }
      throw Error("unexpected command");
    });
    const status = vi.fn();
    const handle = createSpendConfirmationHandler({ invoke, ask, status, projectRoot: () => "/project" });

    const first = handle(payload());
    await vi.waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    // Second, distinct request arrives while the first dialog is open.
    const secondPayload = {
      ...payload(),
      provider: "atlas-cloud",
      category: "media",
      operation: "atlas_generate_image",
      model: "image-model",
      quote: { currency: "USD", requestHash: "b".repeat(64), ledgerRevision: "revision-b",
        disclosure: "No verified maximum exists for this request; charge may exceed budget." },
    };
    const second = handle(secondPayload);
    await vi.waitFor(() =>
      expect(status).toHaveBeenCalledWith(expect.stringContaining("Queued a paid-generation authorization")));
    // Still only one dialog; nothing for the second request has dispatched.
    expect(ask).toHaveBeenCalledTimes(1);
    expect(issued).toHaveLength(0);

    releaseFirst(true);
    await Promise.all([first, second]);
    expect(ask).toHaveBeenCalledTimes(2);
    // Each operation dispatched exactly once, with its own token.
    expect(issued).toHaveLength(2);
    expect(invoke).toHaveBeenNthCalledWith(2, "media_generate_confirmed", expect.objectContaining({ token: "token-a" }));
    expect(invoke).toHaveBeenNthCalledWith(4, "media_generate_confirmed", expect.objectContaining({ token: "token-b" }));
  });

  it("rejects duplicate and cross-project authorization events observably", async () => {
    let releaseFirst!: (v: boolean) => void;
    const ask = vi.fn(async () => true).mockImplementationOnce(
      () => new Promise((res) => { releaseFirst = res; }));
    const invoke = vi.fn(async (command: string): Promise<string> =>
      command === "confirm_spend" ? "one-use-token" : "saved");
    const status = vi.fn();
    let root: string | null = "/project";
    const handle = createSpendConfirmationHandler({ invoke, ask, status, projectRoot: () => root });

    const first = handle(payload());
    await vi.waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    // Exact duplicate of the in-flight request.
    await handle(payload());
    expect(status).toHaveBeenCalledWith(expect.stringContaining("duplicate"));
    // Event for a different open project.
    await handle({ ...payload(), projectRoot: "/other", quote: { ...payload().quote, ledgerRevision: "revision-x" } });
    expect(status).toHaveBeenCalledWith(expect.stringContaining("different project"));
    expect(ask).toHaveBeenCalledTimes(1);
    releaseFirst(true);
    await first;
    // Only the first operation dispatched.
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("discards a queued authorization when the project switches before it is presented", async () => {
    let releaseFirst!: (v: boolean) => void;
    const ask = vi.fn(async () => true).mockImplementationOnce(
      () => new Promise((res) => { releaseFirst = res; }));
    const invoke = vi.fn(async (command: string): Promise<string> =>
      command === "confirm_spend" ? "one-use-token" : "saved");
    const status = vi.fn();
    let root: string | null = "/project";
    const handle = createSpendConfirmationHandler({ invoke, ask, status, projectRoot: () => root });

    const first = handle(payload());
    await vi.waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    const queued = handle({ ...payload(), quote: { ...payload().quote, ledgerRevision: "revision-q" } });
    await vi.waitFor(() => expect(status).toHaveBeenCalledWith(expect.stringContaining("Queued")));
    root = "/switched";
    releaseFirst(true);
    await Promise.all([first, queued]);
    // Both the in-flight and the queued request revalidate against the
    // current project and are discarded; nothing dispatches.
    expect(status).toHaveBeenCalledWith(expect.stringContaining("belongs to a different project"));
    expect(ask).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledTimes(0);
  });

  it("explains a stale-quote rejection with an accurate re-run path", async () => {
    const ask = vi.fn().mockResolvedValue(true);
    const invoke = vi.fn().mockRejectedValue(new Error("spend quote is stale; request a new quote"));
    const status = vi.fn();
    const handle = createSpendConfirmationHandler({
      invoke, ask, status, projectRoot: () => "/project",
    });
    await handle(payload());
    expect(status).toHaveBeenCalledWith(expect.stringContaining("authorization expired"));
    expect(status).toHaveBeenCalledWith(expect.stringContaining("Generate still"));
    // The token-issuing call failed before dispatch; no replay was attempted.
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("confirm_spend", expect.anything());
  });

  it("rejects overflow with an accurate re-trigger instruction (Jobs has no resume)", async () => {
    let askCalls = 0;
    let release!: (v: boolean) => void;
    const ask = vi.fn().mockImplementation(() => {
      askCalls += 1;
      if (askCalls === 1) return new Promise<boolean>((r) => { release = r; });
      return Promise.resolve(true);
    });
    const invoke = vi.fn().mockResolvedValue("ok");
    const status = vi.fn();
    const handle = createSpendConfirmationHandler({
      invoke, ask, status, projectRoot: () => "/project",
    });
    const first = handle(payload());
    await vi.waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    for (let i = 0; i < 5; i += 1) {
      await handle({
        ...payload(),
        operation: `op_${i}`,
        quote: { ...payload().quote, ledgerRevision: `rev-${i}` },
      });
    }
    await vi.waitFor(() =>
      expect(status).toHaveBeenCalledWith(expect.stringContaining("queue is full")));
    expect(status).toHaveBeenCalledWith(expect.stringContaining("Re-trigger the operation from its Studio surface"));
    expect(status).not.toHaveBeenCalledWith(expect.stringContaining("Resume it from Jobs"));
    release(true);
    await first;
  });

  it("queues two distinct operations quoted at the same ledger revision (no dedup collision)", async () => {
    let askCalls = 0;
    let release!: (v: boolean) => void;
    const ask = vi.fn().mockImplementation(() => {
      askCalls += 1;
      if (askCalls === 1) return new Promise<boolean>((r) => { release = r; });
      return Promise.resolve(true);
    });
    const invoke = vi.fn().mockResolvedValue("ok");
    const status = vi.fn();
    const handle = createSpendConfirmationHandler({
      invoke, ask, status, projectRoot: () => "/project",
    });
    const first = handle(payload());
    await vi.waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    // Same project/operation/model and the same ledger revision, but a
    // different output path — a distinct operation, not a duplicate.
    const second = handle({
      ...payload(),
      args: { model: "image-model", params: { prompt: "other scene" }, save_path: "Preproduction/Scene2/references/interior_1.png" },
    });
    await vi.waitFor(() =>
      expect(status).toHaveBeenCalledWith(expect.stringContaining("Queued")));
    expect(status).not.toHaveBeenCalledWith(expect.stringContaining("duplicate"));
    release(true);
    await Promise.all([first, second]);
    // Both were presented and dispatched (confirm + replay each).
    expect(ask).toHaveBeenCalledTimes(2);
    expect(invoke).toHaveBeenCalledTimes(4);
  });

  it("drops an exact re-emission of the same event as a duplicate", async () => {
    let release!: (v: boolean) => void;
    const ask = vi.fn().mockImplementation(() => new Promise<boolean>((r) => { release = r; }));
    const invoke = vi.fn().mockResolvedValue("ok");
    const status = vi.fn();
    const handle = createSpendConfirmationHandler({
      invoke, ask, status, projectRoot: () => "/project",
    });
    const first = handle(payload());
    await vi.waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    await handle(payload());
    expect(status).toHaveBeenCalledWith(expect.stringContaining("duplicate"));
    expect(ask).toHaveBeenCalledTimes(1);
    release(true);
    await first;
  });
});
