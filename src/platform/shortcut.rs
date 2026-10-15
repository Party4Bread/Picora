use anyhow::{Context, Result};
use global_hotkey::{GlobalHotKeyManager, hotkey::HotKey};
use std::str::FromStr;
pub struct Shortcut {
    manager: GlobalHotKeyManager,
    key: HotKey,
}
impl Shortcut {
    pub fn new(text: &str) -> Result<Self> {
        let key = HotKey::from_str(text).context("Use a shortcut such as Ctrl+Shift+Space")?;
        let manager = GlobalHotKeyManager::new()?;
        manager
            .register(key)
            .context("Shortcut unavailable; choose a different combination")?;
        Ok(Self { manager, key })
    }
    pub fn replace(&mut self, text: &str) -> Result<()> {
        let key = HotKey::from_str(text)?;
        if key == self.key {
            return Ok(());
        }
        self.manager
            .register(key)
            .context("Shortcut is already in use")?;
        self.manager.unregister(self.key)?;
        self.key = key;
        Ok(())
    }
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
