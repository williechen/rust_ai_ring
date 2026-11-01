use std::future::Future;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceChoice {
    Inherit,
    Allow,
    Deny,
}

impl SourceChoice {
    pub fn parse(value: &str) -> Result<Self, AccessError> {
        match value {
            "inherit" => Ok(Self::Inherit),
            "allow" => Ok(Self::Allow),
            "deny" => Ok(Self::Deny),
            _ => Err(AccessError::Invalid),
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum AccessError {
    Invalid,
    Denied,
    NotFound,
    Unavailable,
}

pub struct Actor {
    pub id: String,
    pub token_hash: String,
}

pub struct SourceChanges {
    pub chat: SourceChoice,
    pub pet: SourceChoice,
}

pub trait ViewerAccessStore {
    fn update(
        &self,
        actor: &Actor,
        target: &str,
        changes: SourceChanges,
    ) -> impl Future<Output = Result<(), AccessError>> + Send;
}

pub async fn update_sources(
    store: &impl ViewerAccessStore,
    actor: &Actor,
    target: &str,
    changes: SourceChanges,
) -> Result<(), AccessError> {
    if actor.id == target {
        return Err(AccessError::Denied);
    }
    store.update(actor, target, changes).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permission_choices_are_closed_and_inherit_is_distinct_from_deny() {
        for v in ["inherit", "allow", "deny"] {
            assert_eq!(SourceChoice::parse(v).unwrap().key(), v);
        }
        assert_eq!(SourceChoice::parse("admin"), Err(AccessError::Invalid));
        assert_ne!(SourceChoice::Inherit, SourceChoice::Deny);
    }
}
