#[derive(Clone, Default)]
pub struct AccountStatus {
    pub has_session: bool,
    pub saved: bool,
    pub check: CheckState,
    pub storage_error: Option<String>,
}
#[derive(Clone, Default)]
pub enum CheckState {
    #[default]
    Unchecked,
    Checking,
    Verified,
    Failed(String),
}
impl AccountStatus {
    pub fn label(&self) -> &'static str {
        if matches!(self.check, CheckState::Checking) {
            "Checking connection…"
        } else if self.error().is_some() {
            "Connection needs attention"
        } else if !self.has_session {
            "Not connected"
        } else if self.verified() && self.saved {
            "Connected"
        } else if self.verified() {
            "Connected · this session only"
        } else if self.saved {
            "Session saved · not checked"
        } else {
            "Session received · not checked"
        }
    }
    pub fn verified(&self) -> bool {
        matches!(self.check, CheckState::Verified)
    }
    pub fn checking(&self) -> bool {
        matches!(self.check, CheckState::Checking)
    }
    pub fn error(&self) -> Option<&str> {
        match &self.check {
            CheckState::Failed(error) => Some(error),
            _ => None,
        }
    }
    pub fn finish_check(&mut self, result: Result<(), String>) {
        self.check = match result {
            Ok(()) => CheckState::Verified,
            Err(error) => CheckState::Failed(error),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_session_is_not_a_verified_connection() {
        let mut status = AccountStatus {
            has_session: true,
            saved: true,
            ..Default::default()
        };
        assert_eq!(status.label(), "Session saved · not checked");
        status.finish_check(Ok(()));
        assert_eq!(status.label(), "Connected");
        status.finish_check(Err("Verification required".into()));
        assert_eq!(status.label(), "Connection needs attention");
        assert!(!status.verified());
        assert!(status.saved);
        status.finish_check(Ok(()));
        assert!(status.error().is_none());
    }
    #[test]
    fn verified_memory_only_session_is_labeled_as_session_only() {
        let mut status = AccountStatus {
            has_session: true,
            ..Default::default()
        };
        status.finish_check(Ok(()));
        assert_eq!(status.label(), "Connected · this session only");
    }
}
