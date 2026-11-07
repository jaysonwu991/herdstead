# Shell input and agent prompts

The terminal is the desktop's input surface. Type directly into the focused
shell or agent; there is no separate composer or slash-command input.

In zsh, type `@` and a path prefix, then press Tab for completion. Enter expands
standalone references to quoted absolute paths. Missing references stay in the
editor with an error. References inside quoted agent prompts remain literal,
so `codex 'Review @src/main.rs'` sends the reference to the agent.

Markdown emphasis and links are styled while typing without removing their
source delimiters or changing command bytes. Cmd/Ctrl-click opens links without
submitting input. Agents with their own input UI retain their own completion.

Use the command palette for workspace actions. For shell integration details,
quoting rules and preserved aliases, see [Desktop](desktop.md#the-shell-and-rich-terminal).
