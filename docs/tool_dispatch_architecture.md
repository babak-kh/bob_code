# Tool Dispatch Architecture Recommendations

This document outlines architectural improvements for handling tool dispatching in `src/tool/mod.rs` to move away from brittle `match` statements on function names.

## Goal

Improve the extensibility and robustness of the tool execution mechanism by decoupling the calling logic from the specific implementation details of each tool.

## Findings and Recommendations

### 1. 💡 Recommendation 1: Using a `HashMap` and `Box<dyn Fn(...)>` (Idiomatic Runtime Dispatch)

This approach uses a `HashMap` to map tool names (String) to a boxed, callable function (`Box<dyn Fn(...)>`).

*   **Mechanism:** Initialize a `HashMap<String, Box<dyn Fn(...)>>` in `execute_tool`. The map keys are the tool names.
*   **Pros:** Keeps the main `execute_tool` function body relatively clean. Adding a tool only requires adding a key-value pair to the map initialization.
*   **Cons:** Managing the exact signature and constraints of the closure type can become cumbersome with complex arguments.

### 2. 🚀 (BEST) Trait Object Dispatch Map (Recommended)

This is the most robust, object-oriented pattern for handling pluggable, runtime-defined behavior.

*   **Mechanism:**
    1.  Define a **`ToolExecutor` trait**: This trait will define a common interface for all tools, e.g., `trait ToolExecutor { fn execute(&self, args: &HashMap<String, String>) -> String; }`.
    2.  Implement this trait for every tool (e.g., `FileReader`, `Finder`).
    3.  In `execute_tool`, build a **`HashMap<String, Box<dyn ToolExecutor>>`** by registering instances.
    4.  Execution becomes a simple map lookup and call: `let executor = map.get(&name).unwrap(); executor.execute(args)`.
*   **Pros:** Highest level of decoupling. The `execute_tool` function only knows about the `ToolExecutor` trait, not about `file_read` or `find_files` specifically. Adding a new tool only requires implementing the trait and registering it.
*   **Cons:** Involves the most boilerplate upfront (defining traits and structs).

### 3. Minimal Change Refactoring

If a full refactor is too large a scope, one can wrap the existing logic into dedicated, self-contained functions (e.g., `fn handle_file_read(...)` that takes all necessary context) and then use a map to look up and call these functions based on the name. This is a slight improvement over pure `match` but still lacks the strict interface enforcement of a Trait.

## Conclusion & Action Plan

**Action:** Refactor the tool dispatch mechanism in `src/tool/mod.rs` to use the **Trait Object Dispatch Map (Recommendation 2)**.

**Follow-up Steps:**
1.  Define the `ToolExecutor` trait in `tool/mod.rs`.
2.  Create concrete types for each tool (e.g., `FileReader`) that implement `ToolExecutor`.
3.  Update `execute_tool` to build and use the `HashMap<String, Box<dyn ToolExecutor>>`.