use super::web_dto::ChatTicket;

#[derive(Default, Clone)]
pub(super) struct ChatContext {
    generation: u64,
    edit: u64,
}

impl ChatContext {
    pub fn select(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.edit = 0;
    }
    pub fn edit(&mut self) {
        self.edit = self.edit.wrapping_add(1);
    }
    pub fn ticket(&self) -> ChatTicket {
        ChatTicket {
            generation: self.generation,
            edit: self.edit,
        }
    }
    pub fn accepts(&self, ticket: ChatTicket) -> bool {
        self.generation == ticket.generation
    }
    pub fn clears_draft(&self, ticket: ChatTicket) -> bool {
        self.accepts(ticket) && self.edit == ticket.edit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pcm2003_unit_001_returning_to_same_pet_rejects_previous_selection_response() {
        let mut context = ChatContext::default();
        context.select();
        let old = context.ticket();
        context.select();
        context.select();
        assert!(!context.accepts(old));
        assert!(context.accepts(context.ticket()));
    }
    #[test]
    fn pcm2003_unit_002_completed_send_preserves_newly_edited_draft() {
        let mut context = ChatContext::default();
        let sent = context.ticket();
        assert!(context.clears_draft(sent));
        context.edit();
        assert!(context.accepts(sent));
        assert!(!context.clears_draft(sent));
    }
}
