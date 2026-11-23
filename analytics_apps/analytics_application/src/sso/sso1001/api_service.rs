use std::future::Future;

pub struct SessionCredential {
    pub user_id: String,
    pub password_hash: String,
    pub fingerprint: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RenewalError {
    Unavailable,
    Unauthorized,
}

pub trait SessionRenewal {
    fn credential(
        &self,
        digest: &str,
    ) -> impl Future<Output = Result<Option<SessionCredential>, RenewalError>> + Send;
    fn refresh(
        &self,
        digest: &str,
        credential: &SessionCredential,
    ) -> impl Future<Output = Result<Option<i64>, RenewalError>> + Send;
}

/// 回應通過授權後才呼叫；寫入時仍由 Adapter 原子重驗憑證及期限。
pub async fn renew(store: &impl SessionRenewal, digest: &str) -> Result<i64, RenewalError> {
    let credential = store
        .credential(digest)
        .await?
        .ok_or(RenewalError::Unauthorized)?;
    if shared::auth::token_digest(&credential.password_hash) != credential.fingerprint {
        return Err(RenewalError::Unauthorized);
    }
    store
        .refresh(digest, &credential)
        .await?
        .ok_or(RenewalError::Unauthorized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::Mutex,
        task::{Context, Poll, Waker},
    };
    struct Store {
        valid: bool,
        revoked: bool,
        calls: Mutex<Vec<&'static str>>,
    }
    impl SessionRenewal for Store {
        async fn credential(&self, _: &str) -> Result<Option<SessionCredential>, RenewalError> {
            self.calls.lock().unwrap().push("read");
            Ok(Some(SessionCredential {
                user_id: "user".into(),
                password_hash: "hash".into(),
                fingerprint: if self.valid {
                    shared::auth::token_digest("hash")
                } else {
                    "old".into()
                },
            }))
        }
        async fn refresh(
            &self,
            _: &str,
            _: &SessionCredential,
        ) -> Result<Option<i64>, RenewalError> {
            self.calls.lock().unwrap().push("refresh");
            Ok(if self.revoked { None } else { Some(300) })
        }
    }
    fn run<T>(future: impl Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("測試介面不應等待 I/O"),
        }
    }
    #[test]
    fn changed_credential_is_rejected_without_refresh() {
        let s = Store {
            valid: false,
            revoked: false,
            calls: Mutex::new(vec![]),
        };
        assert_eq!(run(renew(&s, "digest")), Err(RenewalError::Unauthorized));
        assert_eq!(*s.calls.lock().unwrap(), ["read"]);
    }
    #[test]
    fn revocation_at_write_is_not_reported_as_success() {
        let s = Store {
            valid: true,
            revoked: true,
            calls: Mutex::new(vec![]),
        };
        assert_eq!(run(renew(&s, "digest")), Err(RenewalError::Unauthorized));
        assert_eq!(*s.calls.lock().unwrap(), ["read", "refresh"]);
    }
    #[test]
    fn database_ttl_is_returned_after_exactly_one_refresh() {
        let s = Store {
            valid: true,
            revoked: false,
            calls: Mutex::new(vec![]),
        };
        assert_eq!(run(renew(&s, "digest")), Ok(300));
        assert_eq!(*s.calls.lock().unwrap(), ["read", "refresh"]);
    }
}
