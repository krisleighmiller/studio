import { mkdtempSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettingsPanel: vi.fn(), renderSettingsPanel: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => { desk.clearWorkflowDesk(); vi.useRealTimers(); vi.restoreAllMocks(); document.body.replaceChildren(); });

it("explorer_old_project_change_ignored", async () => {
  vi.useFakeTimers();
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const base = mkdtempSync(join(process.env.TMPDIR!, "w6-08-"));
  const a = join(base, "A"), b = join(base, "B");
  mkdirSync(a); mkdirSync(b);
  const oldPath = join(a, "A-late.txt"), currentPath = join(b, "B-current.txt");
  writeFileSync(currentPath, "current");
  const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  let chosen = a, holdOldEvent = false, deliverOldEvent: (() => void) | undefined;
  const invoke = vi.fn(async (command: string, args: { path: string }) => {
    if (command === "list_dir") {
      const entries = readdirSync(args.path, { withFileTypes: true }).map(entry => ({
        name: entry.name, path: join(args.path, entry.name), isDir: entry.isDirectory(),
      }));
      if (holdOldEvent && args.path === a) {
        holdOldEvent = false;
        await new Promise<void>(resolve => { deliverOldEvent = resolve; });
      }
      return entries;
    }
    if (command === "workflow_import") return structuredClone(fixture);
    throw new Error(`Unexpected host call: ${command}`);
  });
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, openProject: async () => chosen, ask: async () => true,
    rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {},
    onSpendConfirmationRequired: () => () => {} } });
  const flush = () => vi.advanceTimersByTimeAsync(150);
  try {
    await import("../src/main");
    const open = async (root: string) => {
      chosen = root;
      document.querySelector<HTMLButtonElement>("#home-open-project")!.click();
      await flush();
    };
    const tree = document.querySelector<HTMLElement>("#file-tree")!;
    await flush(); await open(a);
    writeFileSync(oldPath, "outside Studio");
    holdOldEvent = true;
    await vi.advanceTimersByTimeAsync(1200);
    expect(deliverOldEvent).toBeDefined();
    await open(b);
    expect(tree.querySelector(`[data-path="${currentPath}"]`)).not.toBeNull();
    const currentTree = tree.innerHTML;
    deliverOldEvent!();
    await flush();
    expect(tree.innerHTML).toBe(currentTree);
    expect(tree.querySelector(`[data-path="${oldPath}"]`)).toBeNull();
    expect(tree.querySelector(`[data-path="${currentPath}"]`)).not.toBeNull();
  } finally {
    deliverOldEvent?.();
    rmSync(base, { recursive: true, force: true });
  }
});
