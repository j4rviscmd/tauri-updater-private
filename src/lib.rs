//! # tauri-updater-private
//!
//! Thin wrapper over the official [`tauri-plugin-updater`] that enables
//! in-app updates for Tauri 2 applications distributed from **private
//! GitHub repositories**.
//!
//! It configures the official updater plugin with an
//! `Authorization: Bearer <token>` header, embedded into the binary at
//! build time from the `UPDATER_GH_TOKEN` environment variable, plus an
//! `Accept: application/octet-stream` header for private-repo asset
//! downloads. The frontend keeps using the official
//! `@tauri-apps/plugin-updater` JS API unchanged.
//!
//! See `DESIGN.md` for the full design rationale.
//!
//! [`tauri-plugin-updater`]: https://github.com/tauri-apps/plugins-workspace/tree/dev/plugins/updater

/// Re-export so apps can chain further official-builder calls
/// (e.g. `pubkey`, `target`) with this crate as their only extra dependency.
pub use tauri_plugin_updater;

/// Errors returned when presetting the Authorization header fails.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// `UPDATER_GH_TOKEN` was absent or empty when the crate was compiled,
    /// and no explicit token was provided.
    #[error(
        "UPDATER_GH_TOKEN was not set (or was empty) at build time; set it in your release \
         environment, or pass a token explicitly via TauriUpdaterPrivateBuilder::token()"
    )]
    MissingToken,
    /// The preset Authorization header was rejected by the official updater
    /// builder (e.g. the token contains characters illegal in a header value).
    #[error(transparent)]
    Header(#[from] tauri_plugin_updater::Error),
}

/// Convenience alias used by this crate's public API.
pub type Result<T> = std::result::Result<T, Error>;

/// Returns the official updater [`tauri_plugin_updater::Builder`] with the
/// `Authorization: Bearer <token>` header preset from the compile-time
/// `UPDATER_GH_TOKEN`, plus `Accept: application/octet-stream`.
///
/// # Errors
///
/// Returns [`Error::MissingToken`] when the variable was absent or empty at
/// build time — a runtime error (not `compile_error!`) so builds without the
/// updater configured, e.g. local dev, still compile.
///
/// # Examples
///
/// ```no_run
/// # fn main() -> tauri_updater_private::Result<()> {
/// let builder = tauri_updater_private::updater_builder()?;
/// # Ok(())
/// # }
/// ```
pub fn updater_builder() -> Result<tauri_plugin_updater::Builder> {
    match option_env!("UPDATER_GH_TOKEN") {
        Some(token) if !token.is_empty() => TauriUpdaterPrivateBuilder::new()
            .token(token)
            .updater_builder(),
        _ => Err(Error::MissingToken),
    }
}

/// Builder for apps that supply the token explicitly (tests, non-CI builds)
/// instead of embedding it from the compile-time environment.
///
/// # Examples
///
/// ```no_run
/// # fn main() -> tauri_updater_private::Result<()> {
/// let builder = tauri_updater_private::TauriUpdaterPrivateBuilder::new()
///     .token("personal-access-token")
///     .updater_builder()?;
/// # Ok(())
/// # }
/// ```
#[derive(Default)]
pub struct TauriUpdaterPrivateBuilder {
    token: Option<String>,
}

// Why: manual Debug so debug-printing the builder never reveals the token
// (DESIGN.md §5: the crate must not expose the token), unlike a derived impl.
impl std::fmt::Debug for TauriUpdaterPrivateBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TauriUpdaterPrivateBuilder")
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl TauriUpdaterPrivateBuilder {
    /// Creates a builder with no token set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the token to embed into the `Authorization` header.
    /// An empty token is treated as missing and rejected at [`Self::updater_builder`].
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Returns the official updater [`tauri_plugin_updater::Builder`] with
    /// the `Authorization` and `Accept: application/octet-stream` headers
    /// preset.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingToken`] when no non-empty token was set, and
    /// [`Error::Header`] when the token is not a legal header value.
    pub fn updater_builder(self) -> Result<tauri_plugin_updater::Builder> {
        let token = self
            .token
            .filter(|token| !token.is_empty())
            .ok_or(Error::MissingToken)?;
        // Why: github.com/<owner>/<repo>/releases/download/... does NOT honor
        // Authorization on private repos (browser cookies only) — the request
        // 404s. The manifest must come from raw.githubusercontent.com (serves
        // the body regardless of Accept) and the package from the
        // api.github.com releases/assets endpoint, which only returns the
        // binary (302 to a signed URL) with this Accept header — with
        // application/vnd.github.raw it returns the asset metadata JSON
        // instead, which breaks signature verification.
        Ok(tauri_plugin_updater::Builder::default()
            .header("authorization", format!("Bearer {token}"))?
            .header("accept", "application/octet-stream")?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_without_token_is_missing_token_error() {
        assert!(matches!(
            TauriUpdaterPrivateBuilder::new().updater_builder(),
            Err(Error::MissingToken)
        ));
    }

    #[test]
    fn empty_token_is_missing_token_error() {
        assert!(matches!(
            TauriUpdaterPrivateBuilder::new()
                .token("")
                .updater_builder(),
            Err(Error::MissingToken)
        ));
    }

    #[test]
    fn valid_token_returns_builder() {
        assert!(TauriUpdaterPrivateBuilder::new()
            .token("ghp_test_token")
            .updater_builder()
            .is_ok());
    }

    #[test]
    fn invalid_header_token_is_header_error() {
        // '\n' is illegal in an HTTP header value: proves the token flows into
        // the preset Authorization header (the official Builder has no getter).
        assert!(matches!(
            TauriUpdaterPrivateBuilder::new()
                .token("bad\ntoken")
                .updater_builder(),
            Err(Error::Header(_))
        ));
    }

    #[test]
    fn debug_output_masks_token() {
        let builder = TauriUpdaterPrivateBuilder::new().token("ghp_secret");
        let debug = format!("{builder:?}");
        assert!(
            !debug.contains("ghp_secret"),
            "token leaked in Debug: {debug}"
        );
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn free_function_without_env_is_missing_token_error() {
        // The compile-time token is baked in; only assert when it was absent
        // or empty so this test passes in both build contexts.
        if option_env!("UPDATER_GH_TOKEN").map_or(true, |token| token.is_empty()) {
            assert!(matches!(updater_builder(), Err(Error::MissingToken)));
        }
    }
}
