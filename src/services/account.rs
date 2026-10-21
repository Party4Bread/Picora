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

#[cfg(test)]
mod lifecycle_tests {
    use crate::{
        media::MediaCache,
        model::Provider,
        services::{Bus, Event, Services},
    };
    use std::sync::mpsc;

    #[test]
    fn login_preserves_session_across_storage_and_verification_failures() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for (save_ok, verify_ok) in [(true, true), (false, true), (true, false)] {
            let dir = tempfile::tempdir().unwrap();
            let (tx, rx) = mpsc::channel();
            let services = Services::with_session(
                rt.handle().clone(),
                MediaCache::new(dir.path().into()).unwrap(),
                Bus {
                    tx,
                    ctx: Default::default(),
                },
                Ok(vec![]),
            )
            .unwrap();
            rt.block_on(services.login_with(
                async { Ok(vec!["session=test-session; Path=/; Secure".into()]) },
                |cookies| async move {
                    assert_eq!(cookies.len(), 1);
                    if save_ok {
                        Ok(())
                    } else {
                        anyhow::bail!("credential store locked")
                    }
                },
                |repo| async move {
                    assert_eq!(repo.cookie("session").as_deref(), Some("test-session"));
                    if verify_ok {
                        Ok(())
                    } else {
                        anyhow::bail!("verification required")
                    }
                },
            ));
            assert_eq!(
                services
                    .repo(Provider::Arca)
                    .unwrap()
                    .cookie("session")
                    .as_deref(),
                Some("test-session")
            );
            let status = services.account_status.lock().unwrap().clone();
            assert!(status.has_session);
            assert_eq!(status.saved, save_ok);
            assert_eq!(status.verified(), verify_ok);
            assert_eq!(status.storage_error.is_some(), !save_ok);
            assert_eq!(status.error().is_some(), !verify_ok);
            let Event::Login(result) = rx.try_recv().unwrap() else {
                panic!("missing login completion")
            };
            assert_eq!(result.unwrap(), status.label());
            assert!(rx.try_recv().is_err());

            let error = services
                .logout_with(|| anyhow::bail!("deletion denied"))
                .unwrap_err();
            assert!(error.to_string().contains("deletion denied"));
            assert!(
                services
                    .repo(Provider::Arca)
                    .unwrap()
                    .cookie("session")
                    .is_none()
            );
            let status = services.account_status.lock().unwrap().clone();
            assert!(!status.has_session);
            assert!(status.saved);
            assert!(status.error().unwrap().contains("Disconnected locally"));
            services.logout_with(|| Ok(())).unwrap();
            assert_eq!(
                services.account_status.lock().unwrap().label(),
                "Not connected"
            );
        }
    }

    #[test]
    fn failed_sign_in_emits_error_without_saving_or_verifying() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let services = Services::with_session(
            rt.handle().clone(),
            MediaCache::new(dir.path().into()).unwrap(),
            Bus {
                tx,
                ctx: Default::default(),
            },
            Ok(vec![]),
        )
        .unwrap();
        rt.block_on(services.login_with(
            async { anyhow::bail!("sign-in cancelled") },
            |_| async { panic!("must not persist a failed sign-in") },
            |_| async { panic!("must not verify a failed sign-in") },
        ));
        assert!(!services.account_status.lock().unwrap().has_session);
        let Event::Login(result) = rx.try_recv().unwrap() else {
            panic!("missing login completion")
        };
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("sign-in cancelled")
        );
    }
}
