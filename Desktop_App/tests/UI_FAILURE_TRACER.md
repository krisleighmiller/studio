# A02 local failed-provider and approval-lifecycle tracer

This Linux-only scenario extends the accepted `ui-tracer.mjs` ownership/native
Recent/real Electron/renderer/real Rust-host gate. It uses a synthetic empty-room
project, an actual generated one-second local MP4, a dedicated loopback planner
and a feature-gated loopback Atlas fixture. It does not use Black_Hole originals,
live Studio providers, real credentials, or human creative/listening acceptance.

## Build and isolate

From `Desktop_App`, with installed dependencies and exclusive checkout/runtime
ownership:

```sh
cargo build --offline --manifest-path host/Cargo.toml
# Preserve ordinary executable at the card's ordinary-workbench-host path.
cargo build --offline --manifest-path host/Cargo.toml --features mock-provider
# Preserve feature executable at the card's fixture-workbench-host path.
# Restore the ordinary executable at host/target/debug/workbench-host.
```

The scenario explicitly selects the preserved feature executable; it never
requires leaving a mock executable at the default app path. Both executable
identities and `host_build_info` must be retained. The A02 evidence path is
`/home/kris/.hermes/profiles/studio/kanban-plans/current-roadmap/execution/A02`.
Run with a **new**, normalized, card-owned absolute output directory:

```sh
node tests/ui-failure-tracer.mjs /home/kris/.hermes/profiles/studio/kanban-plans/current-roadmap/execution/A02/NEW-RUN
```

Follow `UI_TRACER.md` for X11/Secret Service prerequisites. Ports/settings/recents,
HOME/cache/data/tmp, API IDs and process groups are isolated per run. Only exact
fixture IDs are deleted. A local deny proxy/netlog guard is not an OS network
sandbox. Native key input includes an 80ms key-down interval: instantaneous
press/release missed native menu input in the observed compositor session.

## Exercised behavior

1. Existing Scene Look **Generate still** prepares a genuine host approval.
   Visible mouse approval consumes it but cannot submit before the separate
   native spend warning. XTest accepts that warning; no dialog handler or bridge
   is replaced. The fixture receives one generate, then two polls: processing,
   failed. No image/download/asset is registered. Both owning workflow job and
   spend attempt persist failed; actual cost is unverified, not zero.
2. Native Recent reopens the fixture. Jobs exposes the failed records and a
   read-only **Check provider status** action. It queries the original persisted
   prediction ID, preserving both job identities and causing no generation.
   Unsupported cancel/resume/retry capabilities remain explicit; no universal
   collect-to-disk claim is made.
3. Actual chat form/planner/visible approval exercises local assembly success,
   a resolved `approve:error`, and an IPC-rejected input fingerprint. A local
   ffmpeg wrapper delegates normally; only the error fixture's sentinel makes
   that owned process exit 42. The IPC case mutates fixture production notes
   after card creation. Error cases create no output and cause no planner
   continuation. Consumed Approve controls remain absent after chat-tab rerender
   and native project reopen. IPC rejection is not a lost-response-after-success
   experiment; uncertainty is handled conservatively by retiring at dispatch.
4. A second displayed native quote is invalidated by a fixture ledger change
   from absent budget to a blank-limit budget with stale strict-policy flags.
   This does not impose or relax a numeric ceiling. Confirmation rejects it
   without a provider request. A fresh explicit UI action receives a new quote;
   native Escape declines it. Neither stale nor declined quote submits.

All application actions follow existing UI handlers. The sole direct bridge
query is read-only `host_build_info`; observational event listeners capture
quotes/menu delivery. The provider fixture rejects every route except explicit
models/generate/prediction routes; chat goes only to its separate explicit
loopback planner. No live chat redirect is enabled by the mock feature.

## Evidence and limitations

Retain `result.json`, actual provider/planner request logs, initial/failed/recovery/
final workflow state, approval lifecycle/quote JSON, screenshots, binary/source
identities, command exit statuses, and independent post-exit process/port/keyring
audits. Repeat from a fresh output directory, not by reusing a failed fixture.

Known existing behavior: spend dispatch errors do not automatically refresh the
desk. Reopen reloads authoritative failed state. Stale/declined spend warnings
leave pending local workflow rows; they are not evidence of a paid submission.
The final desk is therefore not claimed to have zero active rows. Read-only
recovery is status lookup only, not collection, cancellation, retry or resume.

The only product repair retires an approval marker synchronously at dispatch,
so rejected IPC cannot recreate a replay affordance. Its regression was first
observed failing in the real-entrypoint renderer harness, then passed alongside
the connected fingerprint-rejection case. Existing dirty baseline code/test
harnesses remain separate from the worker-owned commit.
