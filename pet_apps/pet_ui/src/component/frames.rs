use super::pose::PetPose;

const IDLE: &str = "data:image/svg+xml,%3Csvg%20xmlns%3D%27http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%27%20viewBox%3D%270%200%2096%2096%27%3E%3Cellipse%20cx%3D%2745%27%20cy%3D%2755%27%20rx%3D%2732%27%20ry%3D%2723%27%20fill%3D%27%235c984c%27%2F%3E%3Ccircle%20cx%3D%2771%27%20cy%3D%2750%27%20r%3D%2715%27%20fill%3D%27%239dcc70%27%2F%3E%3Ccircle%20cx%3D%2776%27%20cy%3D%2745%27%20r%3D%273%27%20fill%3D%27%23252f27%27%2F%3E%3Cpath%20d%3D%27M25%2048%20Q45%2018%2065%2048%20M44%2034%20L44%2075%20M28%2052%20L60%2052%27%20fill%3D%27none%27%20stroke%3D%27%232d5838%27%20stroke-width%3D%274%27%2F%3E%3Cpath%20d%3D%27M25%2072%20L28%2078%20M56%2072%20L53%2078%27%20fill%3D%27none%27%20stroke%3D%27%235c984c%27%20stroke-width%3D%276%27%2F%3E%3C%2Fsvg%3E";
const BLINK: &str = "data:image/svg+xml,%3Csvg%20xmlns%3D%27http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%27%20viewBox%3D%270%200%2096%2096%27%3E%3Cellipse%20cx%3D%2745%27%20cy%3D%2755%27%20rx%3D%2732%27%20ry%3D%2723%27%20fill%3D%27%235c984c%27%2F%3E%3Ccircle%20cx%3D%2771%27%20cy%3D%2750%27%20r%3D%2715%27%20fill%3D%27%239dcc70%27%2F%3E%3Cpath%20d%3D%27M72%2045%20Q76%2049%2080%2045%27%20fill%3D%27none%27%20stroke%3D%27%23252f27%27%20stroke-width%3D%273%27%2F%3E%3Cpath%20d%3D%27M25%2048%20Q45%2018%2065%2048%20M44%2034%20L44%2075%20M28%2052%20L60%2052%27%20fill%3D%27none%27%20stroke%3D%27%232d5838%27%20stroke-width%3D%274%27%2F%3E%3Cpath%20d%3D%27M25%2072%20L28%2078%20M56%2072%20L53%2078%27%20fill%3D%27none%27%20stroke%3D%27%235c984c%27%20stroke-width%3D%276%27%2F%3E%3C%2Fsvg%3E";
const INTERACT_A: &str = "data:image/svg+xml,%3Csvg%20xmlns%3D%27http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%27%20viewBox%3D%270%200%2096%2096%27%3E%3Cellipse%20cx%3D%2745%27%20cy%3D%2755%27%20rx%3D%2732%27%20ry%3D%2723%27%20fill%3D%27%235c984c%27%2F%3E%3Ccircle%20cx%3D%2771%27%20cy%3D%2748%27%20r%3D%2715%27%20fill%3D%27%239dcc70%27%2F%3E%3Ccircle%20cx%3D%2776%27%20cy%3D%2745%27%20r%3D%273%27%20fill%3D%27%23252f27%27%2F%3E%3Cpath%20d%3D%27M25%2048%20Q45%2018%2065%2048%20M44%2034%20L44%2075%20M28%2052%20L60%2052%27%20fill%3D%27none%27%20stroke%3D%27%232d5838%27%20stroke-width%3D%274%27%2F%3E%3Cpath%20d%3D%27M25%2072%20L28%2078%20M56%2072%20L53%2078%27%20fill%3D%27none%27%20stroke%3D%27%235c984c%27%20stroke-width%3D%276%27%2F%3E%3C%2Fsvg%3E";
const INTERACT_B: &str = "data:image/svg+xml,%3Csvg%20xmlns%3D%27http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%27%20viewBox%3D%270%200%2096%2096%27%3E%3Cellipse%20cx%3D%2745%27%20cy%3D%2755%27%20rx%3D%2732%27%20ry%3D%2723%27%20fill%3D%27%235c984c%27%2F%3E%3Ccircle%20cx%3D%2771%27%20cy%3D%2750%27%20r%3D%2715%27%20fill%3D%27%239dcc70%27%2F%3E%3Ccircle%20cx%3D%2776%27%20cy%3D%2745%27%20r%3D%273%27%20fill%3D%27%23252f27%27%2F%3E%3Cpath%20d%3D%27M25%2048%20Q45%2018%2065%2048%20M44%2034%20L44%2075%20M28%2052%20L60%2052%27%20fill%3D%27none%27%20stroke%3D%27%232d5838%27%20stroke-width%3D%274%27%2F%3E%3Cpath%20d%3D%27M25%2072%20L20%2078%20M56%2072%20L64%2078%27%20fill%3D%27none%27%20stroke%3D%27%235c984c%27%20stroke-width%3D%276%27%2F%3E%3C%2Fsvg%3E";
const SLEEP_A: &str = "data:image/svg+xml,%3Csvg%20xmlns%3D%27http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%27%20viewBox%3D%270%200%2096%2096%27%3E%3Cellipse%20cx%3D%2745%27%20cy%3D%2755%27%20rx%3D%2732%27%20ry%3D%2722%27%20fill%3D%27%235c984c%27%2F%3E%3Ccircle%20cx%3D%2771%27%20cy%3D%2750%27%20r%3D%2715%27%20fill%3D%27%239dcc70%27%2F%3E%3Cpath%20d%3D%27M72%2045%20Q76%2049%2080%2045%27%20fill%3D%27none%27%20stroke%3D%27%23252f27%27%20stroke-width%3D%273%27%2F%3E%3Cpath%20d%3D%27M25%2048%20Q45%2018%2065%2048%20M44%2034%20L44%2075%20M28%2052%20L60%2052%27%20fill%3D%27none%27%20stroke%3D%27%232d5838%27%20stroke-width%3D%274%27%2F%3E%3Cpath%20d%3D%27M25%2072%20L28%2078%20M56%2072%20L53%2078%27%20fill%3D%27none%27%20stroke%3D%27%235c984c%27%20stroke-width%3D%276%27%2F%3E%3C%2Fsvg%3E";
const SLEEP_B: &str = "data:image/svg+xml,%3Csvg%20xmlns%3D%27http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%27%20viewBox%3D%270%200%2096%2096%27%3E%3Cellipse%20cx%3D%2745%27%20cy%3D%2754%27%20rx%3D%2732%27%20ry%3D%2723%27%20fill%3D%27%235c984c%27%2F%3E%3Ccircle%20cx%3D%2771%27%20cy%3D%2750%27%20r%3D%2715%27%20fill%3D%27%239dcc70%27%2F%3E%3Cpath%20d%3D%27M72%2045%20Q76%2049%2080%2045%27%20fill%3D%27none%27%20stroke%3D%27%23252f27%27%20stroke-width%3D%273%27%2F%3E%3Cpath%20d%3D%27M25%2048%20Q45%2018%2065%2048%20M44%2034%20L44%2075%20M28%2052%20L60%2052%27%20fill%3D%27none%27%20stroke%3D%27%232d5838%27%20stroke-width%3D%274%27%2F%3E%3Cpath%20d%3D%27M25%2072%20L28%2078%20M56%2072%20L53%2078%27%20fill%3D%27none%27%20stroke%3D%27%235c984c%27%20stroke-width%3D%276%27%2F%3E%3C%2Fsvg%3E";

/// 每個姿態只選一張完整SVG，不疊加字形或第二張圖。
pub(crate) fn frame_uri(pose: PetPose) -> &'static str {
    match pose {
        PetPose::Idle => IDLE,
        PetPose::Blink => BLINK,
        PetPose::InteractA => INTERACT_A,
        PetPose::InteractB => INTERACT_B,
        PetPose::SleepA => SLEEP_A,
        PetPose::SleepB => SLEEP_B,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_blink_and_sleep_frames_are_distinct_from_open_eyes() {
        assert_ne!(frame_uri(PetPose::Idle), frame_uri(PetPose::Blink));
        assert_ne!(frame_uri(PetPose::Idle), frame_uri(PetPose::SleepA));
        assert_ne!(frame_uri(PetPose::SleepA), frame_uri(PetPose::SleepB));
    }
}
