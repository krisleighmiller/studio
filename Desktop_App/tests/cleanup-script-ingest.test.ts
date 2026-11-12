import { createRequire } from "node:module";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ingestScriptFile } from "../src/studio/scriptFiles";

const { HOST_COMMANDS } = createRequire(import.meta.url)("../electron/commandAllowlist.cjs");

describe("SC04 ingestScriptFile wire contract", () => {
  afterEach(() => vi.unstubAllGlobals());
  it("admits only the exact ingestion command", () => {
    expect(HOST_COMMANDS.has("ingest_script_file")).toBe(true);
    expect(HOST_COMMANDS.has("ingest_script_file_unchecked")).toBe(false);
  });
  it.each(["external-import", "existing-project-file"] as const)("sends exact %s arguments without model, picker or rescan calls", async (sourceKind) => {
    const invoke = vi.fn(async (command: string) => {
      if (command !== "ingest_script_file") throw new Error(`Forbidden command: ${command}`);
      return "Scripts/friends.md";
    });
    const forbidden = vi.fn(() => { throw new Error("Unexpected network/picker operation"); });
    vi.stubGlobal("window", { workbench: { invoke, showOpenDialog: forbidden } });
    vi.stubGlobal("fetch", forbidden);
    expect(await ingestScriptFile("/project", "/source/friends.md", sourceKind)).toBe("Scripts/friends.md");
    expect(invoke).toHaveBeenCalledExactlyOnceWith("ingest_script_file", {
      projectRoot: "/project", sourcePath: "/source/friends.md", sourceKind,
    });
    expect(forbidden).not.toHaveBeenCalled();
  });
  it("returns actionable ingestError without retry or false success", async () => {
    const invoke = vi.fn(async () => { throw new Error("Existing files are never replaced"); });
    vi.stubGlobal("window", { workbench: { invoke } });
    expect(await ingestScriptFile("/project", "friends.md", "existing-project-file")).toEqual({
      ingestError: "Existing files are never replaced",
    });
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});
