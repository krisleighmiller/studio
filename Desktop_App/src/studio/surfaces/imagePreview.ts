import { getSurfaceHost } from "./session";

/**
 * Full-size image preview. The library and Generate surfaces show cards with
 * cropped thumbnails; clicking the image opens the whole picture in a modal
 * overlay. Cards stay quiet (no buttons); the preview is one click in, one
 * click out, Esc to dismiss.
 */

let openClose: (() => void) | null = null;

/** Whether a preview is currently open (used to avoid stacking previews). */
export function imagePreviewOpen(): boolean {
  return openClose !== null;
}

/** Open a preview for a library-relative asset path. */
export function openImagePreviewForAsset(relativePath: string): void {
  if (openClose) return;
  const host = getSurfaceHost();
  void host
    .mediaUrl(host.resolvePath(relativePath) ?? relativePath)
    .then((url) => openImagePreview(url))
    .catch(() => undefined);
}

/** Open the preview modal for a resolved media URL. */
export function openImagePreview(url: string): void {
  if (openClose) return;
  const dialog = document.createElement("dialog");
  dialog.className = "studio-image-preview";
  dialog.dataset.imagePreview = "";
  const image = document.createElement("img");
  image.src = url;
  image.alt = "Preview";
  const close = document.createElement("button");
  close.type = "button";
  close.className = "studio-image-preview-close";
  close.textContent = "Close";
  close.setAttribute("aria-label", "Close image preview");
  dialog.append(close, image);
  document.body.append(dialog);
  dialog.showModal();
  const finish = () => {
    openClose = null;
    dialog.close();
    dialog.remove();
  };
  close.addEventListener("click", (event) => {
    event.stopPropagation();
    finish();
  });
  dialog.addEventListener("cancel", (event) => {
    event.preventDefault();
    finish();
  });
  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) finish();
  });
  openClose = finish;
}
