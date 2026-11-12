import { createRequire } from "node:module";
import { expect, it } from "vitest";
const { HOST_COMMANDS } = createRequire(import.meta.url)("../electron/commandAllowlist.cjs");
it("CR02 admits reference-selection persistence, not unchecked variants", () => {
  expect(HOST_COMMANDS.has("workflow_save_character_reference_selection")).toBe(true);
  expect(HOST_COMMANDS.has("workflow_save_character_reference_selection_unchecked")).toBe(false);
});
