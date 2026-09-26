import { REQUIREMENT_KINDS, REQUIREMENT_STATUSES } from "../../workflowTypes";
import { getSurfaceHost, isProfileOpen, setProfileOpen } from "./session";
import { presentSurface } from "./shared";
import { buildProfileModel } from "./surfaceModels";
import type { ProfileIntent } from "./surfaceIntents";

function paintProjectProfile(emit: (intent: ProfileIntent) => void): HTMLElement {
  const wrap = document.createElement("section");
  wrap.className = "workflow-project";
  wrap.setAttribute("aria-label", "Production profile");
  const profile = getSurfaceHost().state?.profile;
  const deliverable = profile?.deliverables?.[0];
  const bar = document.createElement("div");
  bar.className = "workflow-profile-bar";
  const recipe = document.createElement("span");
  recipe.className = "workflow-profile-recipe";
  recipe.textContent = getSurfaceHost().activeRecipe()?.label ?? "Narrative dialogue";
  const summary = document.createElement("p");
  summary.className = "workflow-profile-summary";
  const bits = [
    deliverable?.name || getSurfaceHost().state?.project_name || "Untitled",
    deliverable?.aspect_ratio || profile?.aspect_ratios?.[0],
    deliverable?.runtime || profile?.runtime,
    profile?.medium,
  ].filter((bit): bit is string => Boolean(bit));
  summary.textContent = bits.join(" · ");
  const edit = document.createElement("button");
  edit.type = "button";
  edit.className = "ghost-button";
  edit.textContent = isProfileOpen() ? "Close profile" : "Edit profile";
  edit.disabled = Boolean(getSurfaceHost().busy);
  edit.addEventListener("click", () => {
    setProfileOpen(!isProfileOpen());
    getSurfaceHost().rerender();
  });
  bar.append(recipe, summary, edit);
  wrap.append(bar);
  if (isProfileOpen()) wrap.append(renderProfileForm(emit));
  wrap.append(renderRequirements(emit));
  return wrap;
}

