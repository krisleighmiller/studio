export type OrderedReferenceChoice = { id: string; label: string };

/** A request-local list. Nothing is selected until the user adds it. */
export function orderedReferencePicker(choices: readonly OrderedReferenceChoice[]): {
  element: HTMLElement; selected: () => string[];
} {
  const selected: string[] = [];
  const element = document.createElement("section");
  const label = document.createElement("label");
  label.textContent = "Optional reference images (added in order)";
  const select = document.createElement("select");
  select.setAttribute("aria-label", "Reference image");
  const placeholder = document.createElement("option");
  placeholder.value = ""; placeholder.textContent = "Choose an image"; select.append(placeholder);
  for (const choice of choices) {
    const option = document.createElement("option"); option.value = choice.id; option.textContent = choice.label; select.append(option);
  }
  label.append(select);
  const list = document.createElement("ol");
  const redraw = () => {
    list.replaceChildren();
    selected.forEach((id, index) => {
      const item = document.createElement("li");
      item.textContent = choices.find(choice => choice.id === id)?.label ?? id;
      const remove = document.createElement("button");
      remove.type = "button"; remove.textContent = "Remove";
      remove.setAttribute("aria-label", `Remove reference ${index + 1}`);
      remove.addEventListener("click", () => { selected.splice(index, 1); redraw(); });
      item.append(remove); list.append(item);
    });
  };
  const add = document.createElement("button");
  add.type = "button"; add.textContent = "Add reference"; add.dataset.addReference = "";
  add.addEventListener("click", () => {
    if (select.value && !selected.includes(select.value)) { selected.push(select.value); redraw(); }
  });
  element.append(label, add, list);
  return { element, selected: () => [...selected] };
}
