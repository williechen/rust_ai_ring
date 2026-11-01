use chat_domain::MessageAuthor;

pub(super) fn author_label(author: &MessageAuthor) -> String {
    match author {
        MessageAuthor::AnonymousUser => "You".to_string(),
        MessageAuthor::User { user_id } => format!("User {}", user_id),
        MessageAuthor::Ai { persona_id } => format!("AI {}", persona_id),
        MessageAuthor::System => "System".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_author_labels_are_preserved() {
        assert_eq!(author_label(&MessageAuthor::AnonymousUser), "You");
        assert_eq!(
            author_label(&MessageAuthor::User {
                user_id: "u".into()
            }),
            "User u"
        );
        assert_eq!(
            author_label(&MessageAuthor::Ai {
                persona_id: "a".into()
            }),
            "AI a"
        );
        assert_eq!(author_label(&MessageAuthor::System), "System");
    }
}
