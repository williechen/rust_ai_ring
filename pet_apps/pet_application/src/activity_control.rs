use serde::Deserialize;
use std::future::Future;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ActivityStatus {
    pub outside: bool,
    pub roaming: bool,
    #[serde(default)]
    pub sitting: bool,
    pub default_caged: bool,
    pub walk_label: Option<String>,
    pub sit_label: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityAction {
    Refresh,
    Walk,
    Sit,
    Open,
    Cage,
    Sleep,
    Wake,
    Find,
}

pub trait ActivityControl {
    fn query(&self, id: &str) -> impl Future<Output = Result<ActivityStatus, String>>;
    fn set_walk(
        &self,
        id: &str,
        enabled: bool,
    ) -> impl Future<Output = Result<ActivityStatus, String>>;
    fn command(&self, id: &str, action: ActivityAction)
    -> impl Future<Output = Result<(), String>>;
}

#[derive(Debug)]
pub struct ActivityOutcome {
    pub command_confirmed: bool,
    pub status: Result<ActivityStatus, String>,
}

pub async fn execute(
    store: &impl ActivityControl,
    id: &str,
    action: ActivityAction,
) -> Result<ActivityOutcome, String> {
    use ActivityAction::*;
    match action {
        Refresh => Ok(ActivityOutcome {
            command_confirmed: false,
            status: store.query(id).await,
        }),
        Walk | Sit => Ok(ActivityOutcome {
            command_confirmed: true,
            status: Ok(store.set_walk(id, action == Walk).await?),
        }),
        Open | Cage | Sleep | Wake | Find => {
            store.command(id, action).await?;
            Ok(ActivityOutcome {
                command_confirmed: true,
                status: store.query(id).await,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        pin::pin,
        task::{Context, Poll, Waker},
    };
    #[derive(Default)]
    struct Mock {
        calls: RefCell<Vec<String>>,
        fail_query: Cell<bool>,
        fail_command: Cell<bool>,
    }
    fn status() -> ActivityStatus {
        ActivityStatus {
            outside: true,
            roaming: false,
            sitting: true,
            default_caged: true,
            walk_label: None,
            sit_label: None,
        }
    }
    impl ActivityControl for Mock {
        async fn query(&self, id: &str) -> Result<ActivityStatus, String> {
            self.calls.borrow_mut().push(format!("query:{id}"));
            if self.fail_query.get() {
                Err("讀回失敗".into())
            } else {
                Ok(status())
            }
        }
        async fn set_walk(&self, id: &str, enabled: bool) -> Result<ActivityStatus, String> {
            self.calls.borrow_mut().push(format!("walk:{id}:{enabled}"));
            Ok(status())
        }
        async fn command(&self, id: &str, action: ActivityAction) -> Result<(), String> {
            self.calls.borrow_mut().push(format!("{action:?}:{id}"));
            if self.fail_command.get() {
                Err("結果未確認".into())
            } else {
                Ok(())
            }
        }
    }
    fn run<T>(future: impl Future<Output = T>) -> T {
        let mut future = pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("測試替身不應等待"),
        }
    }
    #[test]
    fn confirmed_open_with_failed_readback_can_refresh_without_opening_again() {
        let mock = Mock::default();
        mock.fail_query.set(true);
        let outcome = run(execute(&mock, "a", ActivityAction::Open)).unwrap();
        assert!(outcome.command_confirmed);
        assert!(outcome.status.is_err());
        mock.fail_query.set(false);
        let refreshed = run(execute(&mock, "a", ActivityAction::Refresh)).unwrap();
        assert!(!refreshed.command_confirmed);
        assert!(refreshed.status.unwrap().outside);
        assert_eq!(&*mock.calls.borrow(), &["Open:a", "query:a", "query:a"]);
    }
    #[test]
    fn rejected_command_does_not_query_or_repeat() {
        let mock = Mock::default();
        mock.fail_command.set(true);
        assert!(run(execute(&mock, "b", ActivityAction::Cage)).is_err());
        assert_eq!(&*mock.calls.borrow(), &["Cage:b"]);
    }
    #[test]
    fn walk_and_sit_use_returned_status_without_another_query() {
        let mock = Mock::default();
        for action in [ActivityAction::Walk, ActivityAction::Sit] {
            let result = run(execute(&mock, "b", action)).unwrap();
            assert!(result.command_confirmed && result.status.is_ok());
        }
        assert_eq!(&*mock.calls.borrow(), &["walk:b:true", "walk:b:false"]);
    }
}
