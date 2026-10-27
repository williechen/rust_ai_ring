use leptos::prelude::*;
use pet_domain::PetSnapshot;

/// Ignore stale async IPC and event snapshots; equal revision remains idempotent.
fn accepts_revision(current: Option<u64>, incoming: u64) -> bool {
    current.map(|revision| incoming >= revision).unwrap_or(true)
}

pub fn apply_snapshot(
    incoming: PetSnapshot,
    snapshot: ReadSignal<Option<PetSnapshot>>,
    set_snapshot: WriteSignal<Option<PetSnapshot>>,
) {
    if accepts_revision(
        snapshot.get_untracked().map(|s| s.revision),
        incoming.revision,
    ) {
        set_snapshot.set(Some(incoming));
    }
}

#[cfg(test)]
mod tests {
    use super::accepts_revision;

    #[test]
    fn snapshot_order_preserves_existing_ipc_and_event_assertions() {
        assert!(accepts_revision(None, 0));
        assert!(accepts_revision(Some(7), 7));
        assert!(accepts_revision(Some(7), 8));
        assert!(!accepts_revision(Some(7), 6));
    }
}
