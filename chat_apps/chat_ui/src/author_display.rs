use chat_domain::MessageAuthor;

pub(super) fn author_label(author: &MessageAuthor) -> String {
    match author {
        MessageAuthor::AnonymousUser => "訪客".to_string(),
        MessageAuthor::User { user_id } => format!("會員 {}", user_id),
        MessageAuthor::Ai { persona_id } => format!("AI {}", persona_id),
        MessageAuthor::System => "系統".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_author_labels_are_preserved() {
        assert_eq!(author_label(&MessageAuthor::AnonymousUser), "訪客");
        assert_eq!(
            author_label(&MessageAuthor::User {
                user_id: "u".into()
            }),
            "會員 u"
        );
        assert_eq!(
            author_label(&MessageAuthor::Ai {
                persona_id: "a".into()
            }),
            "AI a"
        );
        assert_eq!(author_label(&MessageAuthor::System), "系統");
    }
}
