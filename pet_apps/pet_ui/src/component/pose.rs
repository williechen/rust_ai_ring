use pet_domain::PetState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetPose {
    Idle,
    Blink,
    InteractA,
    InteractB,
    SleepA,
    SleepB,
}

impl PetPose {
    pub(super) fn css_class(self) -> &'static str {
        match self {
            Self::Idle => "pose-idle",
            Self::Blink => "pose-blink",
            Self::InteractA => "pose-interact-a",
            Self::InteractB => "pose-interact-b",
            Self::SleepA => "pose-sleep-a",
            Self::SleepB => "pose-sleep-b",
        }
    }
}

pub(super) fn project_pose(state: PetState, blinking: bool, animation_phase: bool) -> PetPose {
    match state {
        PetState::Idle => {
            if blinking {
                PetPose::Blink
            } else {
                PetPose::Idle
            }
        }
        PetState::Interacting => {
            if animation_phase {
                PetPose::InteractB
            } else {
                PetPose::InteractA
            }
        }
        PetState::Sleeping => {
            if animation_phase {
                PetPose::SleepB
            } else {
                PetPose::SleepA
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_blink_has_priority() {
        assert_eq!(project_pose(PetState::Idle, true, true,), PetPose::Blink,);
    }

    #[test]
    fn interacting_uses_animation_phase() {
        assert_eq!(
            project_pose(PetState::Interacting, false, false,),
            PetPose::InteractA,
        );

        assert_eq!(
            project_pose(PetState::Interacting, false, true,),
            PetPose::InteractB,
        );
    }

    #[test]
    fn sleeping_uses_animation_phase() {
        assert_eq!(
            project_pose(PetState::Sleeping, false, false,),
            PetPose::SleepA,
        );

        assert_eq!(
            project_pose(PetState::Sleeping, false, true,),
            PetPose::SleepB,
        );
    }

    #[test]
    fn day26_each_pose_selects_single_image_class() {
        use std::collections::HashSet;
        let all = [
            PetPose::Idle,
            PetPose::Blink,
            PetPose::InteractA,
            PetPose::InteractB,
            PetPose::SleepA,
            PetPose::SleepB,
        ];
        let classes: HashSet<&'static str> = all.iter().map(|p| p.css_class()).collect();
        assert_eq!(classes.len(), 6);
        assert!(classes.iter().all(|s| s.starts_with("pose-")));
    }
}
