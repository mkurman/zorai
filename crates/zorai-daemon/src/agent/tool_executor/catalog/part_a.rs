use super::*;
pub(crate) fn add_available_tools_part_a(
    tools: &mut Vec<ToolDefinition>,
    config: &AgentConfig,
    _agent_data_dir: &std::path::Path,
    _has_workspace_topology: bool,
) {
    if config.tools.bash {
        tools.push(tool_def(
            tool_names::BASH_COMMAND,
            "Execute a shell command. Non-quick work (scripts, builds, tests) runs in background and returns `operation_id`. Do not poll status; call `get_operation_status` once with `wait=true` if you need the result now, otherwise continue and this thread auto-resumes on completion.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" },
                    "rationale": { "type": "string", "description": "Why this command should run" },
                    "session": { "type": "string", "description": "Optional terminal session ID or unique substring" },
                    "cwd": { "type": "string", "description": "Optional working directory" },
                    "allow_network": { "type": "boolean", "description": "Whether network access is expected" },
                    "sandbox_enabled": { "type": "boolean", "description": "Ignored: the operator's sandbox setting for this thread decides whether commands run sandboxed" },
                    "security_level": { "type": "string", "enum": ["highest", "moderate", "lowest", "yolo"], "description": "Approval strictness level" },
                    "language_hint": { "type": "string", "description": "Optional language hint for validation" },
                    "wait_for_completion": { "type": "boolean", "description": "Wait for quick-command result (default: true); non-quick commands always background" },
                    "timeout_seconds": { "type": "integer", "description": "Quick-command wait timeout (default: 30, max: 600); larger values force background" },
                    "notify_on_completion": { "type": "boolean", "description": "When backgrounded, automatically resume this thread on completion (default: true). Set false only for fire-and-forget work you will not act on. Do not poll get_operation_status; if you need the result before continuing, call get_operation_status with wait=true." }
                },
                "required": ["command"]
            }),
        ));
    }

    if config.tools.file_operations {
        tools.push(tool_def(
            tool_names::LIST_FILES,
            "List files and directories at a given path.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to list" },
                    "session": { "type": "string", "description": "Optional terminal session ID or unique substring" },
                    "timeout_seconds": { "type": "integer", "description": "Max wait (default: 30, max: 600)" }
                },
                "required": ["path"]
            }),
        ));

        tools.push(tool_def(
            tool_names::READ_FILE,
            "Read file contents. Prefer bounded offset/limit windows over whole-file dumps.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to read" },
                    "offset": { "type": "integer", "description": "0-based starting line offset (default: 0)" },
                    "limit": { "type": "integer", "description": "Max lines to read (default: 250)" },
                    "max_lines": { "type": "integer", "description": "Deprecated alias for limit" }
                },
                "required": ["path"]
            }),
        ));

        tools.push(tool_def(
            tool_names::GET_GIT_LINE_STATUSES,
            "Report git line statuses (unchanged/modified/added) for a bounded window of a file, without parsing a full diff.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to inspect inside a git repository" },
                    "start_line": { "type": "integer", "description": "1-based starting line number for the window (default: 1)" },
                    "limit": { "type": "integer", "description": "Max current lines to inspect (default: 250, max: 500)" }
                },
                "required": ["path"]
            }),
        ));

        tools.push(tool_def(
            tool_names::WRITE_FILE,
            "Write content to a file. Accepts JSON args or a multipart-style payload (path/file parts) for large content.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to write" },
                    "content": { "type": "string", "description": "File content to write" },
                    "session": { "type": "string", "description": "Optional terminal session ID or unique substring" },
                    "timeout_seconds": { "type": "integer", "description": "Max wait (default: 30, max: 600)" }
                },
                "required": ["path", "content"]
            }),
        ));

        tools.push(tool_def(
            tool_names::CREATE_FILE,
            "Create a new file; fails if it exists unless overwrite=true. Accepts JSON args or a multipart-style payload (filename/cwd/file parts) for large content.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to create" },
                    "filename": { "type": "string", "description": "Filename to create when combined with cwd" },
                    "cwd": { "type": "string", "description": "Base directory used with filename/path for daemon-native writes" },
                    "content": { "type": "string", "description": "Initial file content" },
                    "overwrite": { "type": "boolean", "description": "Allow replacing an existing file (default: false)" }
                },
                "required": ["content"]
            }),
        ));

        tools.push(tool_def(
            tool_names::APPEND_TO_FILE,
            "Append text to the end of an existing file without rewriting the whole file.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to append to" },
                    "content": { "type": "string", "description": "Text to append" },
                    "create_if_missing": { "type": "boolean", "description": "Create the file if it does not exist (default: false)" }
                },
                "required": ["path", "content"]
            }),
        ));

        tools.push(tool_def(
            tool_names::REPLACE_IN_FILE,
            "Replace a specific text fragment inside a file; prefer this over rewriting the whole file.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to edit" },
                    "old_text": { "type": "string", "description": "Exact text to replace" },
                    "new_text": { "type": "string", "description": "Replacement text" },
                    "replace_all": { "type": "boolean", "description": "Replace every occurrence instead of exactly one (default: false)" }
                },
                "required": ["path", "old_text", "new_text"]
            }),
        ));

        tools.push(tool_def(
            tool_names::APPLY_FILE_PATCH,
            "Apply one or more exact text replacements to a file in order, for multi-hunk targeted edits.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to patch" },
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_text": { "type": "string", "description": "Exact existing text to replace" },
                                "new_text": { "type": "string", "description": "Replacement text" },
                                "replace_all": { "type": "boolean", "description": "Replace all occurrences for this edit (default: false)" }
                            },
                            "required": ["old_text", "new_text"]
                        }
                    }
                },
                "required": ["path", "edits"]
            }),
        ));

        tools.push(tool_def(
            tool_names::APPLY_PATCH,
            "Apply a harness-style patch wrapped in `*** Begin Patch`/`*** End Patch` with Add/Update/Delete File actions; Update hunks need `@@` plus `-old`/`+new` lines. For plain text replacements use `apply_file_patch`.",
            serde_json::json!({
                "type": "object",
                "minProperties": 1,
                "properties": {
                    "input": { "type": "string", "description": "Harness-style patch text (preferred)" },
                    "patch": { "type": "string", "description": "Legacy alias for `input`" },
                    "explanation": { "type": "string", "description": "Optional reason for the patch" }
                }
            }),
        ));

        tools.push(tool_def(
            tool_names::SEARCH_FILES,
            "Search files with ripgrep; returns matching lines with paths and line numbers.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Literal text by default; set regex=true for regex" },
                    "path": { "type": "string", "description": "Directory to search (default: current directory)" },
                    "file_pattern": { "type": "string", "description": "Glob filter, e.g. '*.rs'" },
                    "regex": { "type": "boolean", "description": "Treat pattern as regex (default: false)" },
                    "max_results": { "type": "integer", "description": "Max results (default: 50, max: 200)" },
                    "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max wait (default: 120, max: 600)" }
                },
                "required": ["pattern"]
            }),
        ));

        tools.push(tool_def(
            tool_names::LSP_QUERY,
            "Ask the workspace's language server (kept running between calls): compiler diagnostics for a file after you edit it, go-to-definition, type definition, implementations, references, hover type info, or workspace symbols. Faster and more exact than running a full build to check one file.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["diagnostics", "definition", "type_definition", "implementation", "references", "hover", "symbols"] },
                    "path": { "type": "string", "description": "File to ask about (absolute or relative to the working directory)" },
                    "line": { "type": "integer", "description": "1-based line (position actions)" },
                    "column": { "type": "integer", "description": "1-based column (position actions)" },
                    "query": { "type": "string", "description": "Symbol name filter for action=symbols" },
                    "wait_seconds": { "type": "integer", "description": "How long to wait for fresh diagnostics (default 20)" }
                },
                "required": ["action", "path"]
            }),
        ));

        tools.push(tool_def(
            tool_names::SEARCH_CODEBASE,
            "Semantic search over the workspace's indexed code: describe behaviour in words (e.g. 'where failed uploads are retried') and get the most relevant functions. Works when the user enabled the code index for the workspace.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What the code does, in natural language" },
                    "path": { "type": "string", "description": "Workspace directory (default: working directory)" },
                    "limit": { "type": "integer", "description": "Max results (default 8, max 30)" }
                },
                "required": ["query"]
            }),
        ));
    }

    if config.tools.system_info {
        tools.push(tool_def(
            tool_names::GET_SYSTEM_INFO,
            "Get system information: CPU, memory, disk, load average, hostname.",
            serde_json::json!({ "type": "object", "properties": {} }),
        ));

        tools.push(tool_def(
            tool_names::LIST_PROCESSES,
            "List running processes sorted by CPU usage.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "Max processes to show (default: 20)" }
                }
            }),
        ));
    }
}
