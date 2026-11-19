use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventInput {
    pub schema_version: u16,
    pub event_id: Uuid,
    pub source_app: String,
    pub event_type: String,
    #[serde(default)]
    pub occurred_at: Option<String>,
}

pub fn valid_event_type(value: &str) -> bool {
    analytics_domain::event_type::valid(value)
}

pub fn valid_event(event: &EventInput) -> bool {
    event.schema_version == 1
        && !event.event_id.is_nil()
        && matches!(event.source_app.as_str(), "web_chat" | "desktop_pet")
        && valid_event_type(&event.event_type)
        && event
            .occurred_at
            .as_deref()
            .is_none_or(analytics_domain::event_type::utc_timestamp)
}
