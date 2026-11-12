# Studio: one synchronized source tree, two Docker hosts

This environment uses the **existing laptop-authoritative Syncthing tree** on
both hosts. It never seeds, exports, or copies application source into a Docker
volume. `/source` is one writable bind of Content_Creation; `/runtime` and
`Desktop_App/node_modules` are host-local dependency/build caches only. No host
app dependencies, provider credentials, Docker socket, or operator home are
mounted. Existing dirty app files are not reset or rewritten by these scripts.
Normal builds write the existing generated `Desktop_App/dist` output.

## Daily commands (server and laptop)

```bash
cd /home/kris/Content_Creation/Studio/dev-container
./build
./prepare
./studio exec workbench bash
# Work inside /source/Studio/Desktop_App; editors on the host see the same source.
./studio exec -T workbench npm run build
./studio exec -T workbench cargo build --locked --manifest-path host/Cargo.toml
./studio exec -T workbench node --test electron/hostBinary.test.mjs
```

`./build` exercises Docker's deprecated **legacy builder** with 2 CPUs and 8 GiB
limits, on the tested server engine and laptop Docker 29.1.3. It fails instead of
silently switching to an unbounded builder if an engine removes that interface.
Default Compose/BuildKit builds do not inherit runtime resource limits: use
`./build`, then `./studio up -d --no-build workbench`. The image is amd64, based
on the pinned native Hermes desktop image; Node 26.12.0 and Rust 1.97.1 match the
existing donor. There is no vendored OpenCode binary or private auth/config.

`./prepare` installs locked npm dependencies, explicitly downloads Electron's
lazy executable while online, builds the frontend, and builds the debug Rust
host. Networked dependency installation is confined to the workbench. UI
containers use `network_mode: none`; automatic local catalog discovery can
therefore log a refused loopback Ollama request. No provider is configured or
called by the smoke harness. The environment does **not** implement or test
live inference/provider integration.

## UI

Headless UI and actual-app smoke (server or laptop):

```bash
./studio --profile ui run --rm --no-deps ui desktop \
  node /source/Studio/dev-container/ui-smoke.mjs
# PNG, UI text, logs, and process sandbox evidence persist in local runtime cache:
docker cp studio-dev-workbench-1:/runtime/ui-smoke \
  "$HOME/.local/state/studio-workflow-repair/server-ui"
```

Interactive laptop UI, from the graphical desktop terminal:

```bash
./native-display
# Or exercise the same smoke against the actual native display:
./native-display node /source/Studio/dev-container/ui-smoke.mjs
```

The helper shares the native X11 socket **read-only**, with a private mode-0600
copy of only the selected display's xauth cookie. It removes that temporary
cookie on exit. No `xhost +`, host D-Bus, host keyring, UI ports, host networking,
or `--no-sandbox` is used. X11 is still a display trust boundary: authorize only
trusted app code to access your desktop. App settings/home are disposable tmpfs;
there are deliberately no personal/provider credentials in this UI session.
Xvfb has no TCP listener on headless hosts. `run-ui.sh` starts Vite and Electron
in the same offline container, with local stdio Rust sidecar communication.

The smoke reuses the app's maintained `tests/ui-tracer.mjs` CDP/until helpers,
opens the real Electron app, checks visible controls and the preload bridge,
captures the rendered page, and asserts a renderer has zero effective
capabilities and extra seccomp filters. It does not open projects or submit
work. Native display smoke was exercised on the laptop's active Xwayland `:0`.

## Real interactive/provider testing (explicit opt-in)

The default `./native-display` above is **an offline disposable fixture**, not a
usable provider-enabled dev session. It cannot preserve API credentials across
container removal and is not evidence that live generation works.

From the laptop graphical terminal, deliberately choose:

```bash
cd /home/kris/Content_Creation/Studio/dev-container
./native-display --interactive-providers
```

No rebuild or dependency install is needed for this launcher repair. It uses
ordinary Docker bridge networking (outbound access), not host networking. This
mode permits provider calls made by Studio/user actions; the launcher itself
never fetches a provider catalog or makes inference requests. Studio may probe
container-local Ollama during startup; Docker `localhost` is not the host.
Existing provider approval/spending rules are unchanged.

**First launch:** a native GNOME prompt says **Choose password for new keyring**,
for **Studio Docker**. Type a new **non-empty** keyring password and confirm it,
then click **Continue**. Type this password only into that native prompt, never
chat, shell arguments, an environment variable, or an agent tool. This is a
container-only keyring password, not your API key or host login password. On
later launches, unlock it in the native prompt. Cancellation/failure prevents
Studio from starting; unencrypted or session-only keyrings are refused. There
is no blank-password default and no plaintext API-key fallback.

