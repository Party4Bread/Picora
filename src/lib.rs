//! Picora desktop sticker library, repository clients, bounded media cache, and platform integration.
pub mod app;
pub mod auth;
pub mod i18n;
pub mod media;
pub mod model;
pub mod platform;
pub mod providers;
mod services;
pub mod store;
pub mod update;

rust_i18n::i18n!("locales", fallback = "en");