function renderProfileForm(emit: (intent: ProfileIntent) => void): HTMLElement {
  const host = getSurfaceHost();
  const activeRecipeId = host.activeRecipe()?.id;
  const profile = host.state?.profile;
  const deliverable = profile?.deliverables?.[0];
  const form = document.createElement("form");
  form.className = "workflow-profile-form";
  const recipeLabel = document.createElement("label");
  recipeLabel.textContent = "Recipe";
  const recipeInput = document.createElement("select");
  recipeInput.name = "recipe";
  const report = getSurfaceHost().report;
  const choices = report?.recipes?.length
    ? report.recipes
    : [
        { id: "narrative_dialogue", label: "Narrative dialogue" },
        { id: "montage", label: "Montage" },
      ];
  for (const choice of choices) {
    const option = document.createElement("option");
    option.value = choice.id;
    option.textContent = choice.label;
    option.selected = choice.id === (profile?.recipe ?? activeRecipeId ?? "narrative_dialogue");
    recipeInput.append(option);
  }
  recipeLabel.append(recipeInput);
  form.append(recipeLabel);
  const fields: { key: string; label: string; value: string; wide?: boolean }[] = [
    { key: "name", label: "Deliverable", value: deliverable?.name ?? "" },
    { key: "audience", label: "Audience", value: profile?.audience ?? deliverable?.audience ?? "" },
    {
      key: "platforms",
      label: "Platforms",
      value: (profile?.platforms ?? deliverable?.platforms ?? []).join(", "),
    },
    { key: "runtime", label: "Runtime", value: profile?.runtime ?? deliverable?.runtime ?? "" },
    {
      key: "aspect",
      label: "Aspect ratio",
      value: deliverable?.aspect_ratio ?? profile?.aspect_ratios?.[0] ?? "",
    },
    {
      key: "resolution",
      label: "Resolution",
      value: profile?.resolution ?? deliverable?.resolution ?? "",
    },
    {
      key: "frame_rate",
      label: "Frame rate",
      value: profile?.frame_rate ?? deliverable?.frame_rate ?? "",
    },
    { key: "language", label: "Language", value: profile?.language ?? deliverable?.language ?? "" },
    { key: "medium", label: "Medium", value: profile?.medium ?? "" },
    { key: "style_notes", label: "Style notes", value: profile?.style_notes ?? "", wide: true },
    { key: "audio_plan", label: "Audio plan", value: profile?.audio_plan ?? "", wide: true },
    {
      key: "continuity_rules",
      label: "Continuity rules",
      value: (profile?.continuity_rules ?? []).join("\n"),
      wide: true,
    },
    {
      key: "safety_constraints",
      label: "Safety constraints",
      value: (profile?.safety_constraints ?? []).join("\n"),
      wide: true,
    },
    { key: "budget", label: "Budget", value: profile?.budget ?? "" },
    { key: "schedule", label: "Schedule", value: profile?.schedule ?? "" },
    { key: "rights_notes", label: "Rights notes", value: profile?.rights_notes ?? "", wide: true },
  ];
  for (const field of fields) {
    const label = document.createElement("label");
    label.className = field.wide ? "wide" : "";
    label.textContent = field.label;
    const input = field.wide ? document.createElement("textarea") : document.createElement("input");
    input.name = field.key;
    input.value = field.value;
    if (field.key === "medium") {
      input.setAttribute("placeholder", "Clay, live action, anime…");
    }
    label.append(input);
    form.append(label);
  }
  const save = document.createElement("button");
  save.type = "submit";
  save.className = "ghost-button";
  save.textContent = "Save profile";
  save.disabled = Boolean(getSurfaceHost().busy);
  form.append(save);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const read = (key: string) =>
      (form.elements.namedItem(key) as HTMLInputElement | HTMLTextAreaElement | null)?.value ?? "";
    const blank = (value: string) => {
      const trimmed = value.trim();
      return trimmed ? trimmed : null;
    };
    const list = (value: string) =>
      value
        .split(/[,\n]/)
        .map((part) => part.trim())
        .filter(Boolean);
    const lines = (value: string) =>
      value
        .split("\n")
        .map((part) => part.trim())
        .filter(Boolean);
    const name = read("name").trim();
    const aspect = blank(read("aspect"));
    const platforms = list(read("platforms"));
    emit({
      type: "save",
      profile: {
        recipe: recipeInput.value,
        deliverables: [
          {
            id: deliverable?.id ?? "",
            name,
            audience: blank(read("audience")),
            platforms,
            runtime: blank(read("runtime")),
            aspect_ratio: aspect,
            resolution: blank(read("resolution")),
            frame_rate: blank(read("frame_rate")),
            language: blank(read("language")),
          },
        ],
        audience: blank(read("audience")),
        platforms,
        runtime: blank(read("runtime")),
        aspect_ratios: aspect ? [aspect] : [],
        resolution: blank(read("resolution")),
        frame_rate: blank(read("frame_rate")),
        language: blank(read("language")),
        medium: blank(read("medium")),
        style_notes: blank(read("style_notes")),
        audio_plan: blank(read("audio_plan")),
        continuity_rules: lines(read("continuity_rules")),
        safety_constraints: lines(read("safety_constraints")),
        budget: blank(read("budget")),
        schedule: blank(read("schedule")),
        rights_notes: blank(read("rights_notes")),
      },
    });
  });
  return form;
}

