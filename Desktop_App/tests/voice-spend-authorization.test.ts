import { describe, expect, it, vi } from "vitest";
import { authorizeVoiceSpendWith } from "../src/voicePanel";

const quote = (policy: "approval_required" | "warn_only") => ({
  policy,
  requestHash: "a".repeat(64),
  ledgerRevision: "revision-a",
  disclosure: "No verified maximum exists for this request.",
});

function setup(policy: "approval_required" | "warn_only", accepted = true) {
  const invoke = vi.fn(async (command: string) => {
    if (command === "elevenlabs_quote_voice_spend") return quote(policy);
    if (command === "elevenlabs_confirm_unbounded_voice_spend") return "one-use-token";
    throw new Error(`unexpected command: ${command}`);
  });
  const confirm = vi.fn(() => accepted);
  return {
    invoke,
    confirm,
    run: () => authorizeVoiceSpendWith(
      { invoke, confirm, projectRoot: () => "/project" },
      "elevenlabs_tts_preview",
      "eleven_multilingual_v2",
      JSON.stringify({ text: "hello", voice_id: "voice-a" }),
    ),
  };
}

describe("direct voice spend authorization (host mocked; no provider calls)", () => {
  it.each(["approval_required", "warn_only"] as const)(
    "%s requires a request-bound one-use confirmation before the paid command can run",
    async (policy) => {
      const { run, invoke, confirm } = setup(policy);
      await expect(run()).resolves.toEqual({ projectRoot: "/project", confirmation: "one-use-token" });
      expect(confirm).toHaveBeenCalledWith(expect.stringContaining("elevenlabs_tts_preview"));
      expect(invoke).toHaveBeenNthCalledWith(1, "elevenlabs_quote_voice_spend", {
        projectRoot: "/project",
        operation: "elevenlabs_tts_preview",
        model: "eleven_multilingual_v2",
        input: JSON.stringify({ text: "hello", voice_id: "voice-a" }),
      });
      expect(invoke).toHaveBeenNthCalledWith(2, "elevenlabs_confirm_unbounded_voice_spend", {
        projectRoot: "/project",
        operation: "elevenlabs_tts_preview",
        model: "eleven_multilingual_v2",
        input: JSON.stringify({ text: "hello", voice_id: "voice-a" }),
        requestHash: "a".repeat(64),
        ledgerRevision: "revision-a",
      });
    },
  );

  it("no-budget approval keeps host disclosure without inventing a budget", async () => {
    const disclosure = "No project budget is configured. Actual provider cost is unknown.";
    const invoke = vi.fn(async (command: string) => command === "elevenlabs_quote_voice_spend"
      ? { ...quote("approval_required"), disclosure } : "one-use-token");
    const confirm = vi.fn((_message: string) => true);
    await authorizeVoiceSpendWith(
      { invoke, confirm, projectRoot: () => "/project" },
      "elevenlabs_tts_preview", "eleven_multilingual_v2",
      JSON.stringify({ text: "hello", voice_id: "voice-a" }),
    );
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining("elevenlabs_tts_preview"));
    expect(confirm.mock.calls[0][0]).not.toContain("more than the project budget");
  });

  it("decline issues no confirmation token and therefore no paid provider command", async () => {
    const { run, invoke } = setup("approval_required", false);
    await expect(run()).rejects.toThrow("no provider request was sent");
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("strict-policy quote failure prompts nobody and issues no confirmation token", async () => {
    const invoke = vi.fn(async () => { throw new Error("strict_hard_stop blocks unknown-cost voice operation"); });
    const confirm = vi.fn(() => true);
    await expect(authorizeVoiceSpendWith(
      { invoke, confirm, projectRoot: () => "/project" },
      "elevenlabs_tts_preview",
      "eleven_multilingual_v2",
      "input",
    )).rejects.toThrow("strict_hard_stop");
    expect(confirm).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});
