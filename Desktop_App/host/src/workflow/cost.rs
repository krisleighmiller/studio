//! Cost, budget, and provider-pricing foundations.
//!
//! Money is represented as integer minor units (cents, pence, …) so authoritative
//! totals never accumulate floating-point error. Unknown cost is an explicit
//! `None`/`Unknown`, never zero.
//!
//! The pricing registry is API-agnostic: it keys on the two data strings the app
//! already owns — `ApiEntry.kind` (a provider family) and a `capabilityDefaults`
//! key (a production operation). No enum of providers or capabilities lives in
//! this module; adding one is a data edit to `providers.json`, not a code change.

use crate::workflow::state::{JobStatus, ProjectState};
use serde::{Deserialize, Serialize};

/// A price the registry knows, or an explicit "we do not know" marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Price {
    Known { currency: String, minor: i64 },
    Unknown,
}

impl Price {
    pub fn minor(&self) -> Option<i64> {
        match self {
            Price::Known { minor, .. } => Some(*minor),
            Price::Unknown => None,
        }
    }

    pub fn currency(&self) -> Option<&str> {
        match self {
            Price::Known { currency, .. } => Some(currency),
            Price::Unknown => None,
        }
    }
}

/// Format integer minor units as a decimal string, e.g. `1234` → `"12.34 USD"`.
pub fn format_minor(minor: i64, currency: &str) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();
    let major = abs / 100;
    let frac = abs % 100;
    format!("{sign}{major}.{frac:02} {currency}")
}

//
// Provider pricing registry (data-driven and API-agnostic).
//
// Keys are data strings, not enums:
//   * `kind`       — an `ApiEntry.kind` ("atlas-cloud", "elevenlabs", …).
//   * `capability` — a `capabilityDefaults` key ("video.text_only", …).
//

#[derive(Debug, Clone, PartialEq)]
struct PriceEntry {
    kind: String,
    capability: String,
    currency: String,
    price_minor: i64,
    pricing_status: Option<String>,
    source_url: Option<String>,
    date_verified: Option<String>,
    billing_unit: Option<String>,
    notes: Option<String>,
}

fn registry() -> &'static [PriceEntry] {
    static REGISTRY: std::sync::OnceLock<Vec<PriceEntry>> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| {
        let raw: serde_json::Value = serde_json::from_str(include_str!("providers.json"))
            .expect("providers.json is invalid");
        raw["prices"]
            .as_array()
            .expect("providers.json needs a prices array")
            .iter()
            .map(|item| PriceEntry {
                kind: item["kind"].as_str().unwrap_or("").to_string(),
                capability: item["capability"].as_str().unwrap_or("").to_string(),
                currency: item["currency"].as_str().unwrap_or("USD").to_string(),
                price_minor: item["price_minor"].as_i64().unwrap_or(0),
                pricing_status: item["pricing_status"].as_str().map(|s| s.to_string()),
                source_url: item["source_url"].as_str().map(|s| s.to_string()),
                date_verified: item["date_verified"].as_str().map(|s| s.to_string()),
                billing_unit: item["billing_unit"].as_str().map(|s| s.to_string()),
                notes: item["notes"].as_str().map(|s| s.to_string()),
            })
            .collect()
    })
}

/// Resolve a known price for a provider `kind` + `capability`, case-insensitively.
/// Anything not listed resolves to `Price::Unknown`.
pub fn resolve_price(kind: &str, capability: &str) -> Price {
    let key = |s: &str| s.trim().to_ascii_lowercase();
    let kind_key = key(kind);
    let capability_key = key(capability);
    registry()
        .iter()
        .find(|entry| key(&entry.kind) == kind_key && key(&entry.capability) == capability_key)
        .and_then(|entry| {
            // Entries with unresolved or genuinely unknown pricing resolve to Unknown.
            match entry.pricing_status.as_deref() {
                Some("placeholder") | Some("unknown") => None,
                _ => Some(Price::Known {
                    currency: entry.currency.clone(),
                    minor: entry.price_minor,
                }),
            }
        })
        .unwrap_or(Price::Unknown)
}

/// A pricing issue surfaced by `validate_pricing`.
#[derive(Debug, Clone, PartialEq)]
pub struct PricingWarning {
    pub kind: String,
    pub capability: String,
    pub issue: String,
}

