# tauri-updater-private

[![CI](https://github.com/j4rviscmd/tauri-updater-private/actions/workflows/ci.yml/badge.svg)](https://github.com/j4rviscmd/tauri-updater-private/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/tauri-updater-private.svg)](https://crates.io/crates/tauri-updater-private)
[![npm](https://img.shields.io/npm/v/tauri-updater-private.svg)](https://www.npmjs.com/package/tauri-updater-private)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

In-app updates for [Tauri 2](https://v2.tauri.app/) applications distributed from **private GitHub repositories**.

Thin wrapper over the official [`tauri-plugin-updater`](https://v2.tauri.app/plugin/updater/) that attaches an `Authorization: Bearer <token>` header — embedded into the binary at build time — to both the update-manifest request and the installer download. Nothing else. No fork, no extra commands: the frontend keeps using the official `@tauri-apps/plugin-updater` JS API unchanged.

## Why

The official updater fetches a `latest.json` manifest and the installer asset over plain HTTPS. For a private GitHub repository both requests return `404` unless an authenticated token is attached. This crate presets that header on the official plugin so the standard update flow works against private releases.

Update authenticity is unaffected: installers are still verified against the minisign `pubkey` pinned in `tauri.conf.json`, independent of transport auth.

## Install

Rust (`src-tauri/Cargo.toml`):

```toml
[dependencies]
tauri-updater-private = "0.1"
```

npm (pure re-export of the official package):

```sh
npm i tauri-updater-private
```

## Usage

1. Create a fine-grained PAT: scope it to the target repository only, permission **Contents: Read-only**, with an expiry. Store it as a repository/organization secret `UPDATER_GH_TOKEN`.
2. Point the updater endpoints at the GitHub release:

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

> **Endpoint topology (verified E2E on a private repo):** `github.com/.../releases/latest/download/...` ignores `Authorization` on private repos (404). Use `raw.githubusercontent.com` for the manifest — so `latest.json` is committed to the default branch — and point its `platforms.*.url` at the asset API (`https://api.github.com/repos/{owner}/{repo}/releases/assets/{asset-id}`). This crate presets the required `Accept: application/octet-stream` alongside the `Authorization` header; without it the API returns asset metadata JSON instead of the binary. Note the asset ID changes every release, and raw.githubusercontent.com caches the manifest for ~5 minutes after each push.

3. Register the plugin via this crate:

```rust
// src-tauri/src/lib.rs
.builder(|b| b.plugin(tauri_updater_private::updater_builder()?.build()))
```

For local dev builds without the env var (which fail with `MissingToken`), pass a token explicitly:

```rust
use tauri_updater_private::TauriUpdaterPrivateBuilder;
.plugin(TauriUpdaterPrivateBuilder::new().token("personal-access-token").updater_builder()?.build())
```

4. Keep using the official JS API:

```ts
import { check } from 'tauri-updater-private';
const update = await check();        // Authorization header applied automatically
await update?.downloadAndInstall();  // ...also on the asset download
```

5. Export `UPDATER_GH_TOKEN` in your release workflow (GitHub Actions):

```yaml
- uses: tauri-apps/tauri-action@v0
  env:
    TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.UPDATER_SIGNING_KEY }}
    UPDATER_GH_TOKEN: ${{ secrets.UPDATER_GH_TOKEN }}
```

> The Actions-default `GITHUB_TOKEN` cannot be embedded — it expires when the job ends. Use a long-lived fine-grained PAT and rotate it.

## Important: never pass `headers` from the frontend

The JS `download`/`downloadAndInstall` options **replace** the header map entirely when `headers` is passed, silently dropping the preset `Authorization`. Call `check()`, `download()`, `downloadAndInstall()` without a `headers` option.

## Security notes

- The embedded token can be extracted from the distributed binary (`strings`). This is accepted for v1: the token is read-only, scoped to the app repository, and expirable. Signature verification prevents a stolen token from pushing a malicious update to app users.
- Rotate the token by replacing the secret and rebuilding.
- A dedicated private `*-updates` repo (artifacts only) is documented as a future hardening path — see `DESIGN.md`.

## License

MIT.
