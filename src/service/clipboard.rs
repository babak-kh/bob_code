use copypasta::{ClipboardContext, ClipboardProvider};

/// Read the system clipboard. Lazily opens the clipboard on first use.
pub fn read(ctx: &mut Option<ClipboardContext>) -> Option<String> {
    if ctx.is_none() {
        *ctx = ClipboardContext::new().ok();
    }
    let ctx = ctx.as_mut()?;
    match ctx.get_contents() {
        Ok(text) => Some(text),
        Err(e) => {
            tracing::warn!("clipboard read failed: {e}");
            None
        }
    }
}
