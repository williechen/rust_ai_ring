use crate::SignInPage;
use leptos::prelude::*;

use leptos_router::{components::Router, location::RequestUrl};

fn render_signin() -> String {
    let owner = Owner::new();
    owner.with(|| {
        provide_context(RequestUrl::new("http://localhost/signin"));
        view! { <Router>
            <SignInPage />
        </Router> }
        .to_html()
    })
}

#[test]
fn day28_leptos_signin_posts_to_existing_login_handler() {
    let html = render_signin();
    assert!(html.contains("<form"));
    assert!(html.contains("method=\"post\""));
    assert!(html.contains("action=\"/login\""));
    assert!(html.contains("name=\"username\""));
    assert!(html.contains("name=\"password\""));
    assert!(html.contains("type=\"password\""));
    assert!(html.contains("autocomplete=\"current-password\""));
    assert!(html.contains("/signup"));
}

#[test]
fn day43_room_list_has_clear_search_control() {
    let owner = Owner::new();
    let html = owner.with(|| {
        provide_context(RequestUrl::new("http://localhost/"));
        view! { <Router><crate::RoomList /></Router> }.to_html()
    });
    assert!(html.contains("清除搜尋"));
    assert!(html.contains("aria-label=\"搜尋房間\""));
}

#[test]
fn existing_signin_is_the_single_login_form() {
    let html = render_signin();
    assert_eq!(html.matches("<form").count(), 1);
    for marker in [
        "method=\"post\"",
        "action=\"/login\"",
        "for=\"login-username\"",
        "id=\"login-username\"",
        "name=\"username\"",
        "for=\"login-password\"",
        "id=\"login-password\"",
        "name=\"password\"",
        "type=\"password\"",
        "autocomplete=\"current-password\"",
        "href=\"/signup\"",
    ] {
        assert!(html.contains(marker), "missing {marker}");
    }
}
