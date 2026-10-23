use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::fmt;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Dcinside,
    Arca,
}
impl Provider {
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Dcinside => "dcinside",
            Self::Arca => "arca",
        }
    }
    #[must_use]
    pub fn origin(self) -> &'static str {
        match self {
            Self::Dcinside => "https://dccon.dcinside.com/",
            Self::Arca => "https://arca.live/",
        }
    }
}
impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Dcinside => "DCInside",
            Self::Arca => "arca.live",
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackRef {
    pub provider: Provider,
    pub id: String,
}
impl PackRef {
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}:{}", self.provider.id(), self.id)
    }
    #[must_use]
    pub fn url(&self) -> String {
        match self.provider {
            Provider::Dcinside => format!("https://dccon.dcinside.com/#{}", self.id),
            Provider::Arca => format!("https://arca.live/e/{}", self.id),
        }
    }
    /// # Errors
    /// Rejects URLs outside supported sticker repositories or without a valid pack identifier.
    pub fn parse(input: &str) -> Result<Self> {
        ensure!(
            !input.contains("..") && !input.to_lowercase().contains("%2e"),
            "Invalid pack URL"
        );
        let url = Url::parse(input.trim())?;
        ensure!(
            url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none(),
            "Use an HTTPS pack URL without credentials or a port"
        );
        let (provider, id) = match url.host_str() {
            Some("dccon.dcinside.com") => (Provider::Dcinside, url.fragment().unwrap_or_default()),
            Some("arca.live") => (
                Provider::Arca,
                url.path()
                    .strip_prefix("/e/")
                    .unwrap_or_default()
                    .trim_end_matches('/'),
            ),
            _ => bail!("Use a DCInside or arca.live sticker pack URL"),
        };
        ensure!(
            !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit()),
            "The URL must identify one sticker pack"
        );
        Ok(Self {
            provider,
            id: id.into(),
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sticker {
    pub id: String,
    pub label: String,
    pub url: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pack {
    pub reference: PackRef,
    pub title: String,
    pub author: String,
    pub url: String,
    pub thumbnail: String,
    pub stickers: Vec<Sticker>,
}
#[derive(Debug, Clone)]
pub struct SavedPack {
    pub pack: Pack,
    pub favorite: bool,
}
#[derive(Debug, Clone)]
pub struct LibrarySticker {
    pub pack_key: String,
    pub pack_title: String,
    pub provider: Provider,
    pub sticker: Sticker,
}
#[derive(Debug, Clone)]
pub struct Group {
    pub id: i64,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub check_updates_automatically: bool,
    pub update_channel: crate::update::release::Channel,
    pub shortcut: String,
    pub auto_paste: bool,
    pub launch_at_login: bool,
    #[serde(with = "theme_setting")]
    pub theme: eframe::egui::ThemePreference,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            check_updates_automatically: true,
            update_channel: crate::update::release::Channel::Stable,
            shortcut: "Ctrl+Shift+Space".into(),
            auto_paste: true,
            launch_at_login: false,
            theme: eframe::egui::ThemePreference::System,
        }
    }
}

mod theme_setting {
    use eframe::egui::ThemePreference;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        theme: &ThemePreference,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match theme {
            ThemePreference::System => "System",
            ThemePreference::Light => "Light",
            ThemePreference::Dark => "Dark",
        })
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ThemePreference, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "Light" => ThemePreference::Light,
            "Dark" => ThemePreference::Dark,
            _ => ThemePreference::System,
        })
    }
}
