use crate::SignInPage;
use leptos::prelude::*;

#[test]
fn day28_leptos_signin_posts_to_existing_login_handler() {
    let html = view! { <SignInPage /> }.to_html();
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
fn existing_signin_is_the_single_login_form() {
    let html = view! { <SignInPage/> }.to_html();
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
