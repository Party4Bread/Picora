//! Accessibility consent belongs to the running app, not the receiving browser.
use core_foundation::{
    base::TCFType, boolean::CFBoolean, dictionary::CFDictionary, string::CFStringRef,
};

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef)
    -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

#[derive(Debug)]
pub struct AccessibilityRequired;

impl std::fmt::Display for AccessibilityRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Allow Picora in macOS Accessibility settings to paste automatically. The sticker is copied; you can switch back and press Cmd+V.")
    }
}
impl std::error::Error for AccessibilityRequired {}

/// Query each time: granting or revoking consent must not be cached by Picora.
pub fn accessibility_allowed() -> bool {
    // SAFETY: this process-wide, parameterless query does not modify consent.
    unsafe { AXIsProcessTrusted() }
}

/// Only an explicit setup action may prompt; ordinary copies never request consent.
pub fn request_accessibility() -> bool {
    // SAFETY: the framework owns this static CFString; the dictionary retains it.
    let key = unsafe {
        core_foundation::string::CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt)
    };
    let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
    // The OS prompt is asynchronous. Its immediate return is not approval.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

pub fn open_settings() -> anyhow::Result<()> {
    open::that("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")?;
    Ok(())
}
