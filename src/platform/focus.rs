use anyhow::{Result, bail};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
/// Native recipient identity. Wayland uses `Target(0)` as a sentinel because
/// this API cannot inspect or control individual Wayland windows.
#[derive(Debug, Clone, Copy)]
pub struct Target(pub u64);
#[cfg(target_os = "macos")]
pub fn activate_current_app() -> Result<()> {
    anyhow::ensure!(
        objc2::MainThreadMarker::new().is_some(),
        "macOS activation must run on the main thread"
    );
    // Showing a window can make it key while its menu-bar app is inactive.
    // Winit then considers it focused and skips its own app activation.
    // An explicit picker-open action must activate Picora as well as its window.
    let app = objc2_app_kit::NSRunningApplication::currentApplication();
    anyhow::ensure!(
        app.activateWithOptions(objc2_app_kit::NSApplicationActivationOptions::empty()),
        "macOS did not accept the activation request"
    );
    Ok(())
}

#[must_use]
pub fn capture() -> Option<Target> {
    if super::is_wayland() {
        return Some(Target(0));
    }
    #[cfg(target_os = "windows")]
    {
        // SAFETY: parameterless query; the returned HWND is borrowed and never dereferenced.
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
            return Some(Target(u64::from(focus)));
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}
/// Capture the recipient, excluding Picora's windows on Windows, macOS, and X11.
/// On Wayland, returns the sentinel without identifying or excluding any window.
#[must_use]
pub fn capture_external() -> Option<Target> {
    let target = capture()?;
    #[cfg(target_os = "windows")]
    {
        let mut pid = 0;
        // SAFETY: pid is writable for the call; Windows validates the opaque HWND.
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
        let mut window = u32::try_from(target.0).ok()?;
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
/// Restore the native recipient on Windows, macOS, or X11. Wayland is a successful
/// no-op; the compositor controls focus after the picker closes.
///
/// # Errors
/// Returns an error if native target lookup or focus restoration fails.
pub fn restore(target: Target) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        // SAFETY: the opaque HWND is passed back to Windows, which rejects invalid handles.
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
        conn.set_input_focus(
            InputFocus::PARENT,
            u32::try_from(target.0)?,
            x11rb::CURRENT_TIME,
        )?
        .check()?;
        conn.flush()?;
    }
    Ok(())
}
#[must_use]
pub fn may_restore(current: Option<u64>, target: u64, picker: Option<u64>) -> bool {
    current.is_none() || current == Some(target) || current == picker
}
/// # Errors
/// Returns an error if focus changes, the ticket is cancelled, input permission is missing, or keyboard injection fails. macOS requires the main thread.
pub fn paste(target: Target, picker: Option<Target>, ticket: super::paste::Ticket) -> Result<()> {
    #[cfg(target_os = "macos")]
    anyhow::ensure!(
        objc2::MainThreadMarker::new().is_some(),
        "macOS paste must run on the main thread"
    );
    prepare_paste(target, picker, &ticket)?;
    send_paste(target, ticket)
}

/// # Errors
/// Returns an error if the ticket is cancelled, another application has focus, or the target cannot be restored.
pub fn prepare_paste(
    target: Target,
    picker: Option<Target>,
    ticket: &super::paste::Ticket,
) -> Result<()> {
    anyhow::ensure!(
        ticket.valid() && may_restore(capture().map(|t| t.0), target.0, picker.map(|t| t.0)),
        "Focus changed; paste manually"
    );
    restore(target)?;
    std::thread::sleep(std::time::Duration::from_millis(120));
    Ok(())
}

/// # Errors
/// Returns an error on cancellation, changed focus, missing input permission, or keyboard injection failure. macOS requires the main thread.
pub fn send_paste(target: Target, ticket: super::paste::Ticket) -> Result<()> {
    #[cfg(target_os = "macos")]
    anyhow::ensure!(
        objc2::MainThreadMarker::new().is_some(),
        "macOS paste must run on the main thread"
    );
    if !ticket.valid() || capture().map(|t| t.0) != Some(target.0) {
        bail!("Focus changed; paste manually");
    }
    #[cfg(target_os = "macos")]
    if !super::macos_permissions::accessibility_allowed() {
        return Err(super::macos_permissions::AccessibilityRequired.into());
    }
    let settings = Settings {
        // Consent is requested from visible setup controls, never after hiding
        // the picker. A failed paste must not repeatedly open OS dialogs.
        #[cfg(target_os = "macos")]
        open_prompt_to_get_permissions: false,
        ..Default::default()
    };
    let mut input = Enigo::new(&settings).map_err(|e| {
        #[cfg(target_os = "macos")]
        if matches!(e, enigo::NewConError::NoPermission) {
            return anyhow::Error::from(super::macos_permissions::AccessibilityRequired);
        }
        anyhow::anyhow!("Input permission required: {e}")
    })?;
    let modifier = if cfg!(target_os = "macos") {
        Key::Meta
    } else {
        Key::Control
    };
    // Resolve before holding Command so lookup failure cannot leave it pressed.
    #[cfg(target_os = "macos")]
    let paste_keycode = super::macos_keyboard::paste_keycode()?;
    input.key(modifier, Direction::Press)?;
    #[cfg(target_os = "macos")]
    let result = input.raw(paste_keycode, Direction::Click);
    #[cfg(not(target_os = "macos"))]
    let result = input.key(Key::Unicode('v'), Direction::Click);
    let release = input.key(modifier, Direction::Release);
    result?;
    release?;
    Ok(())
}
