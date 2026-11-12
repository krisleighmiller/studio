import { createRequire } from "node:module";
import { describe, expect, it } from "vitest";

const { HOST_COMMANDS } = createRequire(import.meta.url)("../electron/commandAllowlist.cjs");

describe("CR01 acquisition command boundary", () => {
  it("admits only the registered confined acquisition command", () => {
    expect(HOST_COMMANDS.has("ingest_character_reference_image")).toBe(true);
    expect(HOST_COMMANDS.has("ingest_character_reference_image_unchecked")).toBe(false);
  });
});
