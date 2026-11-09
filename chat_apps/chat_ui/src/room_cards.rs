use crate::room_list_state::RoomListItem;
use leptos::prelude::*;

/// 卡片只呈現服務端回傳的名稱與識別，不推測人數或房間權限。
#[component]
pub(crate) fn RoomCards(rooms: Vec<RoomListItem>, active_room: String) -> impl IntoView {
    view! {
        <ul class="room-card-grid" role="list">
            {rooms.into_iter().map(|RoomListItem { id, name, category_label }| {
                let heading_id = format!("room-card-heading-{id}");
                let label_id = category_label.as_ref().map(|_| format!("room-card-category-{id}"));
                let heading_ref = heading_id.clone();
                let label_ref = label_id.clone();
                let current = (id == active_room).then_some("page");
                view! {
                    <li>
                        <article class="room-card">
                            <a href=format!("/room/{id}") rel="external" aria-current=current
                                aria-labelledby=heading_ref aria-describedby=label_ref>
                                <span class="room-card-icon" aria-hidden="true">
                                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
                                        <path d="M10 4 7 20m10-16-3 16M5 9h14M4 15h14" />
                                    </svg>
                                </span>
                                <div class="room-card-info">
                                    <h3 id=heading_id>{name}</h3>
                                {category_label.map(|label| view! { <span id=label_id class="room-category">{label}</span> })}
                                </div>
                                <span class="room-card-arrow" aria-hidden="true">
                                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
                                        <path d="M6 12h12m-5-5 5 5-5 5" />
                                    </svg>
                                </span>
                            </a>
                        </article>
                    </li>
                }
            }).collect_view()}
        </ul>
    }
}
