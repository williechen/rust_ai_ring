/// Persistence and window closures are the boundary; no Tauri/SQLx types enter Application.
pub async fn persist_then_open<W, F, O>(
    old_caged: bool,
    mut write: W,
    open_window: O,
) -> Result<(), String>
where
    W: FnMut(bool) -> F,
    F: std::future::Future<Output = Result<(), String>>,
    O: FnOnce() -> Result<(), String>,
{
    write(false).await?;
    if let Err(error) = open_window() {
        return match write(old_caged).await {
            Ok(()) => Err(format!("開窗失敗，已恢復原回籠值：{error}")),
            Err(rollback) => Err(format!("開窗失敗：{error}；DB 補償失敗：{rollback}")),
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        future::{Future, ready},
        task::{Context, Poll, Waker},
    };
    // These fixtures use ready futures only; fail explicitly on Pending rather than pretending
    // this is a production executor.
    fn complete_ready<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("fixture unexpectedly yielded"),
        }
    }
    #[test]
    fn persist_failure_never_opens_or_closes_a_window() {
        complete_ready(async {
            let opened = Cell::new(false);
            let writes = RefCell::new(Vec::new());
            let result = persist_then_open(
                true,
                |flag| {
                    writes.borrow_mut().push(flag);
                    ready(Err("DB".into()))
                },
                || {
                    opened.set(true);
                    Ok(())
                },
            )
            .await;
            assert!(result.is_err());
            assert!(!opened.get());
            assert_eq!(*writes.borrow(), vec![false]);
        });
    }

    #[test]
    fn open_failure_restores_the_actual_old_flag() {
        complete_ready(async {
            for old in [true, false] {
                let writes = RefCell::new(Vec::new());
                let result = persist_then_open(
                    old,
                    |flag| {
                        writes.borrow_mut().push(flag);
                        ready(Ok(()))
                    },
                    || Err("native".into()),
                )
                .await;
                assert!(result.unwrap_err().contains("native"));
                assert_eq!(*writes.borrow(), vec![false, old]);
            }
        });
    }

    #[test]
    fn success_writes_once_and_rollback_error_is_visible() {
        complete_ready(async {
            let writes = RefCell::new(Vec::new());
            persist_then_open(
                true,
                |flag| {
                    writes.borrow_mut().push(flag);
                    ready(Ok(()))
                },
                || Ok(()),
            )
            .await
            .unwrap();
            assert_eq!(*writes.borrow(), vec![false]);
            let calls = Cell::new(0);
            let error = persist_then_open(
                true,
                |_| {
                    calls.set(calls.get() + 1);
                    ready(if calls.get() == 1 {
                        Ok(())
                    } else {
                        Err("rollback".into())
                    })
                },
                || Err("native".into()),
            )
            .await
            .unwrap_err();
            assert!(error.contains("native") && error.contains("rollback"));
        });
    }
}
