use super::api_dto::IdleAction;
use std::future::Future;

/// 籠內狀態讀取由應用層契約隔離儲存細節。
pub trait CageQuery {
    fn read(&self, id: &str) -> impl Future<Output = Result<bool, String>> + Send;
}

pub async fn read(store: &impl CageQuery, id: &str) -> Result<bool, String> {
    store.read(id).await
}

pub const INITIAL_IDLE_SECONDS: u64 = 300;

#[derive(Clone, Debug)]
pub struct Schedule {
    pub action: IdleAction,
    pub selections: u64,
    pub deadline: u64,
}

impl Schedule {
    pub fn new(now: u64) -> Self {
        Self {
            action: IdleAction::Idle,
            selections: 0,
            deadline: now.saturating_add(INITIAL_IDLE_SECONDS),
        }
    }

    pub fn response(&mut self, now: u64) {
        *self = Self::new(now);
    }

    /// 時間只由單調時鐘提供；候選與熵由外層Adapter提供。
    pub fn poll(
        &mut self,
        now: u64,
        walk: bool,
        sit: bool,
        choice: u128,
        delay: u128,
    ) -> Option<IdleAction> {
        if now < self.deadline {
            return None;
        }
        let mut pool = vec![IdleAction::Idle, IdleAction::Sleep];
        if sit {
            pool.push(IdleAction::Sit);
        } else if walk {
            pool.push(IdleAction::Walk);
        }
        self.action = pool[(choice % pool.len() as u128) as usize];
        self.selections = self.selections.saturating_add(1);
        self.deadline = now.saturating_add((5 + (delay % 11) as u64) * 60);
        Some(self.action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pml2001_unit_001_first_selection_at_five_minutes_and_response_restarts() {
        let mut s = Schedule::new(10);
        assert_eq!(s.poll(309, true, false, 2, 0), None);
        assert_eq!(s.poll(310, true, false, 2, 0), Some(IdleAction::Walk));
        s.response(400);
        assert_eq!(s.deadline, 700);
        assert_eq!(s.selections, 0);
        assert_eq!(s.poll(699, false, true, 2, 10), None);
        assert_eq!(s.poll(700, false, true, 2, 10), Some(IdleAction::Sit));
        assert_eq!(s.deadline, 1600);
    }

    #[test]
    fn pml2001_unit_002_random_schedule_is_inclusive_five_to_fifteen_and_only_eligible_actions() {
        for seed in 0..110 {
            for (walk, sit) in [(false, false), (true, false), (false, true), (true, true)] {
                let mut s = Schedule::new(0);
                let action = s.poll(300, walk, sit, seed, seed).unwrap();
                assert!((300..=900).contains(&(s.deadline - 300)));
                if !walk || sit {
                    assert_ne!(action, IdleAction::Walk);
                }
                if !sit {
                    assert_ne!(action, IdleAction::Sit);
                }
            }
        }
    }
}
