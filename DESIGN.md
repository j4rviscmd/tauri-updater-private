# tauri-updater-private — Design Document

> Thin wrapper over the official `tauri-plugin-updater` that enables in-app updates for Tauri 2 applications distributed from **private GitHub repositories**.
>
> Status: design phase (2026-09-18). No implementation yet.

## 1. Overview

`tauri-updater-private` is a thin Rust crate (+ npm package) that configures the official [tauri-plugin-updater](https://github.com/tauri-apps/plugins-workspace/tree/dev/plugins/updater) with:

- an `Authorization: Bearer <token>` header, embedded into the binary at build time from an environment variable, and
- nothing else.

It registers the official updater plugin under its standard name, so the frontend uses the official `@tauri-apps/plugin-updater` JS API unchanged (`check()`, `download()`, `install()`, `downloadAndInstall()`).

Distribution (planned): crates.io (`tauri-updater-private`) and npm (`tauri-updater-private`), public repo, English docs — same developer experience as other Tauri plugins.

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
        tauri-plugin-updater  ←── tauri-updater-private (this crate, ~1 public fn)
```

- **No fork.** The crate does not copy updater code; it depends on `tauri-plugin-updater` and returns its `Builder` pre-configured.
- **No own commands / permissions / ACL.** Commands remain `plugin:updater|*`; apps grant `updater:default` in capabilities exactly as with the official plugin.
- **npm package = pure re-export** of `@tauri-apps/plugin-updater`, so the install convention `npm i tauri-updater-private` works and private-specific helpers can be added later.

### 4.2 Token embedding

- Environment variable at compile time: **`UPDATER_GH_TOKEN`**.
- Read with `option_env!("UPDATER_GH_TOKEN")` inside the crate; absence is a **runtime error with an explicit message** at `updater_builder()` call time (not `compile_error!` — builds without the updater configured, e.g. local dev, must still compile).
- Token requirements: fine-grained PAT, scope limited to the target repository(ies), permission **Contents: Read-only**, with an expiry set. Rotation replaces the secret and triggers a rebuild.

### 4.3 API sketch (Rust)

```rust
/// Returns the official updater Builder with the Authorization header
/// preset from the compile-time UPDATER_GH_TOKEN.
/// Err if the token was not present at build time.
pub fn updater_builder() -> Result<tauri_plugin_updater::Builder, Error>;

/// Explicit token override (tests / non-CI builds).
pub struct TauriUpdaterPrivateBuilder { /* token: Option<String> */ }
impl TauriUpdaterPrivateBuilder {
    pub fn new() -> Self;
    pub fn token(self, t: impl Into<String>) -> Self;
    pub fn updater_builder(self) -> Result<tauri_plugin_updater::Builder, Error>;
}
```

Minimal surface: the free function covers the CI path; the small builder exists only for the token override. v1 ships both if the builder stays under ~50 lines, otherwise only the free function plus `updater_builder_with_token(token)`.

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
.builder(|b| b.plugin(tauri_updater_private::updater_builder().unwrap().build()))
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
- **Threat: token in logs.** The crate never logs the token. Actions masks secrets it knows about; docs warn against `printenv` in workflows.
- **Upgrade path (documented, not v1):** move releases to a dedicated private `*-updates` repo containing only `latest.json` + assets, so a leaked token exposes nothing but artifacts the attacker already has. Requires a cross-repo release upload PAT in the workflow.

## 6. Constraints & gotchas

- Frontend **must not** pass `headers` to `check`/`download`/`downloadAndInstall` — download-side headers replace the preset map and silently drop `Authorization` (verified fact #2). Documented prominently; a future npm minor may narrow the re-exported types to omit `headers`.
- `github.com/.../releases/latest/download/...` URLs do not work on private repos (verified fact #3, E2E-corrected) — use raw.githubusercontent.com for the manifest and the asset API for packages.
- `raw.githubusercontent.com` caches the manifest ~5 min after each push; the `releases/latest` API may briefly serve stale asset IDs right after `gh release create` — a freshly published update may take a few minutes to become visible.
- Desktop only (inherited from the official updater; mobile install is a no-op).
- HTTPS-only endpoints enforced by the official config validation.
- `timeout` given to JS `check()` applies to the manifest request only; pass `timeout` to `downloadAndInstall` for the download (official behavior, inherited).

## 7. Development plan

| Phase | Scope | Done when |
|---|---|---|
| 0. Scaffold | cargo crate + npm package layout, LICENSE (MIT), README (en), CI (fmt/clippy/test, npm build) | CI green on main |
| 1. Rust core | `updater_builder()` + token embed + unit tests (header preset, missing-token error, override) | `cargo test` green; test app compiles with env set |
| 2. npm package | re-export of `@tauri-apps/plugin-updater`, types, build via rollup or tsup | `npm pack` dry-run clean |
| 3. E2E | example Tauri app + private repo release; manual check→download→install on macOS (Windows/Linux as available) | update applied end-to-end |
| 4. Publish | crates.io + npm publish, README usage docs, security notes | installable from both registries |

Phase status (2026-09-18): Phase 3 **done** — verified end-to-end against a real private-repo Tauri 2 app on macOS (darwin-aarch64): check → downloadAndInstall → relaunch, v0.1.0 → v0.1.1, signature verification and version gating working. Two premises were corrected along the way (verified fact #3 rewrite, §4.4 topology notes); the crate now presets `Accept: application/octet-stream` in addition to `Authorization`.

## 8. Decisions (2026-09-18)

1. Thin wrapper crate over official `tauri-plugin-updater`; no fork, no own commands. *(rationale: verified facts #1/#3 — a preset header is sufficient)*
2. Token: fine-grained PAT (Contents: Read-only), app repo direct — releases stay in the app repository. Dedicated updates-repo separation documented as future hardening only.
3. Repo public; distribution via crates.io **and** npm under the name `tauri-updater-private` (same name on both registries).
4. Repo language: English (`.language`), docs in English.
5. Env var name: `UPDATER_GH_TOKEN`.

## 9. Open TODOs

- TODO: confirm crate name `tauri-updater-private` availability on crates.io at publish time (fallback: `tauri-plugin-updater-private`).
- TODO: E2E matrix — verify the raw.githubusercontent + asset-API topology on Windows/Linux (verified on macOS darwin-aarch64 only).