function renderRequirements(emit: (intent: ProfileIntent) => void): HTMLElement {
  const section = document.createElement("section");
  section.className = "workflow-requirements";
  section.setAttribute("aria-label", "Requirements");
  const heading = document.createElement("h2");
  heading.textContent = "Requirements";
  const count = document.createElement("span");
  const requirements = [...(getSurfaceHost().state?.requirements ?? [])];
  count.textContent = String(requirements.length);
  heading.append(count);
  const list = document.createElement("ul");
  list.className = "workflow-requirements-list";
  if (!requirements.length) {
    const empty = document.createElement("li");
    empty.className = "workflow-requirement empty";
    empty.textContent = "No requirements yet.";
    list.append(empty);
  }
  for (const requirement of requirements) {
    const item = document.createElement("li");
    item.className = "workflow-requirement";
    item.dataset.status = requirement.status;
    item.dataset.kind = requirement.kind;
    const selectedSceneId = getSurfaceHost().selectedSceneId;
    const inScene = selectedSceneId != null && (requirement.scene_ids ?? []).includes(selectedSceneId);
    if (inScene) item.classList.add("in-scene");
    if (inScene && getSurfaceHost().requirementBlocks(requirement)) item.classList.add("blocking");
    const kind = document.createElement("span");
    kind.className = "workflow-requirement-kind";
    kind.textContent = getSurfaceHost().requirementKindLabel(requirement.kind);
    const name = document.createElement("span");
    name.className = "workflow-requirement-name";
    name.textContent = requirement.name;
    const scenes = document.createElement("span");
    scenes.className = "workflow-requirement-scenes";
    scenes.textContent = (requirement.scene_ids ?? [])
      .map((id) => getSurfaceHost().state?.scenes.find((scene) => scene.id === id)?.title ?? `Scene ${id}`)
      .join(", ");
    const status = document.createElement("select");
    status.className = "workflow-requirement-status";
    status.setAttribute("aria-label", `${requirement.name} status`);
    status.disabled = Boolean(getSurfaceHost().busy);
    for (const option of REQUIREMENT_STATUSES) {
      const choice = document.createElement("option");
      choice.value = option.id;
      choice.textContent = option.label;
      choice.selected = option.id === requirement.status;
      status.append(choice);
    }
    status.addEventListener("change", () => {
      emit({
        type: "set-requirement",
        requirementId: requirement.id,
        status: status.value,
        name: requirement.name,
      });
    });
    item.append(kind, name, scenes, status);
    list.append(item);
  }
  const add = document.createElement("form");
  add.className = "workflow-requirement-add";
  const nameInput = document.createElement("input");
  nameInput.name = "name";
  nameInput.placeholder = "Add a requirement";
  nameInput.setAttribute("aria-label", "Requirement name");
  const kindInput = document.createElement("select");
  kindInput.name = "kind";
  kindInput.setAttribute("aria-label", "Requirement kind");
  for (const option of REQUIREMENT_KINDS) {
    const choice = document.createElement("option");
    choice.value = option.id;
    choice.textContent = option.label;
    kindInput.append(choice);
  }
  const button = document.createElement("button");
  button.type = "submit";
  button.className = "ghost-button";
  button.textContent = "Add";
  button.disabled = Boolean(getSurfaceHost().busy);
  add.append(nameInput, kindInput, button);
  add.addEventListener("submit", (event) => {
    event.preventDefault();
    const name = nameInput.value.trim();
    if (!name) return;
    emit({
      type: "add-requirement",
      name,
      kind: kindInput.value,
      sceneId: getSurfaceHost().selectedSceneId,
    });
  });
  section.append(heading, list, add);
  return section;
}

export function renderProjectProfile(onIntent: (intent: ProfileIntent) => void): HTMLElement {
  const profile = getSurfaceHost().state?.profile;
  const deliverable = profile?.deliverables?.[0];
  const bits = [
    deliverable?.name || getSurfaceHost().state?.project_name || "Untitled",
    deliverable?.aspect_ratio || profile?.aspect_ratios?.[0],
    deliverable?.runtime || profile?.runtime,
    profile?.medium,
  ].filter((bit): bit is string => Boolean(bit));
  const model = buildProfileModel({
    projectName: getSurfaceHost().state?.project_name ?? null,
    recipeLabel: getSurfaceHost().activeRecipe()?.label ?? null,
    summary: bits.join(" · "),
    requirementCount: getSurfaceHost().state?.requirements?.length ?? 0,
    busy: getSurfaceHost().busy,
    error: getSurfaceHost().error,
  });
  return presentSurface(paintProjectProfile(onIntent), model, false);
}
