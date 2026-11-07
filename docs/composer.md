# The composer

The composer is the prompt line at the bottom of the screen: a place to write a
message for the focused agent, with the conveniences of a chat composer rather
than a shell prompt.

```
❯ explain @crates/render/src/markdown.rs — why does a wrapped link keep its target?
```

The bar is always visible. When it is closed it shows the hint; press `ctrl+b :`
to focus it.

## Writing

| key | effect |
|---|---|
| `Enter` | accept an open completion; otherwise send or run a slash command |
| `Shift+Enter` | insert a newline |
| `Tab` | accept the `@path` completion; with no completion, insert two spaces |
| `Esc` | close the composer and return input to the pane |
| `←` / `→` | move the caret; with `Ctrl`, by word |
| `Home` / `End` | start / end of the whole message |
| `Up` / `Down` | previous / next completion, or the previous / next line |
| `Ctrl+A` / `Ctrl+E` | start / end of the whole message |
| `Ctrl+U` / `Ctrl+K` | delete before / after the caret |
| `Ctrl+W` | delete the previous word |
| `Ctrl+P` | toggle the Markdown preview |
| `Backspace` / `Delete` | delete before / after the caret |

Up to four lines are visible as the text grows. Sending clears the text and
leaves the composer open for the next message.

The text is sent to the **focused shell pane** as typed, followed by a carriage
return. If a viewer pane has focus, or the focused pane's process has exited, the
composer says so and keeps your text instead of dropping it.

## `@path` references

Type `@` and the focused pane's folder is scanned. The popup lists matching
paths, folders with a trailing `/`, and you can keep typing to narrow it:

| key | effect |
|---|---|
| `↑` / `↓` | move through the matches |
| `Tab` or `Enter` | accept the selected path |
| `Esc` | close the composer (the completion closes with it) |

- Paths with spaces are quoted automatically: `@"docs/design notes.md"`.
- A location suffix is kept with the reference: `@src/lib.rs#L12-L14`.
- A reference that does not exist is drawn in the theme's error colour, so a typo
  is visible before you send it.
- The list is rebuilt each time the composer opens, so files created since the
  last prompt appear. Build folders in `tree-ignores` are skipped.

Relative references resolve against the focused pane's folder (which the shell
reports through OSC 7), and the agent receives the text exactly as written — so a
relative `@src/main.rs` is correct from the pane's point of view.

## Inline rendering

The composer parses inline Markdown as you type: `**bold**`, `*italic*`,
`~~strike~~`, `` `code` ``, links, bare URLs and `@mentions` all take their
style. The TUI currently renders recognised delimiters away in its display,
while retaining them in the text sent to the pane. This differs from the
desktop shell input; see [troubleshooting](troubleshooting.md#composer-markdown-markers-disappear).

`Ctrl+P` toggles the preview while composing; from a pane, `ctrl+b m` opens
the composer with preview enabled. The Markdown viewer renderer draws the text
above the input, with headings, lists, tables and highlighted code. You can keep
typing while the preview is open.

## Slash commands

Text that starts with `/` is a command for herdstead instead of a prompt. The
composer stays open afterwards.

| command | effect |
|---|---|
| `/split [h]` | split the focused pane (side by side, or stacked with `h`) |
| `/close` | close the focused pane |
| `/tab [dir]` | new tab in the current folder, or in `dir` |
| `/workspace` | new workspace |
| `/select <n>` | go to tab `n` |
| `/theme [name]` | the theme menu, or apply a named theme |
| `/open <path>` | open a file or folder in a viewer pane |
| `/tree [path]` | open a folder tree (default `.`) |
| `/cat <file>` | open a file highlighted |
| `/md <file>` | open a file rendered |
| `/rename-tab [name]` | open a rename prompt, optionally prefilled |
| `/rename-workspace [name]` | open a rename prompt, optionally prefilled |
| `/scroll-top` / `/scroll-bottom` | move the focused pane's scrollback |
| `/help` | the keybinding list |
| `/detach` | detach from the session |

A command that needs an argument says so rather than sending the line to the
agent; an unknown command says which one it did not recognise.

## Examples

```
explain what changed in @crates/app/src/tui/app.rs and why the retry loop exits early

/split h
/tree crates/render/src
/md docs/architecture.md
/theme daylight
```

Next: [viewers](viewers.md) describes what those panes show.
