# tauri-updater-private — Design Document

> Thin wrapper over the official `tauri-plugin-updater` that enables in-app updates for Tauri 2 applications distributed from **private GitHub repositories**.
>
> Status: v0.1.0 published to crates.io (2026-09-18); npm package unpublished and removed (2026-09-19).

## 1. Overview

`tauri-updater-private` is a thin Rust crate that configures the official [tauri-plugin-updater](https://github.com/tauri-apps/plugins-workspace/tree/dev/plugins/updater) with:

- an `Authorization: Bearer <token>` header, embedded into the binary at build time from an environment variable,
- plus an `Accept: application/octet-stream` header required for private-repo asset downloads (verified fact #3), and
- nothing else.

It registers the official updater plugin under its standard name, so the frontend uses the official `@tauri-apps/plugin-updater` JS API unchanged (`check()`, `download()`, `install()`, `downloadAndInstall()`).

Distribution: crates.io ([tauri-updater-private](https://crates.io/crates/tauri-updater-private)), public repo, English docs — same developer experience as other Tauri plugins. Releases are cut by release-please from conventional commits; the publish workflow ships to crates.io on each GitHub release. An npm re-export package existed at v0.1.0 and was **unpublished** (decision 2026-09-19): it added nothing — the frontend uses `@tauri-apps/plugin-updater` directly, and the cargo side needs the official crate as a direct dependency anyway (verified fact #6).

## 2. Problem

The official updater is designed for public release hosting: it fetches a `latest.json` manifest from a plain HTTPS URL and downloads the installer asset listed inside. For a **private** GitHub repository both requests return `404` unless an authenticated token is attached.

What is needed:

1. Attach `Authorization: Bearer <token>` to the manifest request **and** the asset download request.
2. Embed a long-lived token at build time (injected via GitHub Actions environment), so application code never handles the token.
3. Keep the official update flow, API surface, signature verification, and per-OS install logic untouched.

## 3. Verified facts (research, tauri-plugin-updater v2.11.0)

Findings from the official source (`plugins-workspace/plugins/updater`), which this design relies on:

1. **Headers set at plugin registration flow into both requests.** `tauri_plugin_updater::Builder::header()` stores headers in `UpdaterState` (`src/lib.rs:187-205`). `UpdaterExt::updater_builder()` seeds every `UpdaterBuilder` with those state headers (`src/lib.rs:88`). `Updater::check()` sends them on the manifest request and clones them onto the resulting `Update` (`src/updater.rs:598`); `Update::download()` sends the same headers on the asset GET (`src/updater.rs:680-743`).
2. **JS `check({ headers })` merges (per-key insert), but JS `download`/`downloadAndInstall` `({ headers })` REPLACES the header map entirely** (`src/commands.rs:109-115`, `178-184`). If the frontend passes headers at download time, the preset `Authorization` is lost. → Usage rule: the frontend never passes `headers`.
3. **Token-authenticated endpoints (corrected by E2E, 2026-09-18).** The original research premise — that `github.com/{owner}/{repo}/releases/latest/download/...` accepts `Authorization: Bearer` — **did not hold**: on private repos that host serves browser cookies only and returns `404` to both fine-grained and classic PATs. The verified working topology is:
   - **Manifest**: `https://raw.githubusercontent.com/{owner}/{repo}/main/latest.json` — Bearer honored, body served regardless of `Accept`.
   - **Package**: `https://api.github.com/repos/{owner}/{repo}/releases/assets/{asset-id}` with `Accept: application/octet-stream` — Bearer honored; only this Accept yields the binary (302 to a signed `objects.githubusercontent.com` URL, which needs no auth). The API ignores `application/vnd.github.raw` here and returns the asset *metadata JSON* with HTTP 200 instead, which then fails signature verification.

   Caveats: `raw.githubusercontent.com` caches ~5 min after each manifest push; the `releases/latest` API may briefly serve stale asset IDs right after `gh release create`.
4. **Actions' default `GITHUB_TOKEN` cannot be embedded**: it expires when the workflow job ends. The embedded token must be a long-lived fine-grained PAT stored as a repository/organization secret.
5. **Signature verification is independent of transport auth.** The downloaded installer is verified against the minisign `pubkey` from `tauri.conf.json` before any install step (`src/updater.rs:740`). A leaked or stolen token therefore cannot be used to push a malicious update to app users.
6. **Plugin ACL collection only sees direct dependencies (E2E-verified, 2026-09-18).** Tauri collects plugin permissions via cargo build-script metadata (`DEP_<links>_*` env vars in `tauri-utils/src/acl/build.rs::read_permissions()`), and cargo propagates that metadata to direct dependents only ("pass it to the immediate consuming crate"). An app depending on this crate alone cannot resolve `updater:default` in its capability — apps must list `tauri-plugin-updater` as a **direct** dependency alongside this crate. The wrapper cannot absorb this: the official crate owns the `links = "tauri-plugin-updater"` name, which cargo forbids duplicating, and a different links name would namespace the permissions wrong.

## 4. Architecture

### 4.1 Positioning

```
┌────────────────────────── app (e.g. private repo app) ──────────────────────────┐
│ tauri.conf.json: plugins.updater { endpoints, pubkey, windows.installMode }     │
│ src-tauri:      .plugin(tauri_updater_private::updater_builder()?.build())      │
│ frontend:       import { check, downloadAndInstall } from '@tauri-apps/plugin-updater' │
└──────────────────────────────────────────────────────────────────────────────────┘
                    │ registers "updater" plugin with preset Authorization header
                    ▼
        tauri-plugin-updater  ←── tauri-updater-private (this crate, 1 fn + 1 builder)
```

- **No fork.** The crate does not copy updater code; it depends on `tauri-plugin-updater` and returns its `Builder` pre-configured.
- **No own commands / permissions / ACL.** Commands remain `plugin:updater|*`; apps grant `updater:default` in capabilities exactly as with the official plugin.
- **No npm package (removed 2026-09-19).** The v0.1.0 re-export of `@tauri-apps/plugin-updater` added no value and was unpublished; the frontend imports the official npm package directly.

### 4.2 Token embedding

- Environment variable at compile time: **`UPDATER_GH_TOKEN`**.
- Read with `option_env!("UPDATER_GH_TOKEN")` inside the crate; absence — or an empty value — is a **runtime error with an explicit message** at `updater_builder()` call time (not `compile_error!` — builds without the updater configured, e.g. local dev, must still compile).
- Token requirements: fine-grained PAT, scope limited to the target repository(ies), permission **Contents: Read-only**, with an expiry set. Rotation replaces the secret and triggers a rebuild.

### 4.3 Public API

```rust
/// Returns the official updater Builder with the `Authorization: Bearer <token>`
/// and `Accept: application/octet-stream` headers preset from the
/// compile-time UPDATER_GH_TOKEN.
/// Err(MissingToken) if the variable was absent or empty at build time.
pub fn updater_builder() -> Result<tauri_plugin_updater::Builder, Error>;

/// Explicit token override (tests / non-CI builds). An empty token is
/// rejected as MissingToken. Debug output renders the token as <redacted>.
pub struct TauriUpdaterPrivateBuilder { /* token: Option<String> */ }
impl TauriUpdaterPrivateBuilder {
    pub fn new() -> Self;
    pub fn token(self, token: impl Into<String>) -> Self;
    pub fn updater_builder(self) -> Result<tauri_plugin_updater::Builder, Error>;
}

/// MissingToken | Header(#[from] tauri_plugin_updater::Error)
pub enum Error;
pub type Result<T> = std::result::Result<T, Error>;

/// Re-export so apps can chain official builder calls (pubkey, target, …)
/// with this crate as their only extra dependency.
pub use tauri_plugin_updater;
```

### 4.4 App integration

```jsonc
// tauri.conf.json
{
  "plugins": {
    "updater": {
      "pubkey": "<minisign public key>",
      "endpoints": [
        "https://raw.githubusercontent.com/{owner}/{repo}/main/latest.json"
      ]
    }
  }
}
```

Topology notes (verified fact #3):

- `latest.json` lives **on the default branch** (fetched via raw.githubusercontent.com), not as a release asset.
- Its `platforms.*.url` must point at the **asset API** (`https://api.github.com/repos/{owner}/{repo}/releases/assets/{asset-id}`) — the crate presets the required `Accept: application/octet-stream`. The asset ID changes on every release, so the release flow must update `latest.json` on the default branch after creating each release.

```rust
// src-tauri/src/lib.rs
.builder(|b| b.plugin(tauri_updater_private::updater_builder()?.build()))
```

```ts
// frontend — official API, no options change
import { check } from '@tauri-apps/plugin-updater';
const update = await check();           // Authorization header applied automatically
await update?.downloadAndInstall();     // …also on the asset download
```

Release workflow (app repo):

```yaml
release:
  permissions:
    contents: write
  steps:
    - uses: tauri-apps/tauri-action@v0
      env:
        TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.UPDATER_SIGNING_KEY }}  # minisign key (release-time)
        UPDATER_GH_TOKEN: ${{ secrets.UPDATER_GH_TOKEN }}              # fine-grained PAT (embedded)
```

Token timeline (two distinct credentials — do not conflate):

| Phase | Credential | Lifetime | Purpose |
|---|---|---|---|
| Build & release (Actions) | `GITHUB_TOKEN` | job | create release, upload assets + latest.json (tauri-action) |
| Runtime (end-user machine) | fine-grained PAT, embedded | until rotated | fetch latest.json + asset with `Authorization` |

## 5. Security design

- **Threat: token extraction from the distributed binary.** `strings` on the binary reveals the PAT. Contents:Read on the app repo means the attacker can read that repository's source. Accepted for v1 (decision 2026-09-18). Mitigations: read-only scope, single-repo scope, expiry + rotation procedure; minisign signature verification prevents weaponizing a stolen token against app users.
- **Threat: malicious update injection.** Blocked by signature verification (`pubkey` pinned in `tauri.conf.json`; `TAURI_SIGNING_PRIVATE_KEY` never leaves CI secrets).
- **Threat: token in logs.** The crate never logs the token, and `TauriUpdaterPrivateBuilder`'s Debug impl redacts it. Actions masks secrets it knows about; docs warn against `printenv` in workflows.
- **Upgrade path (documented, not v1):** move releases to a dedicated private `*-updates` repo containing only `latest.json` + assets, so a leaked token exposes nothing but artifacts the attacker already has. Requires a cross-repo release upload PAT in the workflow.

## 6. Constraints & gotchas

- Frontend **must not** pass `headers` to `check`/`download`/`downloadAndInstall` — download-side headers replace the preset map and silently drop `Authorization` (verified fact #2). Documented prominently.
- Apps must depend on `tauri-plugin-updater` **directly** in addition to this crate, or capability resolution fails (verified fact #6).
- `github.com/.../releases/latest/download/...` URLs do not work on private repos (verified fact #3, E2E-corrected) — use raw.githubusercontent.com for the manifest and the asset API for packages.
- `raw.githubusercontent.com` caches the manifest ~5 min after each push; the `releases/latest` API may briefly serve stale asset IDs right after `gh release create` — a freshly published update may take a few minutes to become visible.
- Desktop only (inherited from the official updater; mobile install is a no-op).
- HTTPS-only endpoints enforced by the official config validation.
- `timeout` given to JS `check()` applies to the manifest request only; pass `timeout` to `downloadAndInstall` for the download (official behavior, inherited).

## 7. Decisions (2026-09-18)

1. Thin wrapper crate over official `tauri-plugin-updater`; no fork, no own commands. *(rationale: verified facts #1/#3 — a preset header is sufficient)*
2. Token: fine-grained PAT (Contents: Read-only), app repo direct — releases stay in the app repository. Dedicated updates-repo separation documented as future hardening only.
3. Repo public; distribution via **crates.io only** as `tauri-updater-private`. *(revised 2026-09-19: the npm re-export package published at v0.1.0 was unpublished — it carried no functionality, and the "one name on both registries" convention lost its meaning once apps had to depend on the official crates directly anyway, see verified fact #6)*
4. Repo language: English (`.language`), docs in English.
5. Env var name: `UPDATER_GH_TOKEN`.
6. Releases: release-please (rust strategy) from conventional commits; PAT-backed because GITHUB_TOKEN cannot open mergeable PRs under this repo's branch protection; registry publishing on release published. Versioning keeps release-please defaults (a breaking change during 0.x bumps to 1.0.0).

## 8. Open TODOs

- TODO: E2E matrix — verify the raw.githubusercontent + asset-API topology on Windows/Linux. Verified on macOS (darwin-aarch64) only; the transport layer is OS-independent reqwest and install logic is the official plugin's, so residual risk is low.