/// Check every registered price entry for unresolved placeholders, missing
/// required metadata, or stale version markers. Returns warnings for each
/// issue found — an empty vec means the registry is clean.
pub fn validate_pricing() -> Vec<PricingWarning> {
    let raw: serde_json::Value =
        serde_json::from_str(include_str!("providers.json")).expect("providers.json is invalid");
    let mut warnings = Vec::new();

    // Stale version marker.
    if let Some(ver) = raw["pricing_version"].as_str() {
        if ver.contains("placeholder") {
            warnings.push(PricingWarning {
                kind: "".to_string(),
                capability: "".to_string(),
                issue: format!("pricing_version still contains 'placeholder': \"{}\"", ver),
            });
        }
    }

    // Check each entry.
    for entry in raw["prices"].as_array().unwrap_or(&vec![]) {
        let kind = entry["kind"].as_str().unwrap_or("").to_string();
        let capability = entry["capability"].as_str().unwrap_or("").to_string();
        let status = entry["pricing_status"].as_str();
        let has_source = entry["source_url"].as_str().filter(|s| !s.is_empty());
        let has_date = entry["date_verified"].as_str().filter(|s| !s.is_empty());

        match status {
            Some("placeholder") => {
                warnings.push(PricingWarning {
                    kind,
                    capability,
                    issue: "pricing_status is 'placeholder' — not yet researched".to_string(),
                });
            }
            None => {
                // No explicit status: legacy entry. Treat as "known" but check metadata.
                if has_source.is_none() {
                    warnings.push(PricingWarning {
                        kind: kind.clone(),
                        capability: capability.clone(),
                        issue: "known-pricing entry missing source_url".to_string(),
                    });
                }
                if has_date.is_none() {
                    warnings.push(PricingWarning {
                        kind: kind.clone(),
                        capability: capability.clone(),
                        issue: "known-pricing entry missing date_verified".to_string(),
                    });
                }
            }
            Some("known") => {
                if has_source.is_none() {
                    warnings.push(PricingWarning {
                        kind: kind.clone(),
                        capability: capability.clone(),
                        issue: "known-pricing entry missing source_url".to_string(),
                    });
                }
                if has_date.is_none() {
                    warnings.push(PricingWarning {
                        kind: kind.clone(),
                        capability: capability.clone(),
                        issue: "known-pricing entry missing date_verified".to_string(),
                    });
                }
            }
            // "unknown" — intentionally unresolved; no warning for missing metadata.
            _ => {}
        }
    }

    warnings
}

/// Map a Director specialist name to the capability key its paid work uses.
/// Non-provider specialists (assembly, text writes, …) resolve to `None`, so
/// their cost is honestly reported as unknown rather than invented.
pub fn specialist_capability(specialist: &str) -> Option<&'static str> {
    match specialist.trim().to_ascii_lowercase().as_str() {
        "still_image" | "scene_look" | "model_sheet" | "wardrobe_plate" | "reference_assets" => {
            Some("image.text_to_image")
        }
        "video" | "render" | "scene_generation" => Some("video.text_only"),
        "voice_clone" => Some("audio.tts_clone"),
        "tts" | "tts_profile" => Some("audio.tts_profile"),
        "lip_sync" => Some("video.lip_sync_text_narration"),
        _ => None,
    }
}

/// Known price as `(currency, minor_units)` for a provider `kind` + specialist,
/// or `None` when unknown. Unknown is not zero.
pub fn known_price(kind: &str, specialist: &str) -> Option<(String, i64)> {
    let capability = specialist_capability(specialist)?;
    match resolve_price(kind, capability) {
        Price::Known { currency, minor } => Some((currency, minor)),
        Price::Unknown => None,
    }
}

/// Host-known price as a display label, e.g. `"0.05 USD"`.
pub fn known_price_label(kind: &str, specialist: &str) -> Option<String> {
    known_price(kind, specialist).map(|(currency, minor)| format_minor(minor, &currency))
}

/// Known spend in minor units, or `None` when unknown (not gated numerically).
pub fn known_spend_minor(kind: &str, specialist: &str) -> Option<i64> {
    known_price(kind, specialist).map(|(_, minor)| minor)
}

//
// Budget ledger: derive authoritative spend totals from project state.
//

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetLedger {
    pub currency: String,
    pub limit_minor: Option<i64>,
    pub warning_at_minor: Option<i64>,
    pub hard_stop: bool,
    /// Sum of known estimates for pending/running (committed but unsettled) jobs.
    pub committed_minor: i64,
    /// Completed jobs with an estimate but no reported actual; reservation, not spend.
    pub unsettled_estimated_minor: i64,
    /// Sum of actual cost for completed jobs.
    pub completed_minor: i64,
    /// Sum of billable cost for failed/cancelled jobs (retried spend is still spend).
    pub failed_minor: i64,
    /// `limit - (committed + unsettled estimates + actual spend)`; no limit → `None`.
    pub remaining_minor: Option<i64>,
    /// Count of jobs without a usable actual or estimate in the ledger currency.
    /// Includes pending/running jobs without estimates and completed jobs without actuals.
    pub unknown_cost_jobs: usize,
    /// Budget gate derived from reservations plus actual spend vs thresholds.
    pub gate: SpendGate,
}

