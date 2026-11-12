import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettingsPanel: vi.fn(), renderSettingsPanel: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => { desk.clearWorkflowDesk(); vi.useRealTimers(); vi.restoreAllMocks(); document.body.replaceChildren(); });

it("explorer_reopened_root_old_event_ignored", async () => {
  vi.useFakeTimers();
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = mkdtempSync(join(process.env.TMPDIR!, "w6-09-"));
  const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  let menu: (action: string) => void = () => {};
  const scheduled: Array<() => void> = [];
  const realInterval = globalThis.setInterval;
  vi.spyOn(globalThis, "setInterval").mockImplementation(((callback: TimerHandler, delay?: number, ...args: unknown[]) => {
    if (delay === 1200 && typeof callback === "function") scheduled.push(callback as () => void);
    return realInterval(callback, delay, ...args);
  }) as typeof setInterval);
  const invoke = vi.fn(async (command: string, args: { path: string }) => {
    if (command === "list_dir") return readdirSync(args.path, { withFileTypes: true }).map(entry => ({ name: entry.name, path: join(args.path, entry.name), isDir: entry.isDirectory() }));
    if (command === "workflow_import") return structuredClone(fixture);
    throw Error(`Unexpected host call: ${command}`);
  });
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, openProject: async () => root, ask: async () => true,
    rememberRecentProject: async () => {}, onMenu: (handler: typeof menu) => { menu = handler; return () => {}; },
    onDirectorProgress: () => () => {}, onSpendConfirmationRequired: () => () => {} } });
  const flush = () => vi.advanceTimersByTimeAsync(150);
  try {
    await import("../src/main");
    await flush();
    document.querySelector<HTMLButtonElement>("#home-open-project")!.click(); await flush();
    const oldEvent = scheduled.at(-1);
    expect(oldEvent).toBeDefined();
    writeFileSync(join(root, "old-external.txt"), "outside Studio");
    menu("close-project"); await flush();
    document.querySelector<HTMLButtonElement>("#home-open-project")!.click(); await flush();
    const tree = document.querySelector<HTMLElement>("#file-tree")!;
    expect(tree.querySelector(`[data-path="${join(root, "old-external.txt")}"]`)).not.toBeNull();
    const reopenedTree = tree.innerHTML;
    const reads = invoke.mock.calls.filter(([command]) => command === "list_dir").length;
    oldEvent!(); await flush();
    expect(invoke.mock.calls.filter(([command]) => command === "list_dir")).toHaveLength(reads);
    expect(tree.innerHTML).toBe(reopenedTree);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
