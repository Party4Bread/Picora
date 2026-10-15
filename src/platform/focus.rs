use anyhow::{Result, bail};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
#[derive(Debug, Clone, Copy)]
pub struct Target(pub u64);
pub fn capture() -> Option<Target> {
    if super::is_wayland() {
        return Some(Target(0));
    }
    #[cfg(target_os = "windows")]
    {
        let hwnd = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        if !hwnd.is_null() {
            return Some(Target(hwnd as u64));
        }
    }
    #[cfg(target_os = "macos")]
    {
        let app = objc2_app_kit::NSWorkspace::sharedWorkspace().frontmostApplication()?;
        Some(Target(app.processIdentifier() as u64))
    }
    #[cfg(target_os = "linux")]
    {
        use x11rb::protocol::xproto::ConnectionExt;
        let (conn, _) = x11rb::connect(None).ok()?;
        let focus = conn.get_input_focus().ok()?.reply().ok()?.focus;
        if focus > 1 {
            return Some(Target(focus as u64));
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}
/// Capture the recipient, excluding Picora's own library and picker windows.
pub fn capture_external() -> Option<Target> {
    let target = capture()?;
    #[cfg(target_os = "windows")]
    {
        let mut pid = 0;
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                target.0 as *mut _,
                &mut pid,
            );
        }
        if pid == std::process::id() {
            return None;
        }
    }
    #[cfg(target_os = "macos")]
    if target.0 == std::process::id() as u64 {
        return None;
    }
    #[cfg(target_os = "linux")]
    if !super::is_wayland() {
        use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
        let (conn, _) = x11rb::connect(None).ok()?;
        let atom = conn
            .intern_atom(false, b"_NET_WM_PID")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let mut window = target.0 as u32;
        for _ in 0..16 {
            let reply = conn
                .get_property(false, window, atom, AtomEnum::CARDINAL, 0, 1)
                .ok()?
                .reply()
                .ok()?;
            if reply.value32().and_then(|mut v| v.next()) == Some(std::process::id()) {
                return None;
            }
            let tree = conn.query_tree(window).ok()?.reply().ok()?;
            if tree.parent == tree.root || tree.parent == 0 {
                break;
            }
            window = tree.parent;
        }
    }
    Some(target)
}
pub fn restore(target: Target) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let ok = unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(target.0 as *mut _)
        };
        if ok == 0 {
            bail!("Windows did not restore the previous window");
        }
    }
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(target.0 as i32)
            .ok_or_else(|| anyhow::anyhow!("Previous app closed"))?;
        if !app.activateWithOptions(NSApplicationActivationOptions::empty()) {
            bail!("macOS did not activate the previous app");
        }
    }
    #[cfg(target_os = "linux")]
    {
        use x11rb::{
            connection::Connection,
            protocol::xproto::{ConnectionExt, InputFocus},
        };
        if super::is_wayland() {
            return Ok(());
        }
        let (conn, _) = x11rb::connect(None)?;
        conn.set_input_focus(InputFocus::PARENT, target.0 as u32, x11rb::CURRENT_TIME)?
            .check()?;
        conn.flush()?;
    }
    Ok(())
}
pub fn may_restore(current: Option<u64>, target: u64, picker: Option<u64>) -> bool {
    current.is_none() || current == Some(target) || current == picker
}
pub fn paste(target: Target, picker: Option<Target>, ticket: super::paste::Ticket) -> Result<()> {
    anyhow::ensure!(
        ticket.valid() && may_restore(capture().map(|t| t.0), target.0, picker.map(|t| t.0)),
        "Focus changed; paste manually"
    );
    restore(target)?;
    std::thread::sleep(std::time::Duration::from_millis(120));
    if !ticket.valid() || capture().map(|t| t.0) != Some(target.0) {
        bail!("Focus changed; paste manually");
    }
    let mut input = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("Input permission required: {e}"))?;
    let modifier = if cfg!(target_os = "macos") {
        Key::Meta
    } else {
        Key::Control
    };
    input.key(modifier, Direction::Press)?;
    let result = input.key(Key::Unicode('v'), Direction::Click);
    let release = input.key(modifier, Direction::Release);
    result?;
    release?;
    Ok(())
}
