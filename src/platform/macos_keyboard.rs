//! Resolve command shortcuts against the ASCII layout, even while an IME is active.
use anyhow::{Result, bail, ensure};
use core_foundation::{
    base::{CFType, CFTypeRef, TCFType},
    data::CFData,
    string::CFStringRef,
};
use std::ffi::c_void;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn TISCopyCurrentASCIICapableKeyboardLayoutInputSource() -> CFTypeRef;
    fn TISGetInputSourceProperty(source: CFTypeRef, property: CFStringRef) -> *const c_void;
    static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
    fn LMGetKbdType() -> u8;
    fn UCKeyTranslate(
        layout: *const c_void,
        keycode: u16,
        action: u16,
        modifiers: u32,
        keyboard_type: u32,
        options: u32,
        dead_keys: *mut u32,
        capacity: usize,
        length: *mut usize,
        characters: *mut u16,
    ) -> i32;
}

pub fn paste_keycode() -> Result<u16> {
    ensure!(
        objc2::MainThreadMarker::new().is_some(),
        "macOS keyboard lookup must run on the main thread"
    );
    // SAFETY: Carbon input-source APIs are called on the main thread. The copy
    // rule retains the source until all borrowed properties have been read.
    let source = unsafe { TISCopyCurrentASCIICapableKeyboardLayoutInputSource() };
    ensure!(
        !source.is_null(),
        "No ASCII keyboard layout; paste manually with Cmd+V"
    );
    let source = unsafe { CFType::wrap_under_create_rule(source) };
    let data = unsafe {
        TISGetInputSourceProperty(source.as_CFTypeRef(), kTISPropertyUnicodeKeyLayoutData)
    };
    ensure!(
        !data.is_null(),
        "Keyboard layout unavailable; paste manually with Cmd+V"
    );
    let data = unsafe { CFData::wrap_under_get_rule(data.cast()) };
    let keyboard_type = unsafe { u32::from(LMGetKbdType()) };
    find_paste_key(|keycode| {
        let mut dead_keys = 0;
        let mut length = 0;
        let mut characters = [0u16; 4];
        // SAFETY: CFData keeps the system layout alive; all output pointers
        // address writable buffers with the declared capacity. Carbon uses
        // unsigned long (usize) for UniCharCount on macOS, and u32 OptionBits.
        let status = unsafe {
            UCKeyTranslate(
                data.bytes().as_ptr().cast(),
                keycode,
                0,
                1, // cmdKey >> 8; respects layouts such as Dvorak–QWERTY Cmd.
                keyboard_type,
                1, // kUCKeyTranslateNoDeadKeysMask
                &mut dead_keys,
                characters.len(),
                &mut length,
                characters.as_mut_ptr(),
            )
        };
        (status == 0 && length == 1).then_some(characters[0])
    })
}

fn find_paste_key(mut translate: impl FnMut(u16) -> Option<u16>) -> Result<u16> {
    for keycode in 0..128 {
        if translate(keycode) == Some(u16::from(b'v')) {
            return Ok(keycode);
        }
    }
    bail!("Cannot locate the paste shortcut in this keyboard layout; paste manually with Cmd+V")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_layout_selects_v_instead_of_falling_back_to_a() {
        for v in [9, 47] {
            assert_eq!(
                find_paste_key(|code| match code {
                    0 => Some(u16::from(b'a')),
                    code if code == v => Some(u16::from(b'v')),
                    _ => None,
                })
                .unwrap(),
                v
            );
        }
    }

    #[test]
    fn missing_v_is_an_error_and_never_keycode_zero() {
        assert!(find_paste_key(|_| None).is_err());
    }
}
