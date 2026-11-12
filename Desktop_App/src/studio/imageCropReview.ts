export type CropRectangle = { x: number; y: number; width: number; height: number };

/** Preview a local rectangle before asking the host to make an immutable draft. */
export async function reviewImageCrop(url: string): Promise<CropRectangle | null> {
  const image = new Image();
  image.src = url;
  await image.decode();
  const { naturalWidth: width, naturalHeight: height } = image;
  if (!width || !height || width > 8192 || height > 8192) throw new Error("Still dimensions must be between 1 and 8192 pixels.");
  const dialog = document.createElement("dialog");
  dialog.className = "image-crop-review";
  const heading = document.createElement("h2"); heading.textContent = "Crop this still locally";
  const detail = document.createElement("p");
  detail.textContent = "Source stays unchanged. Crop becomes a new draft; select and approve it separately. No generation or provider charge.";
  const previews = document.createElement("div"); previews.className = "image-crop-previews";
  const before = document.createElement("figure");
  const original = document.createElement("img"); original.src = url; original.alt = "Original still";
  const beforeLabel = document.createElement("figcaption"); beforeLabel.textContent = `Before (${width} × ${height})`;
  before.append(original, beforeLabel);
  const after = document.createElement("figure");
  const canvas = document.createElement("canvas"); canvas.setAttribute("aria-label", "Crop preview");
  const afterLabel = document.createElement("figcaption");
  after.append(canvas, afterLabel); previews.append(before, after);
  const fields = document.createElement("div"); fields.className = "image-crop-fields";
  const inputs: HTMLInputElement[] = [];
  for (const [label, value] of [["Left %", 10], ["Top %", 10], ["Right %", 10], ["Bottom %", 10]] as const) {
    const wrapper = document.createElement("label"); wrapper.textContent = label;
    const input = document.createElement("input"); input.type = "number"; input.min = "0"; input.max = "90"; input.step = "1"; input.value = String(value);
    inputs.push(input); wrapper.append(input); fields.append(wrapper);
  }
  const error = document.createElement("p"); error.setAttribute("role", "status");
  const controls = document.createElement("div");
  const cancel = document.createElement("button"); cancel.type = "button"; cancel.textContent = "Cancel";
  const save = document.createElement("button"); save.type = "button"; save.textContent = "Save crop as draft";
  controls.append(cancel, save); dialog.append(heading, detail, fields, previews, error, controls);
  const rect = (): CropRectangle | null => {
    const values = inputs.map((input) => Number(input.value));
    if (values.some((n) => !Number.isInteger(n) || n < 0 || n > 90)) return null;
    const [left, top, right, bottom] = values;
    const x = Math.round(width * left / 100), y = Math.round(height * top / 100);
    const cropWidth = width - x - Math.round(width * right / 100);
    const cropHeight = height - y - Math.round(height * bottom / 100);
    if (cropWidth < 1 || cropHeight < 1 || (cropWidth === width && cropHeight === height)) return null;
    return { x, y, width: cropWidth, height: cropHeight };
  };
  const draw = () => {
    const crop = rect(); save.disabled = !crop;
    error.textContent = crop ? "" : "Choose a smaller rectangle within the image (whole-image crops are not saved).";
    if (!crop) return;
    canvas.width = crop.width; canvas.height = crop.height;
    canvas.getContext("2d")?.drawImage(image, crop.x, crop.y, crop.width, crop.height, 0, 0, crop.width, crop.height);
    afterLabel.textContent = `After (${crop.width} × ${crop.height}): x=${crop.x}, y=${crop.y}`;
  };
  inputs.forEach((input) => input.addEventListener("input", draw)); draw();
  document.body.append(dialog);
  try {
    dialog.showModal();
    return await new Promise<CropRectangle | null>((resolve) => {
      let done = false;
      const settle = (result: CropRectangle | null) => { if (done) return; done = true; dialog.close(); resolve(result); };
      cancel.addEventListener("click", () => settle(null));
      save.addEventListener("click", () => { const crop = rect(); if (crop) settle(crop); });
      dialog.addEventListener("close", () => settle(null));
    });
  } finally { dialog.remove(); }
}
