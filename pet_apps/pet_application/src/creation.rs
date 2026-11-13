use serde::Deserialize;
use std::future::Future;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CreatedPet {
    pub id: String,
    pub name: String,
    pub species: String,
}

/// 建立與讀回清單的邊界不提供出籠操作。
pub trait PetCreation {
    fn create(&self, name: String) -> impl Future<Output = Result<CreatedPet, String>>;
    fn list(&self) -> impl Future<Output = Result<Vec<CreatedPet>, String>>;
}

pub struct CreationResult {
    pub pet: CreatedPet,
    pub listing: Result<Vec<CreatedPet>, String>,
}

/// 建立一經提交便保留身分；清單失敗不得退回成建立失敗。
pub async fn create_caged(
    store: &impl PetCreation,
    name: String,
) -> Result<CreationResult, String> {
    let name = crate::normalize_new_pet_name(&name)?;
    let pet = store.create(name).await?;
    let listing = store.list().await;
    Ok(CreationResult { pet, listing })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::RefCell,
        task::{Context, Poll, Waker},
    };
    struct Store {
        calls: RefCell<Vec<String>>,
        fail_create: bool,
        fail_list: bool,
    }
    impl PetCreation for Store {
        async fn create(&self, name: String) -> Result<CreatedPet, String> {
            self.calls.borrow_mut().push(format!("create:{name}"));
            if self.fail_create {
                return Err("無法建立".into());
            }
            Ok(CreatedPet {
                id: "committed-id".into(),
                name,
                species: "tortoise".into(),
            })
        }
        async fn list(&self) -> Result<Vec<CreatedPet>, String> {
            self.calls.borrow_mut().push("list".into());
            if self.fail_list {
                Err("讀回失敗".into())
            } else {
                Ok(vec![])
            }
        }
    }
    fn store(fail_create: bool, fail_list: bool) -> Store {
        Store {
            calls: RefCell::new(vec![]),
            fail_create,
            fail_list,
        }
    }
    fn ready<T>(future: impl Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("測試儲存介面不應等待外部 I/O"),
        }
    }
    #[test]
    fn refresh_failure_keeps_committed_identity_without_recreating() {
        let s = store(false, true);
        let result = ready(create_caged(&s, " 小龜 ".into())).unwrap();
        assert_eq!(result.pet.id, "committed-id");
        assert_eq!(result.pet.name, "小龜");
        assert!(result.listing.is_err());
        assert_eq!(*s.calls.borrow(), ["create:小龜", "list"]);
    }
    #[test]
    fn rejected_input_or_failed_create_never_reads_or_changes_selection() {
        let s = store(false, false);
        assert!(ready(create_caged(&s, "龜\n".into())).is_err());
        assert!(s.calls.borrow().is_empty());
        let s = store(true, false);
        assert!(ready(create_caged(&s, "龜".into())).is_err());
        assert_eq!(*s.calls.borrow(), ["create:龜"]);
    }
    #[test]
    fn successful_creation_reads_once_and_preserves_wire_identity() {
        let s = store(false, false);
        let result = ready(create_caged(&s, "龜".into())).unwrap();
        assert_eq!(result.pet.species, "tortoise");
        assert!(result.listing.is_ok());
        assert_eq!(*s.calls.borrow(), ["create:龜", "list"]);
        let decoded: CreatedPet =
            serde_json::from_str(r#"{"id":"id","name":"龜","species":"tortoise"}"#).unwrap();
        assert_eq!(decoded.id, "id");
    }
}
