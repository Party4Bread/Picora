//! Where the picker opens: next to the text cursor of the app being typed in,
//! or at the mouse pointer when the text cursor cannot be found.
//!
//! Rectangles use the units winit takes for window positions: physical pixels
//! on Windows and X11, points with a top-left origin on macOS.
use eframe::egui::{Pos2, Rect, Vec2, pos2, vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    /// The text cursor, or an empty rectangle at the mouse pointer.
    pub target: Rect,
    /// Usable area of the display that contains `target`.
    pub screen: Rect,
}

/// Finds the text cursor when `text_cursor` is set and the platform reports it,
/// otherwise the mouse pointer. `runtime` serves desktop queries over D-Bus.
#[must_use]
pub fn locate(text_cursor: bool, runtime: &tokio::runtime::Handle) -> Option<Anchor> {
    let target = text_cursor
        .then(imp::caret)
        .flatten()
        .or_else(|| imp::pointer(runtime).map(|p| Rect::from_min_size(p, Vec2::ZERO)))?;
    let screen = imp::work_area(target.center())?;
    Some(Anchor { target, screen })
}

/// Whether `locate(true)` can find text cursors on this platform.
#[must_use]
pub fn finds_text_cursor() -> bool {
    cfg!(any(target_os = "windows", target_os = "macos"))
}

/// Top-left corner for a window of `size`: below `target` when it fits,
/// otherwise above it, and always inside `screen`.
#[must_use]
pub fn place(anchor: Anchor, size: Vec2, gap: f32) -> Pos2 {
    let Anchor { target, screen } = anchor;
    let fit = |value: f32, min: f32, len: f32, max: f32| value.min(max - len).max(min);
    let x = fit(target.left(), screen.left(), size.x, screen.right());
    let below = target.bottom() + gap;
    let above = target.top() - gap - size.y;
    let y = if below + size.y <= screen.bottom() || above < screen.top() {
        fit(below, screen.top(), size.y, screen.bottom())
    } else {
        above
    };
    pos2(x, y)
}