pub fn compute_ledger(state: &ProjectState) -> BudgetLedger {
    let budget = state.budget.clone().unwrap_or_default();
    let mut committed = 0i64;
    let mut unsettled = 0i64;
    let mut completed = 0i64;
    let mut failed = 0i64;
    let mut unknown = 0usize;
    for job in &state.jobs {
        // Local assembly/import jobs never call a paid provider. Do not let an
        // unpriced local job veto unrelated paid work.
        if job.provider.is_none()
            && job.spend_category.is_none()
            && job.cost_minor.is_none()
            && job.estimated_cost_minor.is_none()
        {
            continue;
        }
        // A numeric total in the budget currency cannot include an unconverted
        // charge. Legacy jobs without currency retain their historical assumption.
        if job
            .currency
            .as_deref()
            .is_some_and(|currency| !currency.trim().eq_ignore_ascii_case(&budget.currency))
        {
            unknown += 1;
            continue;
        }
        match job.status {
            JobStatus::Pending | JobStatus::Running => {
                if let Some(estimate) = job.estimated_cost_minor.filter(|amount| *amount >= 0) {
                    committed = committed.saturating_add(estimate);
                } else {
                    unknown += 1;
                }
            }
            JobStatus::Completed => match job.cost_minor.filter(|amount| *amount >= 0) {
                Some(cost) => completed = completed.saturating_add(cost),
                None => {
                    unknown += 1;
                    if let Some(estimate) = job.estimated_cost_minor.filter(|amount| *amount >= 0) {
                        unsettled = unsettled.saturating_add(estimate);
                    }
                }
            },
            JobStatus::Failed | JobStatus::Cancelled => {
                match job.cost_minor.filter(|amount| *amount >= 0) {
                    Some(cost) => failed = failed.saturating_add(cost),
                    None => unknown += 1,
                }
            }
        }
    }
    let spent = completed.saturating_add(failed);
    let on_the_hook = spent.saturating_add(committed).saturating_add(unsettled);
    let remaining = budget
        .limit_minor
        .map(|limit| limit.saturating_sub(on_the_hook));
    // Compute gate inline to avoid infinite recursion (gate_spend calls compute_ledger).
    let gate = match budget.limit_minor {
        None => SpendGate::NoLimit,
        Some(limit) => {
            if on_the_hook > limit {
                if budget.hard_stop {
                    SpendGate::Blocked
                } else {
                    SpendGate::Confirm
                }
            } else if let Some(warn_at) = budget.warning_at_minor {
                if on_the_hook > warn_at {
                    SpendGate::Warn
                } else {
                    SpendGate::Allow
                }
            } else {
                SpendGate::Allow
            }
        }
    };
    BudgetLedger {
        currency: budget.currency,
        limit_minor: budget.limit_minor,
        warning_at_minor: budget.warning_at_minor,
        hard_stop: budget.hard_stop,
        committed_minor: committed,
        unsettled_estimated_minor: unsettled,
        completed_minor: completed,
        failed_minor: failed,
        remaining_minor: remaining,
        unknown_cost_jobs: unknown,
        gate,
    }
}

/// Whether a proposed spend can proceed under the project budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpendGate {
    /// No budget limit configured — always allowed.
    NoLimit,
    /// Under both the warning threshold and the limit.
    Allow,
    /// Over the warning threshold but under the limit.
    Warn,
    /// Would exceed the limit, but the budget is not hard-stop: explicit
    /// confirmation is required.
    Confirm,
    /// Would exceed the limit and the budget is hard-stop: vetoed.
    Blocked,
}

