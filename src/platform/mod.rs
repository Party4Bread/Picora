pub mod autostart;
pub mod clipboard;
pub mod focus;
pub mod instance;
pub mod paste;
pub mod shortcut;
#[cfg(target_os = "linux")]
pub mod wayland;
pub fn is_wayland() -> bool {
    cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some()
}
