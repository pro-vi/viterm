# termwiz: keep the attribute reset of a blank line in `Line::changes`

When the only change `Line::changes()` queued for a line is an `AllAttributes`
followed by nothing but spaces, it drops that change and, in its comment's
words, returns just the line clearing operation. That clear is only emitted for
a non-default background, so with the default background the line produced no
changes at all. The two callers that carry attributes from one line to the
next, `lines_to_escapes()` and `Surface::repaint_all()`, pass the attributes of
each line's last cell as the starting attributes of the next line, and went on
as if the change had been written.

A line of plain spaces after a line ending in an underlined cell therefore left
the underline on, and plain text at the start of the next line came out
underlined. In a pane running

    printf '  para one ends \033[4mshort\033[24m\r\n     \r\n  plain words \033[4mnext\033[24m done\r\n'; sleep 60

`wezterm cli get-text --escapes --start-line 0 --end-line 2` printed the third
line as `  plain words next\e(B\e[0m done`, with no reset in front of it, while
`--start-line 2 --end-line 2` printed it correctly, with only `next`
underlined. The pane's cells were plain; only the escape output was wrong.
`pane:get_lines_as_escapes()` and `window:get_selection_escapes_for_pane()` use
the same function. `Surface::repaint_all()` does the same to a blank line after
a line whose last column is styled.

The change removes the pruning. With a non-default background it saved no
output: for a blank line with a red background after a plain line, and after an
underlined one, `lines_to_escapes()` returns the same bytes with and without the
pruning, because the `ClearToEndOfLine` that follows sets the same attributes in
`TerminfoRenderer`. With the default background it lost the reset. A blank line
now always emits its attribute change, and the doc comment on `Line::changes()`
states what its callers rely on. A repaint whose last lines are blank after such
a line also ends with that reset, and `dont_lose_first_char_on_attr_change` is
updated for it. No other expected output changed.

`repaint_all()` does not end by restoring the surface's own current attributes,
before this change or after it, so a delta rendered after a repaint starts from
whatever the last line left. In the one case where that used to match by
coincidence (a styled last column followed by blank lines, with those styles
still current), it now starts from the reset instead. Ending `repaint_all()`
with `AllAttributes` of the surface's attributes would settle that for every
case; it is left out here to keep this change to the one fault.

Tests:

- `wezterm-surface`: `dont_carry_attrs_over_blank_line` repaints an underlined
  full-width line, a blank line and a plain line, and expects the reset on the
  blank line.
- `termwiz-funcs`: `escapes_reset_attrs_on_blank_line` passes the three lines
  above to `lines_to_escapes()` and compares the whole escape string. Without
  the change it returns the string `get-text` printed above.

Both tests fail without the change and pass with it.
`cargo fmt --all -- --check` passes, and so does
`cargo test -p wezterm-surface -p termwiz-funcs -p termwiz`.

An AI coding agent (Claude Code) traced the cause, wrote the change and the
tests, and ran them. The `get-text` output above was captured by another AI
agent session, on a build whose copies of these files match upstream `main`.
Nothing was tested by hand. No upstream submission has been made.
