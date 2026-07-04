# babak_code — Agent Instructions

A terminal-based AI coding assistant TUI written in Rust (edition 2024).
Uses `ratatui` + `crossterm` for rendering and `tokio` for async.
The primary purpose is to assist in software development using multiple LLM backends simultaneously or in sequence.

---

## Build & Run

```bash
cargo build
cargo run          # launches the TUI
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
│   ├── mod.rs        — module declarations
│   ├── model.rs      — LLMModel trait, ChatMessageResponse, Role, ModelResponseErr
│   ├── thread.rs     — Thread, ContextItem (conversation history)
│   ├── tool.rs       — Tool, ToolCallRequest, ToolCallResponse, ToolFunction, ToolResult, ToolStructuredOutput
│   └── display.rs    — MessageKind discriminant (UI role tagging, not conversation state)
│
├── agent/            — LLM BACKEND IMPLEMENTATIONS
│   ├── mod.rs        — module declarations
│   ├── ollama/       — Ollama HTTP backend (gemma4)
│   ├── groq/         — Groq HTTP backend (gpt-oss-120b)
│   └── openrouter/   — OpenRouter HTTP backend (deepseek-r1, deepseek-v4-pro, deepseek-v4-flash)
│
├── tool/             — TOOL DEFINITIONS & EXECUTION
│   ├── mod.rs        — execute_tool dispatcher, tools_catalog
│   ├── file.rs       — read, create, edit, list_files tools
│   └── search.rs     — fd_search, rg_search tools
│
├── controller.rs     — MessageController: conversation state + model registry
├── commands.rs       — Command system: parse, handle, and CommandEffect dispatch
├── system_prompt/    — system prompt generation (injects tool catalog + AGENTS.md)
│
├── ui.rs             — display controllers: PromptController, ResponseAreaController, StatusLineController
├── prompt.rs         — ContentManager: multi-line text buffer + history
│
├── components/       — REUSABLE UI WIDGETS (stateless or self-contained)
│   ├── mod.rs        — module declarations
│   ├── markdown.rs   — markdown → ratatui Text renderer
│   ├── text_area.rs  — block-cursor TextArea widget
│   ├── collapsible.rs   — CollapsibleText: expand/collapse content blocks
│   ├── response_block.rs — ResponseBlock trait + concrete block types (streaming merge)
│   └── prompt_dialog.rs — floating modal dialog (schema-driven)
│
└── service/          — SYSTEM SERVICES
    ├── mod.rs        — module declarations
    ├── clipboard.rs  — system clipboard access (copypasta)
    ├── system_call.rs — NvidiaSmi GPU monitor
    ├── profile/      — user profile & settings persistence
    └── commands/     — tree renderer utility (conversation tree display)
```

---

## Module Responsibilities & Boundaries

### `models/` — Pure Data Contracts
Shared types only. **No HTTP, no file I/O, no business logic.**
`Thread` and `ContextItem` are dumb data bags — they hold conversation history
and nothing else. They must not know about specific model names, tool lists,
or request formats. Any conversion to a backend-specific request format belongs
in the `agent/` module, not here.

`models/display.rs` holds the `MessageKind` enum used by the response area to
discriminate block types for streaming merge. It is a display-layer concern
and does not contain conversation state.

### `agent/` — LLM Backend Implementations
Each subdirectory is one backend (Ollama, Groq, OpenRouter). Each implements the
`LLMModel` trait from `models/model.rs`. The backend owns:
- The HTTP endpoint and auth (API keys read from environment variables)
- Request construction (building the provider-specific request from a `Thread`)
- Response streaming (parsing SSE/NDJSON → `ChatMessageResponse` tokens)
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
> parameter. Tools are constructed ad-hoc inside each backend's `generate`
> method. This must be corrected — tools are owned by the `tool/` module and
> should be passed in by the controller, not built inside each backend.

The backend decides whether to include `tools` in the outgoing request. If the
model does not support native tool-calling, it silently ignores the parameter.
The controller is never aware of per-model capabilities.

### `tool/` — Tool Registry & Execution
Single source of truth for all tools. Responsibilities:
- Define tool schemas (`Tool` structs with JSON schema parameters)
- Implement tool execution logic
- Export `execute_tool(call: &ToolCallRequest) -> ToolResult` dispatcher
- Export `tools_catalog() -> Vec<ToolCatalogEntry>` for system prompt injection

`ToolResult` carries both a plain-text `text` field (for the LLM context) and an
optional `structured` field for rich TUI rendering (e.g., diff views).

Tools are **global** — they are not owned by or coupled to any specific model.
The system prompt always receives the full catalog via `system_prompt/`.

### `controller.rs` — Conversation Orchestration
`MessageController` owns:
- The thread list (`Vec<Thread>`) and the active thread index
- The model registry (`HashMap<String, Arc<dyn LLMModel + Send + Sync>>`) and active model name
- All mutations to conversation state (add user message, assistant response, tool calls)
- Tool call execution via `handle_tool_calls`
- `prepare_call()` — returns a clone of the active thread + a reference to the active model

The controller does **not** own display state. It does not know about scroll
positions, rendered text, or UI layout.

### `commands.rs` — Slash-Command System
Handles user-typed commands (e.g. `/models`, `/tree`). Commands are parsed from
`PromptEvent::Command` in `App::handle_prompt_event`:

1. `App::parse_command(name, args)` maps a command name to a `Command` variant
2. `App::handle_command(command, args)` executes it and returns a `CommandEffect`
3. Effects can be: `None`, `ResponseArea(text)` to display output, or
   `OpenDialog { schema, action }` to show a modal dialog

The prompt area (`ContentManager`) detects a leading `/` and enters command mode,
highlighting the command token. On submission, it emits `PromptEvent::Command`.

### `ui.rs` — Display State Only
`PromptController`, `ResponseAreaController`, `StatusLineController` own only
what is needed to render and scroll. They do not hold conversation history.

Streaming tokens are appended via `ResponseAreaController::add_block` using the
`ResponseBlock` trait. Consecutive blocks of the same `MessageKind` are merged
(streaming merge) to avoid per-token entries.

`ResponseAreaController` manages block-level selection (for collapse/expand) with
`[`, `]`, `Space`, and `Enter` keys when focused.

---

## Streaming Architecture

`LLMModel::generate` sends individual `ChatMessageResponse` tokens to a
`broadcast::Sender<ChatMessageResponse>`. The final chunk carries `done: true`.
`App` receives these in the `tokio::select!` loop:

1. Non-done content/thinking chunks → `ResponseAreaController::add_block` (streaming merge)
2. `tool_calls` chunk → `MessageController::set_tool_calls` → `handle_tool_calls` →
   `set_tool_call_response` → new `generate` spawned to continue the conversation
3. `done: true` → `MessageController::set_response` records the final assistant turn

Each `generate` call is spawned with `tokio::spawn` making it an independent
async unit. This is the foundation for future sub-agent and background task
support — each agent/sub-agent gets its own spawn + channel pair.

---

## Thread Model

`Thread` holds a `Vec<ContextItem>` representing the full conversation history.
`ContextItem` encodes one turn via its `role` field: system, user, assistant content,
tool request, or tool response. Use the static constructors (`ContextItem::user`,
`ContextItem::assistant`, `ContextItem::system`, `ContextItem::tool_request`,
`ContextItem::tool_response`) — never construct the struct directly.

**Future direction:** threads will have a `kind` discriminant:
- `UserThread` — user-facing, named, switchable via commands
- `AgentThread` — programmatic, short-lived, used by sub-agents

Keep this in mind when modifying `Thread` or `MessageController`. Do not
add UI-specific fields to `Thread`.

---

## Configuration & Secrets

### Environment Variables

| Variable | Purpose |
|---|---|
| `GROK_API_TOKEN` | Groq backend API token |
| `OPENROUTER_API_TOKEN` | OpenRouter API token |
| `BOB_CODE_CONFIG_PATH` | Override config directory (default: `$XDG_CONFIG_HOME/bob_code` or platform equivalent) |

### User Profile

`service/profile/` provides `UserProfile` which persists settings to a JSON file
at `{config_base_path}/settings/settings.json`. The profile stores:
- `model` — the user's preferred model (set via `/models` dialog)

Profile is loaded on startup and flushed on shutdown. The active model is initialized
from the profile on app launch.

A richer config file (TOML) for model endpoints, default model selection, and
tool subsets is the right long-term evolution. Do not design new features
assuming hardcoded values beyond what the profile already covers.

---

## Key Bindings (runtime)

| Key | Context | Action |
|-----|---------|--------|
| `Enter` | Prompt | Submit prompt |
| `Shift+Enter` | Prompt | New line in prompt |
| `Ctrl+P` | Prompt | Submit prompt (alternate) |
| `Ctrl+Shift+V` | Prompt | Paste from system clipboard |
| `Shift+Insert` / terminal paste | Prompt | Paste via bracketed paste |
| `Tab` | Global | Toggle focus between Prompt / Response panels |
| `Ctrl+D` | Global | Quit |
| `j` / `↓` | Response (focused) | Scroll one line down |
| `k` / `↑` | Response (focused) | Scroll one line up |
| `Ctrl+D` | Response (focused) | Half-page down |
| `Ctrl+U` | Response (focused) | Half-page up |
| `Ctrl+F` | Response (focused) | Full-page down |
| `Ctrl+B` | Response (focused) | Full-page up |
| `g` | Response (focused) | Jump to top |
| `G` | Response (focused) | Jump to bottom (re-enables auto-scroll) |
| `[` | Response (focused) | Select previous block |
| `]` | Response (focused) | Select next block |
| `Space` / `Enter` | Response (focused) | Toggle collapse on selected block |

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

1. Add a new variant to the `Command` enum in `src/commands.rs`
2. Add a match arm in `App::parse_command` to map the name string to the variant
3. Add a match arm in `App::handle_command` to execute the command and return
   a `CommandEffect` (`None`, `ResponseArea`, or `OpenDialog`)
4. If the command needs a dialog, define a new `DialogAction` variant and handle
   it in `App::handle_dialog_submit`

The routing (`PromptEvent::Command → App::handle_prompt_event → parse_command →
handle_command`) is already wired — no changes needed outside `commands.rs`.

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
- **Do not add ad-hoc keybindings for meta-operations** — use the commands system in `commands.rs`
- **Do not discuss architectural changes inline in code** — raise them before implementing