Then enter the API key directly in Studio Settings and click **Save**. **Reload**
should show **Saved on this computer — leave blank to keep it** with a blank
key field. Fetching models/generating scripts is a separate deliberate user
action; no real provider was called by the repair verification.

Settings/Electron home and the native encrypted keyring persist under the
machine-local, owner-only directory:

```
~/.local/share/studio-docker/interactive/
```

It is not inside the synchronized source tree, not the host keyring, and is
never mounted in offline fixture mode. A process lock prevents two interactive
launches sharing it. Settings metadata is plaintext (keys stripped by Studio);
keyring secrets are encrypted by GNOME Keyring. Both the keyring and settings
must be retained together. Do not synchronize or share this directory. Keep
its password safe: there is no agent-held recovery password. `compose down
--volumes` does not delete this bind-mounted directory. Resetting it is a
separate destructive user decision and loses stored settings/keys.

An already-open old offline window is **not upgraded**. Do not stop it while it
has unsaved work: close it yourself when ready, then use the explicit command
above and re-enter the key directly in the new window. Do not copy private
host credentials or mount host D-Bus to repair it.

Implementation reuses installed libsecret/Secret Service `CreateCollection`
with alias `default`, native GCR password/unlock prompts, and a private session
bus with the correct container HOME/display activation environment. The default
Rust keyring store expects that alias; it does not create it automatically.
`compose.interactive.yaml` alone changes networking/persistence; the default
Compose `ui` remains `network_mode: none` and disposable.

## Identity: rootless server versus rootful laptop

`./studio` discovers daemon rootless mode on each invocation. Workbench UID/GID
is `0:0` under rootless Docker (maps to host kris, server UID 1001), otherwise
the invoking user's real UID/GID (laptop 1000:1000). Override only if necessary:

```bash
STUDIO_SOURCE=/actual/synchronized/Content_Creation ./studio up -d --no-build workbench
STUDIO_PROJECT=studio-other ./studio up -d --no-build workbench
# Explicit identity overrides: STUDIO_UID and STUDIO_GID.
```

The source path must exist; Compose may not silently create a replacement.
Do not chown the tree. Do not mount a source-code volume. The server source is
SSHFS without `allow_other`, so plain container UID 1000 cannot even read it.
Chromium also refuses namespace-root. `compose.rootless.yaml`, selected
automatically by the wrapper, resolves both constraints with one **UI-only
SETFCAP bootstrap capability** and `unshare --user --map-user=1000
--map-group=1000`. Linux >=5.12 requires SETFCAP to map parent UID 0. After
mapping, the UI runs as nested UID 1000 with zero effective/permitted
capabilities while still mapping to the host kris who owns FUSE. Source writes
were tested and remained host-owned 1001:0. This is a narrow explicit exception
to `cap_drop: ALL`, not `privileged`, SYS_ADMIN, or unconfined seccomp. The laptop
UI runs directly as UID 1000 with all capabilities dropped. No FUSE permission
policy, daemon setting, or unrelated service is changed.

All services retain read-only images, no-new-privileges, the donor Chromium
namespace seccomp profile, 2 CPU / 8 GiB / 512 PID limits, bounded executable
nosuid/nodev tmpfs, and no published ports. Runtime volumes are task-local and
writable by the mapped developer identities; they contain code dependencies,
compiler artifacts, and synthetic smoke outputs, not synchronized source.

## Stop / reset

```bash
./studio --profile ui down          # only this named project, retain caches
# Explicit destructive cache reset (NEVER source):
./studio --profile ui down --volumes
```

Do not stop the old `studio-server-workflows` project while adopting this one.
No scripts commit/push or alter Syncthing configuration.

## Reuse receipt

Reused the inspected `/home/kris/Development/studio-coding-environment` pinned
Dockerfile, native Compose hardening, Xvfb/DBus session pattern and namespace
seccomp profile; removed host-specific cgroup build assertions and unnecessary
private OpenCode vendoring. Adaptation: writable source bind, local dependency
caches, daemon-specific identity, offline UI, native scoped xauth, bounded build
wrapper. Public reuse references supplied by the parent:

- https://docs.docker.com/engine/storage/bind-mounts/
- https://github.com/docker/compose/pull/12733
- https://docs.docker.com/compose/how-tos/file-watch.md

Bind mounts are the selected native sharing mechanism, not Compose Watch's
second synchronization/copy layer.
