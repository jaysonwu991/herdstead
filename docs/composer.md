# Native shell input and agent prompts

The terminal is the desktop's input surface. Type directly into the focused
shell or agent; there is no separate composer or slash-command input.

The editable shell or agent line is shown exactly as the terminal process and
its line editor render it. The desktop does not parse Markdown, restyle URLs or
paths, add link icons, or expand `@path` text. Tab completion and every other
editing key behave exactly as they do in the configured shell. Agents with their
own input UI retain their own completion and highlighting.

Use the command palette for workspace actions. For terminal and output details,
see [Desktop](desktop.md#the-shell-and-rich-terminal).
