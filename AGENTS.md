# babak_code — Agent Instructions

A terminal-based AI coding assistant TUI written in Rust (edition 2024).
Uses `ratatui` + `crossterm` for rendering and `tokio` for async.
The primary purpose is to assist in software development using multiple LLM backends simultaneously or in sequence.

---

## Build & Run

```bash
cargo build
cargo run          # launches the TUI — requires Ollama at localhost:11434
cargo test         # run all tests
cargo test <name>  # run a single test
```

Logs are written to `.data/babak_code.log` (created at startup).

---

## Architecture — The Law

The architecture is the primary constraint. Every change, feature, or refactor
must respect it. If a clean implementation requires an architectural change,
**stop and discuss it first** — do not glue things around the existing structure
without considering the proper approach.

### Module Map

```
src/
├── main.rs           — bootstrap: terminal setup, logging init
├── app.rs            — event loop, wires all modules together
│
├── models/           — SHARED DATA CONTRACTS (no logic, no I/O)
│   ├── model.rs      — LLMModel trait, ChatMessageResponse, request/response types
│   ├── thread.rs     — Thread, ContextItem (conversation history)
│   └── tool.rs       — Tool, ToolCallRequest, ToolCallResponse, ToolFunction
│
├── agent/            — LLM BACKEND IMPLEMENTATIONS
│   ├── ollama/       — Ollama HTTP backend
│   └── groq/         — Groq HTTP backend
│
├── tool/             — TOOL DEFINITIONS & EXECUTION
│   ├── mod.rs        — execute_tool dispatcher, tools_catalog
│   ├── file.rs       — read, create, edit, list_files tools
│   └── search.rs     — fd_search, rg_search tools
│
├── controller.rs     — MessageController: conversation state + model registry
├── system_prompt/    — system prompt generation (injects tool catalog + AGENTS.md)
│
├── ui.rs             — display controllers (no conversation state)
├── prompt.rs         — ContentManager: multi-line text buffer + history
│
├── components/       — REUSABLE UI WIDGETS (stateless or self-contained)
│   ├── markdown.rs   — markdown → ratatui Text renderer
│   ├── text_area.rs  — block-cursor TextArea widget
│   └── prompt_dialog.rs — floating modal dialog (schema-driven)
│
└── service/          — SYSTEM SERVICES
    ├── commands/     — slash-command controller and command types
    └── system_call.rs — NvidiaSmi GPU monitor
```

---

## Module Responsibilities & Boundaries

### `models/` — Pure Data Contracts
Shared types only. **No HTTP, no file I/O, no business logic.**
`Thread` and `ContextItem` are dumb data bags — they hold conversation history
and nothing else. They must not know about specific model names, tool lists,
or request formats. Any conversion to a backend-specific request format belongs
in the `agent/` module, not here.

> ⚠️ **Known violation to fix:** `thread.rs` currently contains
> `impl Into<UserChatMessageRequest> for &Thread` which hardcodes a model name
> and tool list. This must be moved into each `LLMModel` impl. Do not copy or
> extend this pattern — fix it when touching either file.

### `agent/` — LLM Backend Implementations
Each subdirectory is one backend (Ollama, Groq, etc.). Each implements the
`LLMModel` trait from `models/model.rs`. The backend owns:
- The HTTP endpoint and auth (API keys read from environment variables)
- Request construction (building `UserChatMessageRequest` from a `Thread`)
- Response streaming (parsing newline-delimited JSON → `ChatMessageResponse` tokens)
- Tool call serialization for its specific wire format

Backends are **isolated from each other**. Adding a new backend means adding a
new subdirectory under `agent/` — nothing else changes.

### `LLMModel` Trait Contract

```rust
#[async_trait]
pub trait LLMModel {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    async fn generate(
        &self,
        thread: &Thread,
        tools: &[Tool],                          // always passed; ignore if unsupported
        resp_tx: broadcast::Sender<ChatMessageResponse>,
    );
}
```

> ⚠️ **Known violation:** `generate` currently does not accept a `tools`
> parameter. Tools are baked into the `Thread → request` conversion. This must
> be corrected — tools are owned by the `tool/` module and passed in by the
> controller, not hardcoded anywhere.

The backend decides whether to include `tools` in the outgoing request. If the
model does not support native tool-calling, it silently ignores the parameter.
The controller is never aware of per-model capabilities.

### `tool/` — Tool Registry & Execution
Single source of truth for all tools. Responsibilities:
- Define tool schemas (`Tool` structs with JSON schema parameters)
- Implement tool execution logic
- Export `execute_tool(call: &ToolCallRequest) -> String` dispatcher
- Export `tools_catalog() -> Vec<ToolCatalogEntry>` for system prompt injection

Tools are **global** — they are not owned by or coupled to any specific model.
The controller passes the full tool list (or a configured subset) to `generate`.
The system prompt always receives the full catalog via `system_prompt/`.

### `controller.rs` — Conversation Orchestration
`MessageController` owns:
- The thread list (`Vec<Thread>`) and the active thread index
- The model registry (`HashMap<String, Arc<dyn LLMModel>>`) and active model name
- All mutations to conversation state (add user message, assistant response, tool calls)
- Tool call execution via `handle_tool_calls`
- `prepare_call()` — returns a clone of the active thread + a reference to the active model

The controller does **not** own display state. It does not know about scroll
positions, rendered text, or UI layout.

