use anyhow::Result;

trait TokenStore {
    fn load(&self) -> impl std::future::Future<Output = Result<Option<String>>> + Send;
    fn save(&self, token: Option<String>) -> impl std::future::Future<Output = Result<()>> + Send;
}
struct Keyring;
impl TokenStore for Keyring {
    async fn load(&self) -> Result<Option<String>> {
        tokio::task::spawn_blocking(|| {
            let entry = keyring::Entry::new("app.picora.desktop", "wayland-keyboard")?;
            match entry.get_password() {
                Ok(token) => Ok(Some(token)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await?
    }
    async fn save(&self, token: Option<String>) -> Result<()> {
        tokio::task::spawn_blocking(move || {
            let entry = keyring::Entry::new("app.picora.desktop", "wayland-keyboard")?;
            match token {
                Some(token) => entry.set_password(&token)?,
                None => match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => (),
                    Err(e) => return Err(e.into()),
                },
            }
            Ok(())
        })
        .await?
    }
}

async fn connect_saved<T, F, Fut>(
    store: &impl TokenStore,
    automatic: bool,
    connect: F,
) -> Result<Option<(T, Option<String>)>>
where
    F: FnOnce(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<(T, Option<String>)>>,
{
    let token = match store.load().await {
        Ok(token) => token,
        Err(e) if automatic => return Err(e.context("Could not load Wayland permission")),
        Err(_) => None, // Explicit reconnect remains possible with a locked keyring.
    };
    if automatic && token.is_none() {
        return Ok(None);
    }
    let restoring = token.is_some();
    match connect(token).await {
        Ok((session, token)) => {
            // Tokens can rotate, or the desktop may grant only a transient session.
            let warning = store.save(token).await.err().map(|e| {
                format!("Keyboard access enabled, but permission could not be saved: {e:#}")
            });
            Ok(Some((session, warning)))
        }
        Err(e) => {
            if restoring && let Err(clear) = store.save(None).await {
                return Err(e.context(format!("Could not clear saved permission ({clear:#}); unlock the credential store and reconnect in Settings")));
            }
            Err(e.context("Reconnect Wayland keyboard access in Settings"))
        }
    }
}

pub(crate) async fn connect(automatic: bool) -> Result<Option<(super::Wayland, Option<String>)>> {
    connect_saved(&Keyring, automatic, |token| async move {
        super::Wayland::connect(token.as_deref()).await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Memory(Mutex<Option<String>>, bool);
    impl TokenStore for Memory {
        async fn load(&self) -> Result<Option<String>> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn save(&self, token: Option<String>) -> Result<()> {
            anyhow::ensure!(!self.1, "locked keyring");
            *self.0.lock().unwrap() = token;
            Ok(())
        }
    }
    #[tokio::test]
    async fn startup_without_grant_never_opens_portal() {
        let store = Memory(Mutex::new(None), false);
        let result = connect_saved(&store, true, |_| async {
            panic!("must not prompt without a saved grant");
            #[allow(unreachable_code)]
            Ok(((), None))
        })
        .await
        .unwrap();
        assert!(result.is_none());
    }
    #[tokio::test]
    async fn restoration_rotates_token() {
        let store = Memory(Mutex::new(Some("old".into())), false);
        let result = connect_saved(&store, true, |token| async move {
            assert_eq!(token.as_deref(), Some("old"));
            Ok((42, Some("new".into())))
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result.0, 42);
        assert!(result.1.is_none());
        assert_eq!(store.load().await.unwrap().as_deref(), Some("new"));
    }
    #[tokio::test]
    async fn failed_restore_clears_token_without_retrying_dialog() {
        let store = Memory(Mutex::new(Some("revoked".into())), false);
        let result = connect_saved(&store, true, |_| async {
            Err::<((), Option<String>), _>(anyhow::anyhow!("revoked"))
        })
        .await;
        assert!(result.is_err());
        assert!(store.load().await.unwrap().is_none());
    }
    #[tokio::test]
    async fn session_only_response_removes_old_grant() {
        let store = Memory(Mutex::new(Some("old".into())), false);
        connect_saved(&store, false, |_| async { Ok(((), None)) })
            .await
            .unwrap();
        assert!(store.load().await.unwrap().is_none());
    }
    #[tokio::test]
    async fn storage_failure_keeps_live_session_and_reports_warning() {
        let store = Memory(Mutex::new(None), true);
        let (session, warning) =
            connect_saved(&store, false, |_| async { Ok((42, Some("new".into()))) })
                .await
                .unwrap()
                .unwrap();
        assert_eq!(session, 42);
        assert!(warning.unwrap().contains("locked keyring"));
    }
    struct Locked;
    impl TokenStore for Locked {
        async fn load(&self) -> Result<Option<String>> {
            anyhow::bail!("credential store unavailable")
        }
        async fn save(&self, _: Option<String>) -> Result<()> {
            anyhow::bail!("credential store unavailable")
        }
    }
    #[tokio::test]
    async fn startup_with_locked_store_does_not_prompt() {
        let result = connect_saved(&Locked, true, |_| async {
            panic!("startup must not prompt when token cannot be read");
            #[allow(unreachable_code)]
            Ok(((), None))
        })
        .await;
        assert!(result.is_err());
    }
    #[tokio::test]
    async fn explicit_connection_works_with_locked_store() {
        let (session, warning) = connect_saved(&Locked, false, |token| async move {
            assert!(token.is_none());
            Ok((42, Some("new".into())))
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(session, 42);
        assert!(warning.is_some());
    }
}
