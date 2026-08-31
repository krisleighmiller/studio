/**
 * Single seam between the desk UI and the Electron desktop host.
 * `window.workbench` is injected by `electron/preload.cjs`.
 */

export type OpenDialogOptions = {
  title?: string;
  defaultPath?: string;
  directory?: boolean;
  multiple?: boolean;
  filters?: { name: string; extensions: string[] }[];
};

export type SaveDialogOptions = {
  title?: string;
  defaultPath?: string;
  filters?: { name: string; extensions: string[] }[];
};

export type AskDialogOptions = {
  title?: string;
  kind?: "info" | "warning" | "error";
};

function bridge(): NonNullable<Window["workbench"]> {
  const wb = typeof window !== "undefined" ? window.workbench : undefined;
  if (!wb) {
    throw new Error(
      "Desktop host bridge missing. Launch with `npm run dev:app` (Electron).",
    );
  }
  return wb;
}

function cleanInvokeError(error: unknown): Error {
  const raw =
    error instanceof Error
      ? error.message
      : typeof error === "string"
        ? error
        : String(error);
  // Electron wraps sidecar failures as:
  // Error invoking remote method 'host:invoke': Error: <actual>
  const cleaned = raw
    .replace(/^Error invoking remote method '[^']+':\s*/i, "")
    .replace(/^Error:\s*/i, "");
  return new Error(cleaned || raw);
}

export async function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await bridge().invoke<T>(cmd, args);
  } catch (error) {
    throw cleanInvokeError(error);
  }
}

export function onMenu(handler: (id: string) => void): () => void {
  return bridge().onMenu(handler);
}

export function onDirectorProgress(
  handler: (message: string) => void,
): () => void {
  return bridge().onDirectorProgress(handler);
}

export async function bindHostEvents(handlers: {
  onMenu: (id: string) => void;
  onDirectorProgress: (message: string) => void;
}): Promise<() => void> {
  const wb = bridge();
  const offMenu = wb.onMenu(handlers.onMenu);
  const offProgress = wb.onDirectorProgress(handlers.onDirectorProgress);
  return () => {
    offMenu();
    offProgress();
  };
}

export function toMediaUrl(path: string): string {
  return bridge().toMediaUrl(path);
}

export async function toMediaUrlAsync(path: string): Promise<string> {
  return bridge().toMediaUrl(path);
}

export async function openDialog(
  opts: OpenDialogOptions,
): Promise<string | string[] | null> {
  return bridge().openDialog(opts);
}

export async function saveDialog(
  opts: SaveDialogOptions,
): Promise<string | null> {
  return bridge().saveDialog(opts);
}

export async function ask(
  message: string,
  opts?: AskDialogOptions,
): Promise<boolean> {
  return bridge().ask(message, opts);
}

export async function rememberRecentProject(projectPath: string): Promise<void> {
  const remember = bridge().rememberRecentProject;
  if (!remember) return;
  await remember(projectPath);
}

export async function removeRecentProject(projectPath: string): Promise<void> {
  const remove = bridge().removeRecentProject;
  if (!remove) return;
  await remove(projectPath);
}

/** Decode `open-recent:<base64url>` menu ids from Electron. */
export function decodeOpenRecentMenuAction(action: string): string | null {
  const prefix = "open-recent:";
  if (!action.startsWith(prefix)) return null;
  const encoded = action.slice(prefix.length);
  try {
    const padded =
      encoded + "=".repeat((4 - (encoded.length % 4 || 4)) % 4);
    const binary = atob(padded.replace(/-/g, "+").replace(/_/g, "/"));
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    return new TextDecoder().decode(bytes);
  } catch {
    return null;
  }
}
