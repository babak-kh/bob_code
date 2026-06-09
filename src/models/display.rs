/// Discriminates the visual role of a message segment in the response area.
///
/// Used by [`ResponseBlock::block_kind`] to decide whether consecutive
/// streaming chunks should be merged into one block.
#[derive(Clone, Debug, PartialEq)]
pub enum MessageKind {
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
}
