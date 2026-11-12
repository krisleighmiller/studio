// Reuse Chromium's editing roles and the context event's exact selection.
// Renderer-owned object menus preventDefault, keeping them out of this path.
export function installEditableContextMenu(win, Menu, clipboard) {
  win.webContents.on('context-menu', (_event, params) => {
    if (!params.isEditable) {
      if (!params.selectionText) return;
      const selected = params.selectionText;
      Menu.buildFromTemplate([
        { label: 'Copy Selection', click: () => clipboard.writeText(selected) },
      ]).popup({ window: win });
      return;
    }
    const flags = params.editFlags;
    Menu.buildFromTemplate([
      { role: 'cut', enabled: flags.canCut },
      { role: 'copy', enabled: flags.canCopy },
      { role: 'paste', enabled: flags.canPaste },
      { type: 'separator' },
      { role: 'selectAll', enabled: flags.canSelectAll },
    ]).popup({ window: win });
  });
}
