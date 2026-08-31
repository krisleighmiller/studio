const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("workbench", {
  invoke: (cmd, args) => ipcRenderer.invoke("host:invoke", cmd, args ?? {}),
  onMenu: (handler) => {
    const listener = (_event, id) => handler(id);
    ipcRenderer.on("menu", listener);
    return () => ipcRenderer.removeListener("menu", listener);
  },
  onDirectorProgress: (handler) => {
    const listener = (_event, message) => handler(message);
    ipcRenderer.on("director-progress", listener);
    return () => ipcRenderer.removeListener("director-progress", listener);
  },
  toMediaUrl: (filePath) =>
    ipcRenderer.sendSync("host:toMediaUrlSync", filePath),
  openDialog: (opts) => ipcRenderer.invoke("dialog:open", opts ?? {}),
  saveDialog: (opts) => ipcRenderer.invoke("dialog:save", opts ?? {}),
  ask: (message, opts) => ipcRenderer.invoke("dialog:ask", message, opts ?? {}),
  listRecentProjects: () => ipcRenderer.invoke("recent:list"),
  rememberRecentProject: (projectPath) =>
    ipcRenderer.invoke("recent:remember", projectPath),
  removeRecentProject: (projectPath) =>
    ipcRenderer.invoke("recent:remove", projectPath),
  platform: process.platform,
});
