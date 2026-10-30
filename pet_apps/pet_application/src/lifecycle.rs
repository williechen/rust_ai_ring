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

pub async fn persist_then_close<W, F, C>(mut write: W, close_window: C) -> Result<(), String>
where
    W: FnMut(bool) -> F,
    F: std::future::Future<Output = Result<(), String>>,
    C: FnOnce() -> Result<(), String>,
{
    write(true).await?;
    if let Err(error) = close_window() {
        return match write(false).await {
            Ok(()) => Err(format!("關閉寵物視窗失敗，已恢復出籠狀態：{error}")),
            Err(rollback) => Err(format!(
                "關閉寵物視窗失敗：{error}；回復資料庫亦失敗：{rollback}"
            )),
        };
    }
    Ok(())
}

/// Preserve an exact activity snapshot on native failure; a single cage boolean
/// cannot restore sitting or simultaneous non-walking activities.
pub async fn persist_then_apply<W, WF, A, R, RF>(
    write: W,
    apply: A,
    restore: R,
) -> Result<(), String>
where
    W: FnOnce() -> WF,
    WF: std::future::Future<Output = Result<(), String>>,
    A: FnOnce() -> Result<(), String>,
    R: FnOnce() -> RF,
    RF: std::future::Future<Output = Result<(), String>>,
{
    write().await?;
    if let Err(error) = apply() {
        return match restore().await {
            Ok(()) => Err(format!("原生操作失敗，已恢復活動狀態：{error}")),
            Err(rollback) => Err(format!("原生操作失敗：{error}；活動補償失敗：{rollback}")),
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

    #[test]
    fn caging_does_not_close_when_db_write_fails() {
        complete_ready(async {
            let closed = Cell::new(false);
            let writes = RefCell::new(Vec::new());
            let result = persist_then_close(
                |flag| {
                    writes.borrow_mut().push(flag);
                    ready(Err("db".into()))
                },
                || {
                    closed.set(true);
                    Ok(())
                },
            )
            .await;
            assert!(result.is_err());
            assert!(!closed.get());
            assert_eq!(*writes.borrow(), vec![true]);
        });
    }

    #[test]
    fn native_close_failure_restores_uncaged_or_reports_rollback_failure() {
        complete_ready(async {
            for fail_rollback in [false, true] {
                let writes = RefCell::new(Vec::new());
                let result = persist_then_close(
                    |flag| {
                        writes.borrow_mut().push(flag);
                        ready(if !flag && fail_rollback {
                            Err("rollback".into())
                        } else {
                            Ok(())
                        })
                    },
                    || Err("native".into()),
                )
                .await
                .unwrap_err();
                assert_eq!(*writes.borrow(), vec![true, false]);
                assert!(result.contains("native"));
                assert_eq!(result.contains("rollback"), fail_rollback);
            }
        });
    }

    #[test]
    fn successful_cage_writes_once_then_closes_once() {
        complete_ready(async {
            let closed = Cell::new(0);
            let writes = RefCell::new(Vec::new());
            persist_then_close(
                |flag| {
                    writes.borrow_mut().push(flag);
                    ready(Ok(()))
                },
                || {
                    closed.set(closed.get() + 1);
                    Ok(())
                },
            )
            .await
            .unwrap();
            assert_eq!(*writes.borrow(), vec![true]);
            assert_eq!(closed.get(), 1);
        });
    }

    #[test]
    fn close_runs_after_persist_and_rollback_runs_after_failed_close() {
        complete_ready(async {
            let events = RefCell::new(Vec::new());
            let error = persist_then_close(
                |flag| {
                    events.borrow_mut().push(if flag {
                        "persist-caged"
                    } else {
                        "restore-uncaged"
                    });
                    ready(Ok(()))
                },
                || {
                    events.borrow_mut().push("close");
                    Err("native".into())
                },
            )
            .await
            .unwrap_err();
            assert_eq!(
                *events.borrow(),
                vec!["persist-caged", "close", "restore-uncaged"]
            );
            assert_eq!(error, "關閉寵物視窗失敗，已恢復出籠狀態：native");
        });
    }
}