#[cfg(target_os = "windows")]
mod imp {
    use super::{Pos2, Rect, pos2, vec2};
    use windows_sys::Win32::{
        Foundation::{HWND, POINT},
        Graphics::Gdi::{
            ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO,
            MonitorFromPoint,
        },
        UI::WindowsAndMessaging::{
            GUITHREADINFO, GetCursorPos, GetForegroundWindow, GetGUIThreadInfo,
            GetWindowThreadProcessId,
        },
    };

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect::from_min_size(pos2(x as f32, y as f32), vec2(w as f32, h as f32))
    }

    pub fn caret() -> Option<Rect> {
        // SAFETY: parameterless query; the HWND is only passed back to Windows.
        let window = unsafe { GetForegroundWindow() };
        if window.is_null() {
            return None;
        }
        // SAFETY: Windows validates the HWND; the process id is not requested.
        let thread = unsafe { GetWindowThreadProcessId(window, std::ptr::null_mut()) };
        // SAFETY: GUITHREADINFO is plain data; cbSize is set as the API requires.
        let mut info: GUITHREADINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        // SAFETY: info is writable and sized for this call.
        if unsafe { GetGUIThreadInfo(thread, &mut info) } == 0 {
            return None;
        }
        // Classic Win32 text boxes report a system caret in client coordinates.
        let r = info.rcCaret;
        if !info.hwndCaret.is_null() && r.bottom > r.top {
            let mut corner = POINT {
                x: r.left,
                y: r.top,
            };
            // SAFETY: corner is writable; Windows validates the caret window.
            if unsafe { ClientToScreen(info.hwndCaret, &mut corner) } != 0 {
                return Some(rect(corner.x, corner.y, r.right - r.left, r.bottom - r.top));
            }
        }
        let focus = if info.hwndFocus.is_null() {
            window
        } else {
            info.hwndFocus
        };
        accessible_caret(focus)
    }

    /// Browsers, Electron apps and Office expose their caret only through
    /// accessibility. The call reaches into the other app, so a hung app must
    /// not delay the picker.
    fn accessible_caret(window: HWND) -> Option<Rect> {
        let window = window as isize;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(msaa_caret(window));
        });
        rx.recv_timeout(std::time::Duration::from_millis(150))
            .ok()
            .flatten()
    }

    fn msaa_caret(window: isize) -> Option<Rect> {
        use windows::Win32::{
            Foundation::HWND,
            System::{
                Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
                Variant::VARIANT,
            },
            UI::Accessibility::{AccessibleObjectFromWindow, IAccessible},
            UI::WindowsAndMessaging::{CHILDID_SELF, OBJID_CARET},
        };
        use windows::core::Interface;
        // SAFETY: this thread is new and uninitialized; it is balanced below.
        if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
            return None;
        }
        let found = (|| {
            let mut object = std::ptr::null_mut();
            // SAFETY: object receives an owned IAccessible pointer on success.
            unsafe {
                AccessibleObjectFromWindow(
                    HWND(window as _),
                    OBJID_CARET.0 as u32,
                    &IAccessible::IID,
                    &mut object,
                )
            }
            .ok()?;
            // SAFETY: on success, object is an owned IAccessible reference.
            let caret = unsafe { IAccessible::from_raw(object) };
            let (mut x, mut y, mut w, mut h) = (0, 0, 0, 0);
            // SAFETY: the out-parameters are writable for this call.
            unsafe {
                caret.accLocation(
                    &mut x,
                    &mut y,
                    &mut w,
                    &mut h,
                    &VARIANT::from(CHILDID_SELF as i32),
                )
            }
            .ok()?;
            (h > 0).then(|| rect(x, y, w, h))
        })();
        // SAFETY: balances the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
        found
    }

    pub fn pointer(_: &tokio::runtime::Handle) -> Option<Pos2> {
        let mut point = POINT { x: 0, y: 0 };
        // SAFETY: point is writable for this call.
        (unsafe { GetCursorPos(&mut point) } != 0).then(|| pos2(point.x as f32, point.y as f32))
    }

    pub fn work_area(point: Pos2) -> Option<Rect> {
        let point = POINT {
            x: point.x as i32,
            y: point.y as i32,
        };
        // SAFETY: MONITOR_DEFAULTTONEAREST always yields a monitor handle.
        let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
        // SAFETY: MONITORINFO is plain data; cbSize is set as the API requires.
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        // SAFETY: info is writable and sized for this call.
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return None;
        }
        let r = info.rcWork;
        Some(rect(r.left, r.top, r.right - r.left, r.bottom - r.top))
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{Pos2, Rect, pos2, vec2};
    use core_foundation::{
        base::{CFType, CFTypeRef, TCFType},
        boolean::CFBoolean,
        string::{CFString, CFStringRef},
    };
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy)]
    struct CGRect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy)]
    struct CFRange {
        location: isize,
        length: isize,
    }
    const AX_VALUE_CG_RECT: u32 = 3;
    const AX_VALUE_CF_RANGE: u32 = 4;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXUIElementCreateSystemWide() -> CFTypeRef;
        fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
        fn AXUIElementCopyAttributeValue(
            element: CFTypeRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
        fn AXUIElementCopyParameterizedAttributeValue(
            element: CFTypeRef,
            attribute: CFStringRef,
            parameter: CFTypeRef,
            value: *mut CFTypeRef,
        ) -> i32;
        fn AXUIElementSetAttributeValue(
            element: CFTypeRef,
            attribute: CFStringRef,
            value: CFTypeRef,
        ) -> i32;
        fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> i32;
        fn AXUIElementGetPid(element: CFTypeRef, pid: *mut i32) -> i32;
        fn AXValueCreate(kind: u32, value: *const c_void) -> CFTypeRef;
        fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> bool;
    }

    /// Wraps a +1 reference returned by a Create or Copy call.
    fn owned(value: CFTypeRef) -> Option<CFType> {
        // SAFETY: callers pass only references they own.
        (!value.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(value) })
    }

    fn attribute(element: &CFType, name: &str) -> Option<CFType> {
        let name = CFString::new(name);
        let mut value = std::ptr::null();
        // SAFETY: element is an AXUIElement; value receives a +1 reference on success.
        let error = unsafe {
            AXUIElementCopyAttributeValue(
                element.as_CFTypeRef(),
                name.as_concrete_TypeRef(),
                &mut value,
            )
        };
        if error == 0 { owned(value) } else { None }
    }

    fn bounds(element: &CFType, range: CFRange) -> Option<Rect> {
        // SAFETY: range is a CFRange as AX_VALUE_CF_RANGE requires.
        let range = owned(unsafe { AXValueCreate(AX_VALUE_CF_RANGE, (&raw const range).cast()) })?;
        let name = CFString::new("AXBoundsForRange");
        let mut value = std::ptr::null();
        // SAFETY: element is an AXUIElement and range an AXValue; value receives +1.
        let error = unsafe {
            AXUIElementCopyParameterizedAttributeValue(
                element.as_CFTypeRef(),
                name.as_concrete_TypeRef(),
                range.as_CFTypeRef(),
                &mut value,
            )
        };
        let value = if error == 0 {
            owned(value)?
        } else {
            return None;
        };
        let mut rect = CGRect::default();
        // SAFETY: rect matches AX_VALUE_CG_RECT; the call fails for other value kinds.
        let ok = unsafe {
            AXValueGetValue(
                value.as_CFTypeRef(),
                AX_VALUE_CG_RECT,
                (&raw mut rect).cast(),
            )
        };
        (ok && rect.height > 0.).then(|| {
            Rect::from_min_size(
                pos2(rect.x as f32, rect.y as f32),
                vec2(rect.width as f32, rect.height as f32),
            )
        })
    }

    fn text_cursor(system: &CFType) -> Option<Rect> {
        let focused = attribute(system, "AXFocusedUIElement")?;
        let selection = attribute(&focused, "AXSelectedTextRange")?;
        let mut range = CFRange::default();
        // SAFETY: range matches AX_VALUE_CF_RANGE; the call fails for other value kinds.
        if !unsafe {
            AXValueGetValue(
                selection.as_CFTypeRef(),
                AX_VALUE_CF_RANGE,
                (&raw mut range).cast(),
            )
        } {
            return None;
        }
        if range.length > 0 {
            return bounds(&focused, range);
        }
        // Many apps report no bounds for an empty range. Measure a neighbouring
        // character and use the edge that touches the cursor.
        let caret = |r: Rect, x: f32| Rect::from_min_size(pos2(x, r.top()), vec2(0., r.height()));
        bounds(&focused, range)
            .or_else(|| {
                bounds(
                    &focused,
                    CFRange {
                        location: range.location,
                        length: 1,
                    },
                )
                .map(|r| caret(r, r.left()))
            })
            .or_else(|| {
                (range.location > 0).then_some(())?;
                bounds(
                    &focused,
                    CFRange {
                        location: range.location - 1,
                        length: 1,
                    },
                )
                .map(|r| caret(r, r.right()))
            })
    }

    pub fn caret() -> Option<Rect> {
        if !super::super::macos_permissions::accessibility_allowed() {
            return None;
        }
        // SAFETY: returns a +1 reference to the system-wide element.
        let system = owned(unsafe { AXUIElementCreateSystemWide() })?;
        // The timeout on the system-wide element applies to every element, so
        // an unresponsive app cannot hold up the picker.
        // SAFETY: system is an AXUIElement.
        unsafe { AXUIElementSetMessagingTimeout(system.as_CFTypeRef(), 0.1) };
        if let Some(rect) = text_cursor(&system) {
            return Some(rect);
        }
        // Chromium and Electron apps build their accessibility tree only on
        // request. Ask once, then retry; later lookups find it ready.
        let app = attribute(&system, "AXFocusedApplication")?;
        let mut pid = 0;
        // SAFETY: app is an AXUIElement; pid is writable.
        if unsafe { AXUIElementGetPid(app.as_CFTypeRef(), &mut pid) } != 0 {
            return None;
        }
        // SAFETY: returns a +1 reference to the application element.
        let app = owned(unsafe { AXUIElementCreateApplication(pid) })?;
        let name = CFString::new("AXManualAccessibility");
        // SAFETY: app is an AXUIElement; the boolean is a static CFBoolean.
        let enabled = unsafe {
            AXUIElementSetAttributeValue(
                app.as_CFTypeRef(),
                name.as_concrete_TypeRef(),
                CFBoolean::true_value().as_CFTypeRef(),
            )
        };
        if enabled == 0 {
            text_cursor(&system)
        } else {
            None
        }
    }

    /// Converts AppKit's bottom-left coordinates to the top-left coordinates
    /// that accessibility and window positions use.
    fn flip(rect: objc2_foundation::NSRect, primary_height: f64) -> Rect {
        Rect::from_min_size(
            pos2(
                rect.origin.x as f32,
                (primary_height - rect.origin.y - rect.size.height) as f32,
            ),
            vec2(rect.size.width as f32, rect.size.height as f32),
        )
    }

    fn screens() -> Option<(Vec<objc2::rc::Retained<objc2_app_kit::NSScreen>>, f64)> {
        let mtm = objc2::MainThreadMarker::new()?;
        let screens = objc2_app_kit::NSScreen::screens(mtm).to_vec();
        let primary = screens.first()?.frame().size.height;
        Some((screens, primary))
    }

    pub fn pointer(_: &tokio::runtime::Handle) -> Option<Pos2> {
        let (_, primary) = screens()?;
        let point = objc2_app_kit::NSEvent::mouseLocation();
        Some(pos2(point.x as f32, (primary - point.y) as f32))
    }

    pub fn work_area(point: Pos2) -> Option<Rect> {
        let (screens, primary) = screens()?;
        let screen = screens
            .iter()
            .find(|s| flip(s.frame(), primary).contains(point))
            .or_else(|| screens.first())?;
        Some(flip(screen.visibleFrame(), primary))
    }
}

