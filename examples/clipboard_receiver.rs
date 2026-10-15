//! X11 smoke-test target: captures Ctrl+V image data without contacting a chat service.
#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    use std::io::Write;
    use x11rb::{
        connection::Connection,
        protocol::{Event, xproto::*},
        wrapper::ConnectionExt as _,
    };
    let (conn, screen) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen];
    let window = conn.generate_id()?;
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        window,
        screen.root,
        10,
        10,
        320,
        160,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new()
            .background_pixel(screen.white_pixel)
            .event_mask(EventMask::KEY_PRESS | EventMask::STRUCTURE_NOTIFY),
    )?;
    conn.change_property8(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        b"Picora clipboard test receiver",
    )?;
    let clipboard = conn.intern_atom(false, b"CLIPBOARD")?.reply()?.atom;
    let png = conn.intern_atom(false, b"image/png")?.reply()?.atom;
    let property = conn.intern_atom(false, b"PICORA_TEST_IMAGE")?.reply()?.atom;
    let pid = conn.intern_atom(false, b"_NET_WM_PID")?.reply()?.atom;
    conn.change_property32(
        PropMode::REPLACE,
        window,
        pid,
        AtomEnum::CARDINAL,
        &[std::process::id()],
    )?;
    conn.map_window(window)?;
    conn.flush()?;
    println!("WINDOW={window}");
    std::io::stdout().flush()?;
    loop {
        match conn.wait_for_event()? {
            Event::KeyPress(event) => {
                let keys = conn.get_keyboard_mapping(event.detail, 1)?.reply()?.keysyms;
                if keys.contains(&0xff0d) {
                    println!("ENTER_SEEN");
                }
                if event.state.contains(KeyButMask::CONTROL) && keys.contains(&0x76) {
                    println!("PASTE_SHORTCUT");
                    conn.convert_selection(window, clipboard, png, property, event.time)?;
                    conn.flush()?;
                }
                std::io::stdout().flush()?;
            }
            Event::SelectionNotify(event) if event.property != 0 => {
                let data = conn
                    .get_property(true, window, property, png, 0, 1024 * 1024)?
                    .reply()?;
                let path = std::env::args()
                    .nth(1)
                    .unwrap_or_else(|| "/tmp/picora-received.png".into());
                std::fs::write(&path, &data.value)?;
                println!("IMAGE_BYTES={}", data.value.len());
                std::io::stdout().flush()?;
            }
            _ => {}
        }
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This manual test receiver requires Linux X11/XWayland.");
}
