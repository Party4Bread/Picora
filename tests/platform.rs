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
