import { describe, expect, it } from "vitest";
import { orderedReferencePicker } from "../src/studio/orderedReferencePicker";

describe("ordered reference picker", () => {
  it("starts empty and adds, removes, and re-adds images in explicit selection order", () => {
    const picker = orderedReferencePicker([
      { id: "a", label: "Face" }, { id: "b", label: "Body" }, { id: "c", label: "Clothes" },
    ]);
    const select = picker.element.querySelector("select")!;
    const add = picker.element.querySelector<HTMLButtonElement>('[data-add-reference]')!;
    expect(picker.selected()).toEqual([]);
    select.value = "c"; add.click(); select.value = "a"; add.click(); add.click();
    expect(picker.selected()).toEqual(["c", "a"]);
    expect(picker.element.querySelector("ol")!.textContent).toContain("Clothes");
    picker.element.querySelector<HTMLButtonElement>('button[aria-label="Remove reference 1"]')!.click();
    select.value = "c"; add.click();
    expect(picker.selected()).toEqual(["a", "c"]);
    const returned = picker.selected(); returned.pop();
    expect(picker.selected()).toEqual(["a", "c"]);
  });
});
