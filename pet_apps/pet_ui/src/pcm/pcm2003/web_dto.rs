#[derive(Clone, Copy)]
pub(super) struct ChatTicket {
    pub generation: u64,
    pub edit: u64,
}
