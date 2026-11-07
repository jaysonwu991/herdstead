# Viewers

Click a file in Files, or Cmd/Ctrl-click an existing path in terminal output, to
open a viewer pane. Markdown uses the shared block renderer; source files use
syntax highlighting with line numbers. Folder viewers use the shared tree
renderer. The command palette can open the workspace folder tree.

Viewers scroll independently of shell scrollback. They read UTF-8 files on the
client, refuse files over 4 MiB and show read errors in the pane. Source files
containing NUL bytes are rejected. Close a viewer with its pane header button.

Inside the desktop terminal the bundled helpers use the same renderers:

```sh
herdstead cat src/main.rs
herdstead md README.md
herdstead render < README.md
herdstead tree .
```

The zsh wrappers `cat`, `md`, `hcat`, `hmd` and `tree` provide shortcuts while
preserving existing aliases and raw pipelines. See [Desktop](desktop.md).
