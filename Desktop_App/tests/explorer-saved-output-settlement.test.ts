import { mkdtempSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettings: vi.fn(), renderSettingsPanel: vi.fn(), setSelectedChatModelFromSelect: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => { desk.clearWorkflowDesk(); vi.restoreAllMocks(); document.body.replaceChildren(); });

it("explorer_saved_output_settlement", async () => {
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = mkdtempSync(join(process.env.TMPDIR!, "w6-05-"));
  const parent = join(root, "Production");
  mkdirSync(parent);
  const output = join(parent, "completed-output.png");
  const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  let onSpend: ((payload: unknown) => void) | undefined;
  let finishJob: (() => void) | undefined;
  const invoke = vi.fn(async (command: string, args: any) => {
    if (command === "list_dir") return readdirSync(args.path, { withFileTypes: true }).map(entry => ({
      name: entry.name, path: join(args.path, entry.name), isDir: entry.isDirectory(),
    }));
    if (command === "workflow_import") return structuredClone(fixture);
    if (command === "workflow_status") return { state: { jobs: [] } };
    if (command === "confirm_unbounded_spend") return "fixture-token";
    if (command === "media_generate_confirmed") {
      expect(args.projectRoot).toBe(root);
      await new Promise<void>(resolve => { finishJob = resolve; });
      writeFileSync(output, "local saved output bytes");
      return { continuation: { reply: "Saved local output" } };
    }
    throw new Error(`Unexpected host call: ${command}`);
  });
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, openProject: async () => root, ask: async () => true,
    rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {},
    onSpendConfirmationRequired: (handler: (payload: unknown) => void) => { onSpend = handler; return () => {}; } } });
  await import("../src/main");
  document.querySelector<HTMLButtonElement>("#home-open-project")!.click();
  const tree = document.querySelector<HTMLElement>("#file-tree")!;
  await vi.waitFor(() => expect(tree.querySelector(`[data-path="${parent}"]`)).not.toBeNull());
  tree.querySelector<HTMLButtonElement>(`[data-path="${parent}"]`)!.click();
  await vi.waitFor(() => expect(invoke.mock.calls.some(([cmd, args]) => cmd === "list_dir" && args.path === parent)).toBe(true));
  const payload = { projectRoot: root, provider: "fixture-local", category: "media", operation: "atlas_generate_image",
    model: "fixture-image", quote: { currency: "USD", requestHash: "a".repeat(64), ledgerRevision: "fixture", disclosure: "Fixture only" },
    args: { model: "fixture-image", params: { prompt: "No provider call" } } };
  expect(onSpend).toBeDefined();
  onSpend!(payload);
  await vi.waitFor(() => expect(finishJob).toBeDefined());
  expect(tree.querySelector(`[data-path="${output}"]`)).toBeNull();
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "list_dir").length).toBeGreaterThan(0);
  finishJob!();
  await vi.waitFor(() => expect(tree.querySelectorAll(`[data-path="${output}"]`)).toHaveLength(1));
  expect(tree.querySelector(`[data-path="${output}"]`)?.textContent).toContain("completed-output.png");
  expect(readFileSync(output, "utf8")).toBe("local saved output bytes");
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "media_generate_confirmed")).toHaveLength(1);
});
