#[test]
fn helper_rejects_missing_job_before_touching_installation() {
    assert!(picora::update::helper::run_helper(std::path::Path::new("missing-job.json")).is_err());
}