#[cfg(target_os = "linux")]
pub use imp::frame_extents;

#[cfg(target_os = "linux")]
mod imp {
    use super::{Pos2, Rect, pos2, vec2};
    use std::time::{Duration, Instant};
    use x11rb::{
        connection::Connection,
        protocol::{
            Event,
            randr::ConnectionExt as _,
            xproto::{
                AtomEnum, ColormapAlloc, ConnectionExt as _, CreateWindowAux, EventMask, Screen,
                VisualClass, WindowClass,
            },
        },
        rust_connection::RustConnection,
    };

    /// No text cursor lookup yet: Wayland apps do not share it with other apps.
    pub fn caret() -> Option<Rect> {
        None
    }

    fn connect() -> Option<(RustConnection, Screen)> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let screen = conn.setup().roots.get(screen)?.clone();
        Some((conn, screen))
    }

    /// On Wayland the picker is an XWayland window, but XWayland knows where the
    /// pointer is only while it is over an XWayland window.
    pub fn pointer(runtime: &tokio::runtime::Handle) -> Option<Pos2> {
        let (conn, screen) = connect()?;
        let last = conn.query_pointer(screen.root).ok()?.reply().ok()?;
        let last = pos2(last.root_x.into(), last.root_y.into());
        if !super::super::is_wayland() {
            return Some(last);
        }
        let root = vec2(
            screen.width_in_pixels.into(),
            screen.height_in_pixels.into(),
        );
        runtime
            .block_on(kwin_pointer(root))
            .or_else(|| probe_pointer(&conn, &screen, last))
    }

    /// Plasma reports the pointer to a KWin script, which passes it back over D-Bus.
    async fn kwin_pointer(root: eframe::egui::Vec2) -> Option<Pos2> {
        use std::sync::Mutex;
        use tokio::sync::oneshot;
        struct Reporter(Mutex<Option<oneshot::Sender<String>>>);
        #[zbus::interface(name = "app.picora.Pointer")]
        impl Reporter {
            fn report(&self, position: String) {
                if let Some(tx) = self.0.lock().ok().and_then(|mut tx| tx.take()) {
                    let _ = tx.send(position);
                }
            }
        }
        let conn = zbus::Connection::session().await.ok()?;
        let scripting = zbus::Proxy::new(
            &conn,
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
        )
        .await
        .ok()?;
        let (tx, rx) = oneshot::channel();
        conn.object_server()
            .at("/app/picora/Pointer", Reporter(Mutex::new(Some(tx))))
            .await
            .ok()?;
        let mut file = tempfile::Builder::new().suffix(".js").tempfile().ok()?;
        let name = conn.unique_name()?.to_string();
        std::io::Write::write_all(
            &mut file,
            format!(
                "const p = workspace.cursorPos, s = workspace.virtualScreenSize;
callDBus({name:?}, '/app/picora/Pointer', 'app.picora.Pointer', 'Report',
    [p.x, p.y, s.width, s.height].join(','));"
            )
            .as_bytes(),
        )
        .ok()?;
        let plugin = format!("picora-pointer-{}", std::process::id());
        let path = file.path().to_str()?;
        let id: i32 = scripting.call("loadScript", &(path, &plugin)).await.ok()?;
        let found = async {
            zbus::Proxy::new(
                &conn,
                "org.kde.KWin",
                format!("/Scripting/Script{id}"),
                "org.kde.kwin.Script",
            )
            .await
            .ok()?
            .call::<_, _, ()>("run", &())
            .await
            .ok()?;
            tokio::time::timeout(Duration::from_millis(300), rx)
                .await
                .ok()?
                .ok()
        }
        .await;
        let _ = scripting
            .call::<_, _, bool>("unloadScript", &(&plugin,))
            .await;
        kwin_position(&found?, root)
    }

    /// Reads `x,y,width,height` from the KWin script: the pointer and the
    /// desktop size in logical pixels. XWayland's pixels are smaller when
    /// Plasma lets X11 apps scale themselves, so scale to its `root` size.
    pub(super) fn kwin_position(report: &str, root: eframe::egui::Vec2) -> Option<Pos2> {
        let values = report
            .split(',')
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let &[x, y, width, height] = values.as_slice() else {
            return None;
        };
        (width > 0. && height > 0.).then(|| pos2(x * root.x / width, y * root.y / height))
    }

    /// Briefly cover the screen with a transparent window that does not take
    /// focus, and read the position the compositor reports when the pointer is
    /// over it. The X server first reports its own outdated position `last`, and
    /// compositors that only report the pointer once it moves report nothing, so
    /// only a different position counts. Both windows share XWayland's
    /// coordinates, so the position needs no conversion.
    fn probe_pointer(conn: &RustConnection, screen: &Screen, last: Pos2) -> Option<Pos2> {
        let visual = screen
            .allowed_depths
            .iter()
            .filter(|d| d.depth == 32)
            .flat_map(|d| &d.visuals)
            .find(|v| v.class == VisualClass::TRUE_COLOR)?
            .visual_id;
        let colormap = conn.generate_id().ok()?;
        let window = conn.generate_id().ok()?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, screen.root, visual)
            .ok()?;
        conn.create_window(
            32,
            window,
            screen.root,
            0,
            0,
            screen.width_in_pixels,
            screen.height_in_pixels,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new()
                .background_pixel(0)
                .border_pixel(0)
                .colormap(colormap)
                .override_redirect(1)
                .event_mask(EventMask::ENTER_WINDOW | EventMask::POINTER_MOTION),
        )
        .ok()?;
        conn.map_window(window).ok()?;
        conn.flush().ok()?;
        let deadline = Instant::now() + Duration::from_millis(150);
        let mut found = None;
        while found.is_none() && Instant::now() < deadline {
            match conn.poll_for_event() {
                Ok(Some(Event::EnterNotify(e))) => {
                    found = Some(pos2(e.root_x.into(), e.root_y.into())).filter(|p| *p != last);
                }
                Ok(Some(Event::MotionNotify(e))) => {
                    found = Some(pos2(e.root_x.into(), e.root_y.into())).filter(|p| *p != last);
                }
                Ok(Some(_)) => {}
                Ok(None) => std::thread::sleep(Duration::from_millis(2)),
                Err(_) => break,
            }
        }
        let _ = conn.destroy_window(window);
        let _ = conn.free_colormap(colormap);
        let _ = conn.flush();
        found
    }

    /// Width and height the window manager's frame adds around `window`.
    pub fn frame_extents(window: u32) -> Option<eframe::egui::Vec2> {
        let (conn, _) = connect()?;
        let atom = conn
            .intern_atom(false, b"_NET_FRAME_EXTENTS")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let reply = conn
            .get_property(false, window, atom, AtomEnum::CARDINAL, 0, 4)
            .ok()?
            .reply()
            .ok()?;
        let [left, right, top, bottom] =
            <[u32; 4]>::try_from(reply.value32()?.collect::<Vec<_>>()).ok()?;
        Some(vec2((left + right) as f32, (top + bottom) as f32))
    }

    pub fn work_area(point: Pos2) -> Option<Rect> {
        let (conn, screen) = connect()?;
        let monitor = conn
            .randr_get_monitors(screen.root, true)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|reply| {
                reply
                    .monitors
                    .iter()
                    .map(|m| {
                        Rect::from_min_size(
                            pos2(m.x.into(), m.y.into()),
                            vec2(m.width.into(), m.height.into()),
                        )
                    })
                    .find(|m| m.contains(point))
            })
            .unwrap_or_else(|| {
                Rect::from_min_size(
                    Pos2::ZERO,
                    vec2(
                        screen.width_in_pixels.into(),
                        screen.height_in_pixels.into(),
                    ),
                )
            });
        // Panels reserve space through the desktop-wide work area.
        let atom = conn
            .intern_atom(false, b"_NET_WORKAREA")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let workarea = conn
            .get_property(false, screen.root, atom, AtomEnum::CARDINAL, 0, 4)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| {
                let v: Vec<u32> = r.value32()?.collect();
                (v.len() == 4).then(|| {
                    Rect::from_min_size(
                        pos2(v[0] as f32, v[1] as f32),
                        vec2(v[2] as f32, v[3] as f32),
                    )
                })
            });
        Some(
            workarea
                .map(|w| w.intersect(monitor))
                .filter(|r| r.is_positive())
                .unwrap_or(monitor),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect::from_min_max(pos2(0., 0.), pos2(1000., 800.));
    const SIZE: Vec2 = vec2(300., 400.);

    fn at(x: f32, y: f32, height: f32) -> Anchor {
        Anchor {
            target: Rect::from_min_size(pos2(x, y), vec2(0., height)),
            screen: SCREEN,
        }
    }

    #[test]
    fn opens_below_the_cursor_when_it_fits() {
        assert_eq!(place(at(100., 50., 20.), SIZE, 4.), pos2(100., 74.));
    }

    #[test]
    fn opens_above_the_cursor_near_the_bottom_edge() {
        assert_eq!(place(at(100., 700., 20.), SIZE, 4.), pos2(100., 296.));
    }

    #[test]
    fn stays_on_screen_near_the_right_edge_and_when_neither_side_fits() {
        assert_eq!(place(at(950., 50., 20.), SIZE, 4.), pos2(700., 74.));
        let tall = vec2(300., 700.);
        assert_eq!(place(at(100., 380., 20.), tall, 4.), pos2(100., 100.));
        let huge = vec2(1200., 900.);
        assert_eq!(place(at(100., 380., 20.), huge, 4.), pos2(0., 0.));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kwin_pointer_is_scaled_to_xwayland_pixels() {
        use imp::kwin_position;
        let root = vec2(3200., 1800.);
        assert_eq!(
            kwin_position("400,300.5,1600,900", root),
            Some(pos2(800., 601.))
        );
        assert_eq!(
            kwin_position("400,300,3200,1800", root),
            Some(pos2(400., 300.))
        );
        assert_eq!(kwin_position("400,300,0,0", root), None);
        assert_eq!(kwin_position("400,300", root), None);
        assert_eq!(kwin_position("x,300,1600,900", root), None);
    }

    #[test]
    fn respects_a_screen_that_does_not_start_at_the_origin() {
        let anchor = Anchor {
            target: Rect::from_min_size(pos2(-1900., 1000.), Vec2::ZERO),
            screen: Rect::from_min_max(pos2(-1920., 0.), pos2(0., 1040.)),
        };
        assert_eq!(place(anchor, SIZE, 4.), pos2(-1900., 596.));
    }
}