/// Decide whether `proposed_minor` of new spend is allowed given committed,
/// completed, and failed spend already in the project. A hard-stop budget
/// blocks spending over its limit; this is the mechanism the Director cannot
/// bypass.
pub fn gate_spend(state: &ProjectState, proposed_minor: i64) -> SpendGate {
    let Some(budget) = &state.budget else {
        return SpendGate::NoLimit;
    };
    let Some(limit) = budget.limit_minor else {
        return SpendGate::NoLimit;
    };
    let ledger = compute_ledger(state);
    // Use a wider intermediate: saturating i64 addition could turn
    // MAX + one minor unit into MAX, incorrectly approving a hard-stop call.
    let on_the_hook = i128::from(ledger.committed_minor)
        + i128::from(ledger.unsettled_estimated_minor)
        + i128::from(ledger.completed_minor)
        + i128::from(ledger.failed_minor);
    let total = on_the_hook + i128::from(proposed_minor.max(0));
    if total > i128::from(limit) {
        return if budget.hard_stop {
            SpendGate::Blocked
        } else {
            SpendGate::Confirm
        };
    }
    if let Some(warning) = budget.warning_at_minor {
        if total > i128::from(warning) {
            return SpendGate::Warn;
        }
    }
    SpendGate::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::state::{JobRecord, JobStatus, ProjectBudget};

    fn job(
        kind: &str,
        status: JobStatus,
        cost_minor: Option<i64>,
        estimated_minor: Option<i64>,
    ) -> JobRecord {
        let mut job = JobRecord::open("job", kind);
        job.status = status;
        job.provider = Some("legacy-provider".into());
        job.cost_minor = cost_minor;
        job.estimated_cost_minor = estimated_minor;
        job
    }

    #[test]
    fn gate_does_not_allow_a_positive_charge_when_minor_units_overflow_i64() {
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget {
            currency: "USD".into(),
            limit_minor: Some(i64::MAX),
            warning_at_minor: None,
            hard_stop: true,
        });
        state
            .jobs
            .push(job("image", JobStatus::Completed, Some(i64::MAX), None));
        assert_eq!(gate_spend(&state, 1), SpendGate::Blocked);
    }

    #[test]
    fn registry_resolves_known_price_case_insensitively() {
        assert_eq!(
            resolve_price("ATLAS-CLOUD", " IMAGE.TEXT_TO_IMAGE "),
            Price::Known {
                currency: "USD".to_string(),
                minor: 7
            }
        );
        assert_eq!(
            resolve_price("atlas-cloud", "video.text_only"),
            Price::Unknown // video pricing is placeholder — not reliably known per-op
        );
    }

    #[test]
    fn registry_marks_unlisted_entries_unknown_not_zero() {
        assert_eq!(resolve_price("runway", "video.text_only"), Price::Unknown);
        assert_eq!(
            resolve_price("atlas-cloud", "audio.tts_custom"),
            Price::Unknown
        );
        assert_eq!(resolve_price("", ""), Price::Unknown);
        assert_eq!(
            resolve_price("elevenlabs", "video.text_only"),
            Price::Unknown
        );
        // The clone operation is zero, not a zero-cost TTS generation.
        assert_eq!(
            resolve_price("elevenlabs", "audio.tts_clone"),
            Price::Known {
                currency: "USD".into(),
                minor: 0
            }
        );
        assert_eq!(
            resolve_price("elevenlabs", "audio.tts_profile"),
            Price::Unknown
        );
        assert_eq!(resolve_price("elevenlabs", "voice_design"), Price::Unknown);
        assert_eq!(resolve_price("elevenlabs", "voice_clone"), Price::Unknown);
    }

    #[test]
    fn minor_units_format_to_decimal_without_float_error() {
        assert_eq!(format_minor(1234, "USD"), "12.34 USD");
        assert_eq!(format_minor(400, "USD"), "4.00 USD");
        assert_eq!(format_minor(5, "USD"), "0.05 USD");
        assert_eq!(format_minor(-1234, "USD"), "-12.34 USD");
    }

    #[test]
    fn ledger_sums_spend_and_keeps_unknown_separate_from_zero() {
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget {
            currency: "USD".to_string(),
            limit_minor: Some(10_000),
            warning_at_minor: Some(7_000),
            hard_stop: true,
        });
        state.jobs = vec![
            job("render", JobStatus::Pending, None, Some(1_000)),
            job("render", JobStatus::Completed, Some(2_000), Some(1_500)),
            job("look", JobStatus::Failed, Some(500), Some(250)),
            job("look", JobStatus::Completed, None, None), // unknown-cost completed job
            job("look", JobStatus::Cancelled, Some(300), None),
        ];

        let ledger = compute_ledger(&state);
        assert_eq!(ledger.currency, "USD");
        assert_eq!(ledger.committed_minor, 1_000);
        assert_eq!(ledger.completed_minor, 2_000);
        assert_eq!(ledger.failed_minor, 800);
        assert_eq!(ledger.remaining_minor, Some(6_200));
        assert_eq!(ledger.unknown_cost_jobs, 1);
        assert!(ledger.hard_stop);
    }

    #[test]
    fn ledger_tracks_unsettled_completed_estimates_without_reporting_them_as_actual() {
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget {
            limit_minor: Some(1_000),
            hard_stop: true,
            ..Default::default()
        });
        state.jobs = vec![
            job("render", JobStatus::Pending, None, Some(100)),
            job("render", JobStatus::Completed, None, Some(400)),
            job("render", JobStatus::Completed, Some(200), Some(900)),
        ];
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.committed_minor, 100);
        assert_eq!(ledger.unsettled_estimated_minor, 400);
        assert_eq!(ledger.completed_minor, 200);
        assert_eq!(ledger.unknown_cost_jobs, 1);
        assert_eq!(ledger.remaining_minor, Some(300));
        assert_eq!(gate_spend(&state, 301), SpendGate::Blocked);
    }

    #[test]
    fn ledger_does_not_add_foreign_currency_to_budget_totals() {
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget {
            currency: "USD".into(),
            limit_minor: Some(100),
            hard_stop: true,
            ..Default::default()
        });
        let mut foreign = job("render", JobStatus::Completed, Some(999), Some(999));
        foreign.currency = Some("EUR".into());
        state.jobs = vec![foreign];
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.completed_minor, 0);
        assert_eq!(ledger.unsettled_estimated_minor, 0);
        assert_eq!(ledger.remaining_minor, Some(100));
        assert_eq!(ledger.unknown_cost_jobs, 1);
        assert_eq!(gate_spend(&state, 100), SpendGate::Allow);
    }

    #[test]
    fn ledger_counts_unpriced_inflight_jobs_and_does_not_reserve_negative_estimates() {
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget {
            limit_minor: Some(100),
            hard_stop: true,
            ..Default::default()
        });
        state.jobs = vec![
            job("render", JobStatus::Pending, None, None),
            job("render", JobStatus::Running, None, Some(-300)),
        ];
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.committed_minor, 0);
        assert_eq!(ledger.unknown_cost_jobs, 2);
        assert_eq!(ledger.remaining_minor, Some(100));
        assert_eq!(gate_spend(&state, 101), SpendGate::Blocked);
    }

    #[test]
    fn ledger_treats_negative_actual_cost_as_unknown_not_a_credit() {
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget {
            limit_minor: Some(100),
            hard_stop: true,
            ..Default::default()
        });
        state.jobs = vec![job("render", JobStatus::Completed, Some(-50), Some(80))];
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.completed_minor, 0);
        assert_eq!(ledger.unsettled_estimated_minor, 80);
        assert_eq!(ledger.unknown_cost_jobs, 1);
        assert_eq!(ledger.remaining_minor, Some(20));
    }

    #[test]
    fn ledger_treats_negative_failed_cost_as_unknown_not_a_credit() {
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget {
            limit_minor: Some(100),
            ..Default::default()
        });
        state.jobs = vec![job("render", JobStatus::Failed, Some(-50), Some(80))];
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.failed_minor, 0);
        assert_eq!(ledger.unknown_cost_jobs, 1);
        assert_eq!(ledger.remaining_minor, Some(100));
    }

    #[test]
    fn ledger_without_a_limit_has_no_remaining_but_still_tracks_unknown() {
        let mut state = ProjectState::new();
        state.jobs = vec![
            job("render", JobStatus::Completed, None, None),
            job("render", JobStatus::Completed, Some(123), None),
        ];
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.remaining_minor, None);
        assert_eq!(ledger.completed_minor, 123);
        assert_eq!(ledger.unknown_cost_jobs, 1);
        assert_eq!(ledger.currency, "USD");
    }

    #[test]
    fn missing_budget_defaults_to_usd_and_no_limit() {
        let state = ProjectState::new();
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.currency, "USD");
        assert_eq!(ledger.limit_minor, None);
        assert!(!ledger.hard_stop);
    }

    #[test]
    fn specialist_names_map_to_capability_keys() {
        assert_eq!(
            specialist_capability("still_image"),
            Some("image.text_to_image")
        );
        assert_eq!(
            specialist_capability("scene_look"),
            Some("image.text_to_image")
        );
        assert_eq!(specialist_capability("VIDEO"), Some("video.text_only"));
        assert_eq!(
            specialist_capability("voice_clone"),
            Some("audio.tts_clone")
        );
        assert_eq!(specialist_capability("assemble"), None);
        assert_eq!(specialist_capability("text_write"), None);
        assert_eq!(specialist_capability(""), None);
    }

    #[test]
    fn known_price_label_resolves_known_and_unknown() {
        assert_eq!(
            known_price_label("atlas-cloud", "still_image"),
            Some("0.07 USD".to_string())
        );
        assert_eq!(
            known_price_label("atlas-cloud", "video"),
            None // video is placeholder — price depends on model+duration
        );
        assert_eq!(
            known_price_label("elevenlabs", "voice_clone"),
            Some("0.00 USD".to_string()) // no per-clone API charge
        );
        // TTS pricing is per-character — genuinely unknown per operation.
        assert_eq!(known_price_label("elevenlabs", "tts"), None);
        // Unknown kind, unknown capability, non-provider specialist: unknown.
        assert_eq!(known_price_label("runway", "still_image"), None);
        assert_eq!(known_price_label("atlas-cloud", "assemble"), None);
        assert_eq!(known_price_label("atlas-cloud", ""), None);
    }

    #[test]
    fn known_spend_minor_returns_minor_units_or_unknown() {
        assert_eq!(known_spend_minor("atlas-cloud", "still_image"), Some(7));
        assert_eq!(known_spend_minor("atlas-cloud", "video"), None); // placeholder
        assert_eq!(known_spend_minor("elevenlabs", "voice_clone"), Some(0));
        // TTS is unknown per operation — None, never 0.
        assert_eq!(known_spend_minor("elevenlabs", "tts"), None);
        // Unknown spend is None, never 0.
        assert_eq!(known_spend_minor("runway", "still_image"), None);
        assert_eq!(known_spend_minor("atlas-cloud", "assemble"), None);
    }

    #[test]
    fn the_director_cannot_bypass_a_hard_stop_for_known_spend() {
        let state = ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(5),
                warning_at_minor: None,
                hard_stop: true,
            }),
            ..ProjectState::new()
        };
        // Known image spend (7 minor) over a 5-minor hard-stop limit → vetoed.
        let minor = known_spend_minor("atlas-cloud", "still_image").unwrap();
        assert_eq!(gate_spend(&state, minor), SpendGate::Blocked);
        // Under the limit with the same spend when budget is higher.
        let state2 = ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(10),
                warning_at_minor: None,
                hard_stop: true,
            }),
            ..ProjectState::new()
        };
        assert_eq!(gate_spend(&state2, minor), SpendGate::Allow);
        // Unknown spend has no minor value to compare, so it is not vetoed here;
        // the ledger flags it separately.
        assert_eq!(known_spend_minor("elevenlabs", "tts"), None);
        assert_eq!(known_spend_minor("runway", "still_image"), None);
    }

    #[test]
    fn a_completed_job_keeps_its_estimate_separate_from_actual_cost() {
        let mut job = JobRecord::open("j", "scene_look");
        job.estimated_cost_minor = Some(40);
        job.currency = Some("USD".to_string());
        job.finish(Some("out.png"), None);
        assert_eq!(job.cost_minor, None);
        assert_eq!(job.estimated_cost_minor, Some(40));

        let mut state = ProjectState::new();
        state.jobs = vec![job];
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.completed_minor, 0);
        assert_eq!(ledger.unknown_cost_jobs, 1);
    }

    #[test]
    fn failed_jobs_with_billable_cost_flow_into_ledger() {
        let mut job = JobRecord::open("j", "render");
        job.estimated_cost_minor = Some(500);
        job.currency = Some("USD".to_string());
        // Billable failure: provider charged despite the error.
        job.fail(Some(350), "GPU preempted");
        assert_eq!(job.cost_minor, Some(350));
        assert_eq!(job.error.as_deref(), Some("GPU preempted"));

        let mut state = ProjectState::new();
        state.jobs = vec![job];
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.failed_minor, 350);
        assert_eq!(ledger.completed_minor, 0);
        assert_eq!(ledger.unknown_cost_jobs, 0);
    }

    #[test]
    fn failed_jobs_without_billable_cost_are_unknown_cost() {
        let mut job = JobRecord::open("j", "render");
        job.estimated_cost_minor = Some(500);
        job.currency = Some("USD".to_string());
        // Non-billable failure: provider didn't charge for the failed attempt.
        job.fail(None, "timeout");
        assert!(job.cost_minor.is_none());

        let mut state = ProjectState::new();
        state.jobs = vec![job];
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.failed_minor, 0);
        assert_eq!(ledger.unknown_cost_jobs, 1);
    }

    #[test]
    fn ledger_gate_reports_budget_status_at_all_thresholds() {
        // ---- No limit ----
        let state = ProjectState {
            budget: None,
            ..ProjectState::new()
        };
        assert_eq!(compute_ledger(&state).gate, SpendGate::NoLimit);

        // ---- Under warn threshold, under limit ----
        let state = ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(10_000),
                warning_at_minor: Some(7_000),
                hard_stop: true,
            }),
            // committed 500 + completed 1_000 = on_the_hook = 1_500 → under warn (7_000)
            jobs: vec![
                job("r1", JobStatus::Pending, None, Some(500)),
                job("r2", JobStatus::Completed, Some(1_000), None),
            ],
            ..ProjectState::new()
        };
        assert_eq!(compute_ledger(&state).gate, SpendGate::Allow);

        // ---- Over warn, under limit ----
        let state = ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(10_000),
                warning_at_minor: Some(2_000),
                hard_stop: true,
            }),
            // committed 500 + completed 2_000 = 2_500 → over warn (2_000), under limit
            jobs: vec![
                job("r1", JobStatus::Pending, None, Some(500)),
                job("r2", JobStatus::Completed, Some(2_000), None),
            ],
            ..ProjectState::new()
        };
        assert_eq!(compute_ledger(&state).gate, SpendGate::Warn);

        // ---- Over limit, soft stop → Confirm ----
        let state = ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(2_000),
                warning_at_minor: None,
                hard_stop: false,
            }),
            // committed 500 + completed 2_000 = 2_500 → over limit (2_000), soft
            jobs: vec![
                job("r1", JobStatus::Pending, None, Some(500)),
                job("r2", JobStatus::Completed, Some(2_000), None),
            ],
            ..ProjectState::new()
        };
        assert_eq!(compute_ledger(&state).gate, SpendGate::Confirm);

        // ---- Over limit, hard stop → Blocked ----
        let state = ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(2_000),
                warning_at_minor: None,
                hard_stop: true,
            }),
            jobs: vec![
                job("r1", JobStatus::Pending, None, Some(500)),
                job("r2", JobStatus::Completed, Some(2_000), None),
            ],
            ..ProjectState::new()
        };
        assert_eq!(compute_ledger(&state).gate, SpendGate::Blocked);

        // ---- No warning configured, under limit → Allow ----
        let state = ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(10_000),
                warning_at_minor: None,
                hard_stop: true,
            }),
            // committed 500 + completed 1_000 = 1_500 → under limit (10_000), no warn
            jobs: vec![
                job("r1", JobStatus::Pending, None, Some(500)),
                job("r2", JobStatus::Completed, Some(1_000), None),
            ],
            ..ProjectState::new()
        };
        assert_eq!(compute_ledger(&state).gate, SpendGate::Allow);
    }

    #[test]
    fn gate_spend_applies_warning_confirm_and_hard_stop() {
        let budgeted = |limit: i64, warning: i64, hard_stop: bool| ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(limit),
                warning_at_minor: Some(warning),
                hard_stop,
            }),
            ..ProjectState::new()
        };

        // No budget, and a budget with no limit: always allowed.
        assert_eq!(
            gate_spend(&ProjectState::new(), 100_000),
            SpendGate::NoLimit
        );
        assert_eq!(
            gate_spend(&ProjectState::default(), 100_000),
            SpendGate::NoLimit
        );

        let mut hard = budgeted(10_000, 7_000, true);
        assert_eq!(gate_spend(&hard, 1_000), SpendGate::Allow);
        assert_eq!(gate_spend(&hard, 8_000), SpendGate::Warn);
        assert_eq!(gate_spend(&hard, 11_000), SpendGate::Blocked);

        let soft = budgeted(10_000, 7_000, false);
        assert_eq!(gate_spend(&soft, 11_000), SpendGate::Confirm);

        // Already over the limit from completed spend: hard stop blocks even a
        // zero-cost proposal, and a negative proposal cannot buy headroom.
        hard.jobs = vec![job("render", JobStatus::Completed, Some(12_000), None)];
        assert_eq!(gate_spend(&hard, 0), SpendGate::Blocked);
        assert_eq!(gate_spend(&hard, -5_000), SpendGate::Blocked);
    }

    #[test]
    fn validate_pricing_reports_only_placeholder_warnings() {
        let warnings = validate_pricing();
        // Video entries with "placeholder" status are expected — their
        // cost depends on model and duration. The test verifies there
        // are no *unexpected* warnings.
        let unexpected: Vec<_> = warnings
            .iter()
            .filter(|w| {
                w.kind != "video"
                    && w.capability != "video.text_encoding"
                    && !w.issue.contains("placeholder")
                    && !w.issue.contains("unknown")
            })
            .collect();
        assert!(
            unexpected.is_empty(),
            "unexpected pricing warnings: {:?}",
            unexpected
        );
    }

    #[test]
    fn pricing_status_unknown_returns_unknown_price() {
        // ElevenLabs TTS: pricing_status = "unknown" → resolve_price returns Unknown
        // even though the entry exists in the registry.
        assert_eq!(
            resolve_price("elevenlabs", "audio.tts_profile"),
            Price::Unknown
        );
    }

    #[test]
    fn pricing_status_known_entry_returns_known_price() {
        // Voice clone is Known(0) — the API has no per-clone charge.
        assert_eq!(
            resolve_price("elevenlabs", "audio.tts_clone"),
            Price::Known {
                currency: "USD".to_string(),
                minor: 0
            }
        );
    }

    #[test]
    fn video_capabilities_are_placeholder_and_return_unknown() {
        // Video costs depend on model and duration — the registry marks them
        // as "placeholder" so resolve_price returns Unknown rather than an
        // inaccurate fixed price.
        for cap in &[
            "video.text_only",
            "video.start_frame",
            "video.edit",
            "video.character_reference",
            "video.location_prop_other_reference",
            "video.lip_sync_text_narration",
            "video.lip_sync_audio_included",
        ] {
            assert_eq!(
                resolve_price("atlas-cloud", cap),
                Price::Unknown,
                "capability {cap} should be Unknown (placeholder)"
            );
        }
    }

    // ── Regression: Finding 2 — boundary semantics ──────────────────────

    fn budget_with(limit: Option<i64>, warning: Option<i64>, hard_stop: bool) -> ProjectBudget {
        ProjectBudget {
            limit_minor: limit,
            warning_at_minor: warning,
            hard_stop,
            ..Default::default()
        }
    }

    #[test]
    fn gate_spend_uses_strict_greater_than_not_gte() {
        let mut state = ProjectState::default();
        // Spend = 1000, limit = 1000 — exact-equal should NOT block.
        state.budget = Some(budget_with(Some(1000), None, true));
        state.jobs = vec![job("render", JobStatus::Completed, Some(1000), Some(1000))];

        let gate = gate_spend(&state, 0);
        assert!(!matches!(gate, SpendGate::Blocked),
            "at exact limit (on_the_hook == limit) gate should not be Blocked (strict >); got {gate:?}");
        assert!(
            matches!(gate, SpendGate::Allow | SpendGate::NoLimit),
            "at exact limit, gate should be Allow or NoLimit; got {gate:?}"
        );

        // One cent over should block.
        let gate = gate_spend(&state, 1);
        assert!(
            matches!(gate, SpendGate::Blocked),
            "one cent over limit should be Blocked; got {gate:?}"
        );
    }

    #[test]
    fn warning_threshold_also_uses_strict_greater_than() {
        let mut state = ProjectState::default();
        // Spend = 500, warning = 500 — exact-equal should NOT warn.
        state.budget = Some(budget_with(Some(2000), Some(500), false));
        state.jobs = vec![job("render", JobStatus::Completed, Some(500), Some(500))];

        let gate = gate_spend(&state, 0);
        assert!(
            !matches!(gate, SpendGate::Warn),
            "at exact warning threshold, gate should not be Warn (strict >); got {gate:?}"
        );

        // One cent over should warn (but not block since we're under the limit).
        let gate = gate_spend(&state, 1);
        assert_eq!(
            gate,
            SpendGate::Warn,
            "one cent over warning should be Warn; got {gate:?}"
        );
    }

    // ── Regression: Finding 3 — fail-closed on missing budget state ─────

    #[test]
    fn gate_spend_with_no_budget_returns_nolimit() {
        // When no budget is configured, gate_spend returns NoLimit.
        // The director is responsible for failing closed in that scenario —
        // gate_spend itself does not block absent a budget.
        let state = ProjectState::default();
        assert_eq!(gate_spend(&state, 100), SpendGate::NoLimit);
    }

    // ── Regression: Finding 4 — JobRecord fields exist and are populated ─

    #[test]
    fn jobrecord_fields_provider_model_scope_output_attempts_retry_of_exist() {
        let mut job = JobRecord::open("job-1", "render");

        // Populate the six fields the audit questioned.
        job.provider = Some("atlas-cloud".into());
        job.model = Some("ray-3".into());
        job.scope = Some(crate::workflow::state::JobScope {
            kind: "scene".into(),
            id: "s1".into(),
        });
        job.output_asset_ids = vec!["asset-a".into(), "asset-b".into()];
        job.retry_of = Some("job-0".into());
        // attempts is populated at open(); verify it.
        assert_eq!(
            job.attempts.len(),
            1,
            "open() should seed one pending attempt"
        );
        assert_eq!(job.attempts[0].status, JobStatus::Pending);

        // Verify all six fields hold the values we set.
        assert_eq!(job.provider.as_deref(), Some("atlas-cloud"));
        assert_eq!(job.model.as_deref(), Some("ray-3"));
        assert!(job.scope.is_some());
        assert_eq!(job.output_asset_ids.len(), 2);
        assert_eq!(job.retry_of.as_deref(), Some("job-0"));
        assert_eq!(job.attempts.len(), 1);
    }

    #[test]
    fn jobrecord_provider_and_model_populated_from_estimate() {
        // Simulate what estimate_for_action does — set provider + model
        // as Option<String> from an estimate. The actual population sites
        // are in look.rs / start_frames.rs / production.rs / voice.rs.
        let mut job = JobRecord::open("job-2", "video.start_frame");
        assert!(job.provider.is_none());
        assert!(job.model.is_none());

        // simulate estimate_for_action
        job.provider = Some("atlas-cloud".into());
        job.model = Some("ray-3".into());

        assert_eq!(job.provider.as_deref(), Some("atlas-cloud"));
        assert_eq!(job.model.as_deref(), Some("ray-3"));
    }
}
