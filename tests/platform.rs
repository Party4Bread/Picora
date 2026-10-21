use picora::platform::paste::{Action, PasteFlow};
#[cfg(target_os = "macos")]
#[test]
fn macos_worker_paste_is_rejected_before_calling_native_keyboard_apis() {
    let result = std::thread::spawn(|| {
        picora::platform::focus::send_paste(
            picora::platform::focus::Target(0),
            picora::platform::paste::Cancellation::default().ticket(),
        )
    })
    .join()
    .expect("Worker paste must return an error instead of terminating the process");
    assert_eq!(
        result.unwrap_err().to_string(),
        "macOS paste must run on the main thread"
    );
}

#[test]
fn never_hides_or_injects_before_successful_copy() {
    let mut flow = PasteFlow::new(true, true);
    assert!(flow.hidden().is_err());
    assert_eq!(flow.copied().unwrap(), Action::Hide);
    assert_eq!(flow.hidden().unwrap(), Action::Paste);
    assert!(flow.hidden().is_err());
}
#[test]
fn disabled_auto_paste_or_missing_target_only_copies() {
    for (auto, target) in [(false, true), (true, false)] {
        let mut flow = PasteFlow::new(auto, target);
        flow.copied().unwrap();
        assert_eq!(flow.hidden().unwrap(), Action::Copied);
    }
}
#[test]
fn focus_loss_while_loading_cancels_automatic_paste() {
    let mut flow = PasteFlow::new(true, true);
    flow.cancel_auto();
    flow.copied().unwrap();
    assert_eq!(flow.hidden().unwrap(), Action::Copied);
}
#[test]
fn reopening_picker_invalidates_work_already_scheduled_on_a_worker() {
    let cancellation = picora::platform::paste::Cancellation::default();
    let ticket = cancellation.ticket();
    assert!(ticket.valid());
    cancellation.cancel();
    assert!(!ticket.valid());
    assert!(cancellation.ticket().valid());
}
#[test]
fn a_different_app_gaining_focus_prevents_focus_restoration() {
    use picora::platform::focus::may_restore;
    assert!(may_restore(Some(10), 10, Some(20)));
    assert!(may_restore(Some(20), 10, Some(20)));
    assert!(may_restore(None, 10, Some(20)));
    assert!(!may_restore(Some(30), 10, Some(20)));
}
