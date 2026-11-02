use leptos::prelude::*;

/// 卡片只呈現服務端回傳的名稱與識別，不推測人數或房間權限。
#[component]
pub(crate) fn RoomCards(rooms: Vec<(String, String)>, active_room: String) -> impl IntoView {
    view! {
        <ul class="room-card-grid" role="list">
            {rooms.into_iter().map(|(id, name)| {
                let current = (id == active_room).then_some("page");
                view! {
                    <li>
                        <article class="room-card">
                            <h3><a href=format!("/room/{id}") rel="external" aria-current=current>{name}</a></h3>
                        </article>
                    </li>
                }
            }).collect_view()}
        </ul>
    }
}
