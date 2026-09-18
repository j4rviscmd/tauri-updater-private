//! # tauri-updater-private
//!
//! Thin wrapper over the official [`tauri-plugin-updater`] that enables
//! in-app updates for Tauri 2 applications distributed from **private
//! GitHub repositories**.
//!
//! It configures the official updater plugin with an
//! `Authorization: Bearer <token>` header, embedded into the binary at
//! build time from the `UPDATER_GH_TOKEN` environment variable, and
//! nothing else. The frontend keeps using the official
//! `@tauri-apps/plugin-updater` JS API unchanged.
//!
//! See `DESIGN.md` for the full design rationale.
//!
//! [`tauri-plugin-updater`]: https://github.com/tauri-apps/plugins-workspace/tree/dev/plugins/updater

// Phase 1 (DESIGN.md §7): implement `updater_builder()` with the
// compile-time token embed, the token override, and unit tests.
