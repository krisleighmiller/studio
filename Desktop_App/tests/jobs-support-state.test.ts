import { describe, expect, it } from "vitest";
import { buildJobsModel } from "../src/studio/jobsModel";
import { renderJobsView, type JobsIntent } from "../src/studio/jobsView";

const modelFor = (job: Parameters<typeof buildJobsModel>[0]["jobs"][number]) => buildJobsModel({
  jobs: [job], scenes: [], assets: [], filter: "all",
});

describe("job cancellation and resume presentation", () => {
  it("shows unknown provider capability, not a cancel or resume action, for an active provider job", () => {
    const model = modelFor({ id: "remote", kind: "render", provider: "runway", status: "running" });
    expect(model.rows[0].cancellation).toBe("Provider support unknown · unavailable in Studio");
    expect(model.rows[0].resume).toBe("Provider support unknown · unavailable in Studio");
    const intents: JobsIntent[] = [];
    const view = renderJobsView(model, true, "all", (intent) => intents.push(intent));
    expect(view.querySelector(".studio-job-support")?.textContent).toContain("Cancel: Provider support unknown · unavailable in Studio");
    expect(view.querySelector(".studio-job-support")?.textContent).toContain("Resume: Provider support unknown · unavailable in Studio");
    expect([...view.querySelectorAll("button")].some((button) => /^(cancel|resume)$/i.test(button.textContent?.trim() ?? ""))).toBe(false);
    expect(intents).toEqual([]);
  });

  it("offers read-only provider status recovery when a prediction ID is preserved", () => {
    const model = modelFor({
      id: "spend-status",
      kind: "atlas_generate_video",
      provider: "atlas-cloud",
      spend_category: "media",
      status: "failed",
      request_metadata: {
        request_hash: "a".repeat(64),
        prediction_id: "prediction-123",
      },
    });
    expect(model.rows[0].predictionId).toBe("prediction-123");
    expect(model.rows[0].actionLabel).toBe("Check provider status");
    expect(model.rows[0].resume).toContain("read-only status recovery available");
    const intents: JobsIntent[] = [];
    const view = renderJobsView(model, true, "all", (intent) => intents.push(intent));
    expect(view.textContent).toContain("Prediction prediction-123");
    view.querySelector<HTMLButtonElement>("button.studio-job")?.click();
    expect(intents).toEqual([{ type: "open", jobId: "spend-status" }]);
  });

  it("marks local assembly cancellation and resume unsupported without inventing controls", () => {
    const model = modelFor({ id: "local", kind: "assemble", status: "completed", output_path: "export.mp4" });
    expect(model.rows[0].cancellation).toBe("Unsupported for local assembly");
    expect(model.rows[0].resume).toBe("Unsupported for local assembly");
    const intents: JobsIntent[] = [];
    const view = renderJobsView(model, true, "all", (intent) => intents.push(intent));
    expect(view.querySelector(".studio-job-support")?.textContent).toContain("Cancel: Unsupported for local assembly");
    expect([...view.querySelectorAll("button")].some((button) => /^(cancel|resume)$/i.test(button.textContent?.trim() ?? ""))).toBe(false);
    view.querySelector<HTMLButtonElement>("button.studio-job")?.click();
    expect(intents).toEqual([{ type: "open", jobId: "local" }]);
  });

  it("does not infer provider support from a stored cancelled status or a raw provider ID", () => {
    const model = modelFor({ id: "old", kind: "render", provider: "runway", status: "cancelled", raw_provider_id: "scene_1" });
    expect(model.rows[0].cancellation).toContain("support unknown");
    expect(model.rows[0].resume).toContain("support unknown");
    const view = renderJobsView(model, true, "all", () => { throw new Error("unexpected action"); });
    expect(view.querySelector(".studio-job-title")?.textContent).toContain("Cancelled");
    expect(view.querySelector(".studio-job-support")?.textContent).toContain("unavailable in Studio");
  });
});