### `ui.rs` — Display State Only
`PromptController`, `ResponseAreaController`, `StatusLineController` own only
what is needed to render and scroll. They do not hold conversation history.
Streaming tokens are appended via `add_to_payload`; consecutive tokens of the
same `MessageKind` are merged to avoid per-token entries.

### `service/commands/` — Slash-Command System
This module handles user-typed commands (e.g. `/model groq`, `/new`, `/clear`).
Commands are registered with `CommandController` at startup. The prompt area
detects a leading `/` and routes input to the command controller instead of the
LLM. Commands mutate `MessageController` or `App` state directly.

> ⚠️ **Currently incomplete:** `CommandController` stores commands but routing
> from the prompt is not wired up and `CommandType` variants are unused.
> Extend this module rather than adding ad-hoc keybindings for meta-operations.

---

## Streaming Architecture

`LLMModel::generate` sends individual `ChatMessageResponse` tokens to a
`broadcast::Sender<ChatMessageResponse>`. The final chunk carries `done: true`.
`App` receives these in the `tokio::select!` loop:

1. Non-done content/thinking chunks → forwarded to `ResponseAreaController`
2. `tool_calls` chunk → `MessageController::set_tool_calls` → `execute_tool` →
   `set_tool_call_response` → new `generate` spawned to continue the conversation
3. `done: true` → `MessageController::set_response` records the final assistant turn

Each `generate` call is spawned with `tokio::spawn` making it an independent
async unit. This is the foundation for future sub-agent and background task
support — each agent/sub-agent gets its own spawn + channel pair.

---

## Thread Model

`Thread` holds a `Vec<ContextItem>` representing the full conversation history.
`ContextItem` encodes one turn: system, user, assistant content, tool request,
or tool response. Use the static constructors (`ContextItem::user`,
`ContextItem::assistant`, etc.) — never construct the struct directly.

**Future direction:** threads will have a `kind` discriminant:
- `UserThread` — user-facing, named, switchable via commands
- `AgentThread` — programmatic, short-lived, used by sub-agents

Keep this in mind when modifying `Thread` or `MessageController`. Do not
add UI-specific fields to `Thread`.

---

## Configuration & Secrets

API keys are read from environment variables at model construction time:
- `GROQ_API_KEY` — Groq backend

> ⚠️ **Known violation:** the Groq API key is currently hardcoded in `app.rs`.
> Move it to `std::env::var("GROQ_API_KEY")` at the earliest opportunity.

A config file (`~/.config/babak_code/config.toml` or `.data/config.toml`) is
the right long-term home for model endpoints, default model selection, and
tool subsets. Do not design new features assuming hardcoded values.

---

## Key Bindings (runtime)

| Key | Action |
|-----|--------|
| `Enter` | Submit prompt |
| `Shift+Enter` | New line in prompt |
| `Ctrl+P` | Submit prompt (alternate) |
| `Ctrl+Shift+V` | Paste from system clipboard |
| `Shift+Insert` / terminal paste | Paste via bracketed paste |
| `Tab` | Toggle focus between Prompt / Response panels |
| `Ctrl+D` | Quit |
| `j` / `k` / arrows | Scroll response (when focused) |
| `Ctrl+D` / `Ctrl+U` | Half-page down / up |
| `Ctrl+F` / `Ctrl+B` | Full-page down / up |
| `g` / `G` | Jump to top / bottom |

---

## How to Add a New LLM Backend

1. Create `src/agent/<name>/mod.rs`
2. Define a struct and implement `LLMModel` via `#[async_trait]`
3. In `generate`: build the request from `thread` + `tools`, stream the response,
   send `ChatMessageResponse` tokens to `resp_tx`
4. Read any API key from an environment variable
5. Register the model in `app.rs` via `controller.register_model(Arc::new(...))`

Nothing else needs to change. Do not touch `thread.rs`, `tool/`, or `controller.rs`.

---

## How to Add a New Tool

1. Implement the execution function in `tool/file.rs` or `tool/search.rs`
   (or a new file under `tool/` for a new category)
2. Define the `Tool` schema (JSON schema parameters) as a public `fn <name>_tool() -> Tool`
3. Add a match arm in `tool/mod.rs::execute_tool`
4. Add an entry to `tools_catalog()` in `tool/mod.rs`

The tool is now automatically available to all models (passed via `generate`)
and described in the system prompt. No changes needed in `agent/` or `controller.rs`.

---

## How to Add a New Slash-Command

1. Define a new variant in `CommandType` (in `service/commands/mod.rs`)
2. Implement a handler that receives `&mut MessageController` (and/or `&mut App`)
3. Register it in `app.rs` via `command_controller.add_command(...)`
4. The prompt routing (detect `/`, parse command name + args, dispatch) is
   in `PromptController::handle_event` — wire it there

---

## Tests

Unit tests are not required during active development. The `components/prompt_dialog.rs`
module contains tests for the dialog widget — keep them passing.
When the architecture stabilises, tests belong closest to the logic they cover
(inline `#[cfg(test)]` modules), not in a separate `tests/` tree.

---

## What Not To Do

- **Do not add logic to `models/`** — it is a data contract layer only
- **Do not hardcode model names or tool lists outside `agent/`**
- **Do not add conversation state to `ui.rs`** — display state only
- **Do not add ad-hoc keybindings for meta-operations** — use the commands module
- **Do not copy the `Into<UserChatMessageRequest>` pattern from `thread.rs`** — it is a known violation scheduled for removal
- **Do not discuss architectural changes inline in code** — raise them before implementing
