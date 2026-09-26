import type { SurfaceAction } from "./surfaces/surfaceModels";

/** Apply a view-model action to the matching control. Disabled actions cannot be clicked. */
export function applySurfaceActions(view: ParentNode, actions: SurfaceAction[]): void {
  const buttons = view.querySelectorAll<HTMLButtonElement>("button");
  for (const item of actions) {
    for (const button of buttons) {
      if (button.textContent !== item.label) continue;
      if (!item.enabled) {
        button.disabled = true;
        if (item.reason) button.title = item.reason;
      }
    }
  }
}
