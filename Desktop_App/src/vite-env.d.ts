/// <reference types="vite/client" />

declare module "*.md?raw" {
  const content: string;
  export default content;
}

interface WorkbenchBridge {
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  onMenu: (handler: (id: string) => void) => () => void;
  onDirectorProgress: (handler: (message: string) => void) => () => void;
  toMediaUrl: (path: string) => string;
  openDialog: (opts: {
    title?: string;
    defaultPath?: string;
    directory?: boolean;
    multiple?: boolean;
    filters?: { name: string; extensions: string[] }[];
  }) => Promise<string | string[] | null>;
  saveDialog: (opts: {
    title?: string;
    defaultPath?: string;
    filters?: { name: string; extensions: string[] }[];
  }) => Promise<string | null>;
  ask: (
    message: string,
    opts?: { title?: string; kind?: "info" | "warning" | "error" },
  ) => Promise<boolean>;
  listRecentProjects?: () => Promise<string[]>;
  rememberRecentProject?: (projectPath: string) => Promise<string[]>;
  removeRecentProject?: (projectPath: string) => Promise<string[]>;
  platform: string;
}

interface Window {
  workbench?: WorkbenchBridge;
}
