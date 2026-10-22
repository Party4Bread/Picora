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
