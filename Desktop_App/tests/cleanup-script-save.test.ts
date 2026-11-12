import { createRequire } from "node:module";
import { afterEach, describe, expect, it, vi } from "vitest";
import { saveProjectScript } from "../src/studio/scriptFiles";

const { HOST_COMMANDS } = createRequire(import.meta.url)("../electron/commandAllowlist.cjs");

describe("SC03 saveProjectScript wire contract", () => {
  afterEach(() => vi.unstubAllGlobals());
  it("admits only the exact saver command, not arbitrary script commands", () => {
    expect(HOST_COMMANDS.has("save_project_script")).toBe(true);
    expect(HOST_COMMANDS.has("save_project_script_unchecked")).toBe(false);
  });
  it("sends exact typed arguments and returns the relative host path without other operations", async () => {
    const text = "Scene 1\r\nHello — exact.\r\n";
    const invoke = vi.fn(async () => "Scripts/friends.md");
    const forbidden = vi.fn(() => { throw new Error("Unexpected provider operation"); });
    vi.stubGlobal("window", { workbench: { invoke } });
    vi.stubGlobal("fetch", forbidden);
    expect(await saveProjectScript("/project", "friends.md", text)).toBe("Scripts/friends.md");
    expect(invoke).toHaveBeenCalledExactlyOnceWith("save_project_script", {
      projectRoot: "/project", filename: "friends.md", text,
    });
    expect(forbidden).not.toHaveBeenCalled();
  });
  it("returns the actionable host saveError without retrying or claiming success", async () => {
    const invoke = vi.fn(async () => { throw new Error("Already exists; choose another filename."); });
    vi.stubGlobal("window", { workbench: { invoke } });
    expect(await saveProjectScript("/project", "friends.md", "Scene 1")).toEqual({
      saveError: "Already exists; choose another filename.",
    });
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});
