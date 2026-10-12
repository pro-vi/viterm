# Tab bar themes: the DAG

Goal: ViTerm can switch its tab bar between four looks (Strip, Pills, Blades,
Status line) from a key, on the engine this machine runs. The critical path
runs through the engine unit's merge into `build` and ends at two approvals
that wait on the owner and on bootstrap's other session.

Settled, 2026-10-12:

- "ofc we should build a config for the tab custom theme and keep these 3, and
  think of 1 more" — the fourth is Status line.
- "/architect if ever needed.. you may /tongchou and just go onto it"
- Design: `2026-10-12-001-feat-fancy-tab-bar-style-plan.md` (C1 is the
  contract between the engine and the presets).

```
1 mockup ──→ 2 plan (C1) ─┬─→ 3 unit build ─→ 4 gate + commit ─→ 5 join build ─┐
                          └─→ 6 presets (bootstrap worktree) ──────────────────┼─→ 7 look ─→ 8 go live
                                                                               ┘
```

| # | Node | Kind | Owner | Runs with | Waits on | Done when |
|---|------|------|-------|-----------|----------|-----------|
| 1 | four looks mocked | direct | here | ui-sketch | — | `/tmp/ui-sketch-tabbar-themes.html` |
| 2 | option design | architect | here | /architect | 1 | the plan file exists |
| 3 | `unit/fancy-tab-bar-style` | architect | here | cargo | 2 | fmt and the U1/U2 tests pass on the unit branch |
| 4 | gate and commit | check | here | /gate | 3 | one commit on the unit branch, gate findings addressed or reported |
| 5 | join `build` | direct | here | git merge --no-ff | 4 | merge commit, fit tests pass, `PATCHES.md` row |
| 6 | presets and key | architect | here | bootstrap worktree | 2 | the spec passes in the worktree |
| 7 | look at each preset | check | here, then the owner | throwaway GUI | 5, 6 | the owner has seen the four looks |
| 8 | go live | direct | the owner | deploy, merge | 7, bootstrap's session commits `configs/wezterm.lua` | engine installed, presets on bootstrap master |

Start now: 3 and 6. Needs you: 8 (two approvals), and 7's look, since this
process may not capture the screen.

Fog: none past 7; whether 5's conflicts need the fit measurement changed is a
verify-at-contact item in the plan (U4).

Sources: `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs`, `box_model.rs`,
`corners.rs`, `config/src/color.rs` at `cab251610`; bootstrap `git status` and
`git log` at 2026-10-12 05:00.
