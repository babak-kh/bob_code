/// Discriminates the visual role of a message segment in the response area.
///
/// Used by [`ResponseBlock::block_kind`] to decide whether consecutive
/// streaming chunks should be merged into one block.
#[derive(Clone, Debug, PartialEq)]
pub enum MessageKind {
    /// System prompt sent to the model (shown collapsed, tinted background).
    System,
    /// Text the user typed and submitted.
    User,
    /// Visible content returned by the assistant.
    AssistantContent,
    /// Internal thinking/reasoning trace (shown dimmed, collapsible).
    AssistantThinking,
    /// Tool call request or response payload (shown dimmed, collapsible).
    AssistantToolCall,
    /// Output from a slash-command (e.g. `/tree`).
    InfoCommandOutput,
    /// Error returned by the model backend (e.g. parse failure, API error).
    Error,
}
