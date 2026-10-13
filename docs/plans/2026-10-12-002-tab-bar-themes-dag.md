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

Landed, 2026-10-12:

- [x] 1: `/tmp/ui-sketch-tabbar-themes.html`.
- [x] 2: `d729d8b10`.
- [x] 3, 4: `unit/fancy-tab-bar-style` at `d3a8d8f28`; gate lenses /overbuild
  and /perf, five findings, all fixed before the commit.
- [x] 5: merge `32873c236`, `PATCHES.md` row `6b40302fe`. Verify at contact
  (U4): the fit measured tabs through `tab_elem`, so the marker moved there.
- [x] 6: bootstrap `feat/tab-themes` at `1735ea3d` in
  `~/Development/_worktrees/bootstrap-tab-themes`, not merged; gate lenses
  /overbuild and /comment, six findings, all fixed before the commit.
- [ ] 7: four throwaway GUIs from `build` (debug) are open for the owner,
  one per look, with the live session's 50 titles.
- [ ] 8: waits on 7 and on bootstrap's other session committing its
  `configs/wezterm.lua` edits.

Round 2, tints, 2026-10-12. The owner on the mocked paint areas: "the line
solution is really bad when there are multi rows", and "two options of tint:
the fill, because I prefer the darker color here, and the text".

- [x] `title_background_paints` (Fill, Text) and the active-tab fix: unit
  `3ac2ba13a`, merge `b5662e493`, ledger `27c67a376`; gate /perf (0.31 ms per
  rebuild at most, one fix taken) and /overbuild (one optional finding,
  declined for the fit path's repeated calls).
- [x] Bootstrap `c408f439`: a choice is a look and a tint, ten in all, picked
  with Alt+Shift+L; gate /overbuild and /comment, nine findings, all fixed.
- [ ] 7 again: eight throwaway GUIs open, each look with each tint.

Needs you: 7's look, and 8's two approvals.

Fog: none past 7; whether 5's conflicts need the fit measurement changed is a
verify-at-contact item in the plan (U4).

Sources: `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs`, `box_model.rs`,
`corners.rs`, `config/src/color.rs` at `cab251610`; bootstrap `git status` and
`git log` at 2026-10-12 05:00.
