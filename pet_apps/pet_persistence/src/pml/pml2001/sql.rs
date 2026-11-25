pub const LOCAL_IDS: &str = "SELECT id FROM pet_profile WHERE owner_id='local-owner' ORDER BY id";

pub const DEFAULTS: &str = "SELECT id,name,species FROM pet_profile WHERE owner_id='local-owner' AND COALESCE(is_default_closure,0)=0 ORDER BY id";

pub const SET_CHOICE: &str = "UPDATE pet_activity SET is_activity=(activity_type=?2),updated_at=CURRENT_TIMESTAMP WHERE pet_id=?1 AND owner_id='local-owner' AND activity_type IN ('walk','sit')";
