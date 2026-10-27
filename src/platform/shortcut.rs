use anyhow::{Context, Result};
use global_hotkey::{GlobalHotKeyManager, hotkey::HotKey};
use rust_i18n::t;
use std::str::FromStr;
pub struct Shortcut {
    manager: GlobalHotKeyManager,
    key: HotKey,
}
impl Shortcut {
    /// # Errors
    /// Rejects invalid key combinations and reports platform initialization or hotkey registration failures.
    pub fn new(text: &str) -> Result<Self> {
        let key = HotKey::from_str(text)
            .with_context(|| t!("Use a shortcut such as Ctrl+Shift+Space").into_owned())?;
        let manager = GlobalHotKeyManager::new()?;
        manager.register(key).with_context(|| {
            t!("Shortcut unavailable; choose a different combination").into_owned()
        })?;
        Ok(Self { manager, key })
    }
    /// # Errors
    /// Rejects invalid or unavailable key combinations and reports failures unregistering the previous hotkey.
    pub fn replace(&mut self, text: &str) -> Result<()> {
        let key = HotKey::from_str(text)?;
        if key == self.key {
            return Ok(());
        }
        self.manager
            .register(key)
            .with_context(|| t!("Shortcut is already in use").into_owned())?;
        self.manager.unregister(self.key)?;
        self.key = key;
        Ok(())
    }
    #[must_use]
    pub fn triggered(&self) -> bool {
        let mut triggered = false;
        while let Ok(e) = global_hotkey::GlobalHotKeyEvent::receiver().try_recv() {
            if e.id == self.key.id() && e.state == global_hotkey::HotKeyState::Released {
                triggered = true;
            }
        }
        triggered
    }
}
