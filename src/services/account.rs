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
    pub fn label(&self) -> std::borrow::Cow<'static, str> {
        use rust_i18n::t;
        if matches!(self.check, CheckState::Checking) {
            t!("Checking connection…")
        } else if self.error().is_some() {
            t!("Connection needs attention")
        } else if !self.has_session {
            t!("Not connected")
        } else if self.verified() && self.saved {
            t!("Connected")
        } else if self.verified() {
            t!("Connected · this session only")
        } else if self.saved {
            t!("Session saved · not checked")
        } else {
            t!("Session received · not checked")
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
mod lifecycle_tests {
    use crate::{
        model::Provider,
        services::{Event, tests::Fixture},
    };

    #[test]
    fn login_preserves_session_across_storage_and_verification_failures() {
        for (save_ok, verify_ok) in [(true, true), (false, true), (true, false)] {
            let fixture = Fixture::new();
            let (services, rt, rx) = (&fixture.services, &fixture.runtime, &fixture.rx);
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
    fn failed_reconnect_keeps_previous_session_and_does_not_persist_candidate() {
        let fixture = Fixture::new();
        let services = &fixture.services;
        services
            .repo(Provider::Arca)
            .unwrap()
            .set_cookies(&["session=previous; Path=/; Secure".into()]);
        {
            let mut status = services.account_status().unwrap();
            status.has_session = true;
            status.saved = true;
        }
        fixture.runtime.block_on(services.login_with(
            async { Ok(vec!["session=candidate; Path=/; Secure".into()]) },
            |_| async { panic!("failed reconnect must not replace saved credentials") },
            |_| async { anyhow::bail!("catalog verification failed") },
        ));
        assert_eq!(
            services
                .repo(Provider::Arca)
                .unwrap()
                .cookie("session")
                .as_deref(),
            Some("previous")
        );
        let status = services.account_status().unwrap();
        assert!(status.saved && status.has_session);
        assert!(
            status
                .error()
                .unwrap()
                .contains("catalog verification failed")
        );
        assert!(matches!(
            fixture.rx.try_recv().unwrap(),
            Event::Login(Err(_))
        ));
    }

    #[test]
    fn failed_sign_in_emits_error_without_saving_or_verifying() {
        let fixture = Fixture::new();
        let (services, rt, rx) = (&fixture.services, &fixture.runtime, &fixture.rx);
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
