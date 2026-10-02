---
title: Close the mux-attached GUI pin-a-core class
objective: The live ViTerm window stays usable after a pane is added to a mux tab — no core pinned for hours, no need to split panes apart or relaunch to recover.
type: fix
status: active
date: 2026-09-20
origin: conversation
---

# Close the mux-attached GUI pin-a-core class

**Thread (cold):** Cursor thread on closing the mux GUI pin-a-core class — carry wezterm #7871 and #8131 as two independent fork units.

**Depth:** Standard.

**Naming pass:** no new or renamed architectural vocabulary. Reuse `unit/<slug>`, `build`, `PATCHES.md`, `carried`, and git `worktree` (owner: the builder; not a new engine type). Upstream names that enter with the patches (`NotifyMux`, `TabTitleContext`) stay theirs.

## Background

A mux-attached ViTerm GUI pins a core. Two producers share the tab-bar rebuild stack:

1. **Focus-echo loop.** With two panes in one mux tab, `SetFocusedPane` / `PaneFocused` bounce between GUI and mux until the active pane is split into its own tab. Inbox 2026-09-05: GUI main thread ~95% in `TabBarState::new` → `compute_tab_title`; ~1.3 GB RSS/hour; mux server idle. Inbox 2026-09-07: `activate-pane` does not break it; `move-pane-to-new-tab` does. That recovery is already the runbook; this plan does not implement a new one.
2. **Expensive `format-tab-title` marshal.** Each rebuild marshals every tab and every pane into Lua userdata on every `call_format_tab_title`, twice per tab (`TabBarState::new` pre-pass + render pass). That is the first slice of the cost; full title memo, event-loop coalesce, and OSC 0 double-alert stay on `BACKLOG.md` as `tabbar-title-memo`.

Class 1 is upstream [#7871](https://github.com/wezterm/wezterm/pull/7871) (successor of #7763). Class 2 first slice is [#8131](https://github.com/wezterm/wezterm/pull/8131). Neither is merged. Do not wait for wez.

The 2026-09-06 inbox still sketches `wezterm-gui connect human-main` as a second client. `CLAUDE.md` forbids that: the attention plugin acks markers in the focused window. This plan relaunches the **one** real ViTerm GUI.

## Requirements

- **R1.** Carry #7871 as `unit/pr-7871-focus-storm` from current `upstream/main` after fetch + fast-forward of `main`. Drop the chrono `clock` `Cargo.toml` hunk. One upstream-voice commit via `git am`. `cargo check` in that unit's worktree.
- **R2.** Carry #8131 as `unit/pr-8131-tabbar-lua-once` from the same `upstream/main`. Independent of R1. One change: marshal tabs/panes once per rebuild. Not the full `tabbar-title-memo`.
- **R3.** Merge both `--no-ff` into `build`. `PATCHES.md` rows (`carried`, files, proving tests, provenance). Drop BACKLOG `pr-7763-focus-storm` as absorbed by 7871. Leave remaining `tabbar-title-memo` on BACKLOG. Fork-only commit(s) on `build`; no engine patches committed on `build`.
- **R4.** Build `wezterm-gui` from `build`, install `~/.local/opt/viterm-engine.app`, relaunch the real ViTerm. Prove on that window: two-pane mux tab (split or wait); GUI under 10% of a core for an hour; `wezterm cli list` stable `is_active`. Human watches the live window / watcher at `~/.local/state/wezterm-storm/`.
- **R5.** Mux restart is not a prerequisite and is not in this plan. GUI binary first against the running mux. Server-half of 7871 (`sessionhandler` / `NotifyMux`) waits for a later ops step after this proof.

## Architecture Decision

**Approach:** Carry two upstream PRs as two fork units in two git worktrees, both cut from current `upstream/main`, merge both `--no-ff` into `build`, prove GUI-only on the live ViTerm window.

**Rationale (consistency, then simplicity):** This is the fork's documented carry path (`git am`, `unit/<slug>`, `carried` ledger row, prove from `build`). #7871 is take 3 of the focus-storm fix (review comments on #7763 already applied). #8131 is the marshal-once slice of #8086 and compiles alone on `upstream/main`. Two independent bases match "does this need another unit's code to compile?" — no.

**Rejected (do not reopen):**

| Rejected | Why it lost |
|---|---|
| (a) One branch with both PRs | Breaks one-change-per-unit; cannot send either upstream as it stands. |
| (b) Stacking 8131 on 7871 | They do not compile-depend. Stacking forges a fake base. |
| (c) Carrying #7763 `44a8f93` | Superseded by #7871. |
| (d) Implementing full `tabbar-title-memo` in the 8131 unit | Second (and third) changes in one unit. Memo / coalesce / OSC 0 stay on BACKLOG. |
| (e) Proving via a second GUI on `human-main` | Attention plugin acks markers. `CLAUDE.md` forbids it. |
| (f) Mux restart as a prerequisite to first proof | GUI half of 7871 is the claim under test; restart is a later ops step. |

**Existing-thing check:** Carry beat reimplementing `NotifyMux` / `TabTitleContext` locally. Two worktrees beat switching the primary `build` checkout to a unit branch (that checkout would then miss every other unit). `git am` beat a rewritten commit that drops authorship. Engine install at `~/.local/opt/viterm-engine.app` beat attaching `./target/debug/wezterm-gui connect human-main`.

**Trade-offs accepted:**

- GUI-only proof against an unpatched mux may leave the server-half of 7871 unexercised until a later restart. Inbox 2026-09-06 already marked "client hunks alone stop the loop" **unverified**; U4 is that test.
- #7871 can leave `wezterm cli list-clients` FOCUS stale for a **non-activating** client. This machine has one GUI; that side effect is out of topology. Do not attach a second GUI to measure it.
- #8131 does **not** remove per-tab `Config` → Lua conversion (the 2.15 ms/tab figure). It removes `2N×(N+M)` tab/pane userdata per rebuild. Remaining cost stays `tabbar-title-memo`.
- Merge of 8131 into `build` will conflict with `unit/tab-bar-row-status` in `wezterm-gui/src/tabbar.rs`. Resolve on the merge; do not restack 8131.

**Approval criteria:** Approving this plan means: carry #7871 and #8131 as the two named units from current `upstream/main`; prove on the one live window after engine install; hold mux restart; leave memo/coalesce/OSC 0 and live-storm recovery unimplemented here.

## High-Level Technical Design

Directional guidance for review, not an implementation specification. The patches already name the types.

### Two producers, one rebuild stack

```
paint (focused pane)
  → ClientPane.advise_focus
  → SetFocusedPane  ──RPC──►  sessionhandler
       → Tab.set_active_pane → advise_focus_change → MuxNotification::PaneFocused
       → mux.notify(PaneFocused) again          ← duplicate today
       → dispatch encodes Pdu::PaneFocused
  ← ClientPane / frontend reconcile
       → focus_pane_and_containing_tab → set_active_pane
            → focus_changed(true) → advise_focus → SetFocusedPane
              (cache not pre-armed → loop when 2+ panes share a mux tab)

TermWindow: PaneFocused / title alerts → update_title_post_status
  → TabBarState::new
       → compute_tab_title × 2 × N_tabs
            → call_format_tab_title
                 → Lua sequences of all tabs + all panes   ← #8131 cost
  → invalidate → paint → advise_focus                       ← feeds the echo
```

**#7871 cuts the echo** (existing names in the PR): `NotifyMux` gates `mux.notify(PaneFocused)` inside `advise_focus_change`; `focus_pane_and_containing_tab` and the `SetFocusedPane` handler activate with `NotifyMux::No`; `ClientPane` pre-sets `focused_remote_pane_id` before reconcile so `advise_focus` does not echo; `frontend.rs` skips reconcile if the pane is gone.

**#8131 cuts the marshal** (existing names in the PR): one Lua scope in `TabBarState::new`, one `tabs` + `panes` sequence pair (`TabTitleContext`), private `build` holds today's `new` body. Twice-per-tab `compute_tab_title` stays (hover / width differ).

Same `mux` crate compiles into `wezterm-gui` and `wezterm-mux-server`. A `tab.rs` change is in both artifacts; the **running** mux process only picks it up after restart. `CODEC_VERSION` stays 45; no wire change.

### Where to apply vs where it conflicts

```
upstream/main ── am #7871 ── unit/pr-7871-focus-storm     (worktree A)
             └── am #8131 ── unit/pr-8131-tabbar-lua-once (worktree B)

build = upstream/main
      + merge unit/tab-bar-rows
      + merge unit/tab-bar-row-status   ← already touches tabbar.rs
      + merge unit/pr-7871-focus-storm  ← no tabbar.rs
      + merge unit/pr-8131-tabbar-lua-once
            expected conflict: wezterm-gui/src/tabbar.rs
            with tab-bar-row-status (LeftStatus/RightStatus rows,
            TabBarState::new left_status/right_status as &[String])
```

Phase 1 corrected an origin slip: **`unit/tab-bar-rows` does not touch `tabbar.rs`**. Its files are config + fancy/retro renderers. The conflict partner is **`unit/tab-bar-row-status`**. Apply 8131 on `upstream/main` in the unit worktree; the conflict appears at merge into `build`, not at `git am`.

## Implementation Units

### U1. Carry #7871 as `unit/pr-7871-focus-storm`

- **Goal:** Independent unit: focus-echo fix from #7871, minus `Cargo.toml`, passing `cargo check` alone on current `upstream/main`.
- **Requirements:** R1
- **Dependencies:** None
- **Files:**
  - Modify (via `git am`, not hand-edit): `mux/src/tab.rs`, `mux/src/lib.rs`, `mux/src/tmux_commands.rs`, `wezterm-client/src/pane/clientpane.rs`, `wezterm-gui/src/frontend.rs`, `wezterm-mux-server-impl/src/sessionhandler.rs`
  - Do not modify: `Cargo.toml` (chrono `clock` already at workspace `Cargo.toml`; drop that hunk)
  - Test: unit tests that land inside `mux/src/tab.rs` with the patch
- **Approach:** Keep the primary checkout on `build`. `git fetch upstream`, fast-forward local `main` **without checking it out** (`git fetch upstream main:main` when that is a fast-forward). `git worktree add -b unit/pr-7871-focus-storm <worktree-path> upstream/main`. Download the GitHub `.patch`, delete the `Cargo.toml` diff (header and hunks), `git am` so authorship stays with the PR author. If the mailbox has more than one commit, fold to **one** commit keeping that author. Commit message is upstream-voice: no ViTerm, bootstrap, this machine, or this plan. Do not `git switch main` in the primary worktree.
- **Patterns to follow:** `CLAUDE.md` adding-a-change block; `PATCHES.md` preamble (`git am`, status `carried`); merge-commit style `Merge unit/tab-bar-rows into build` (used later in U3). Closest ledger template is BACKLOG `pr-7763-focus-storm`, rewritten as 7871.
- **Test scenarios:**
  - *Happy path:* `git am` of the filtered patch applies on `upstream/main`; `cargo check` in the worktree is green; `cargo test -p mux` runs the suppress/notify tests the patch adds.
  - *Edge case:* workspace `Cargo.toml` still has chrono `clock` after the unit commit (the dropped hunk did not sneak back).
  - *Error path:* `git am` rejects on `upstream/main` after dropping `Cargo.toml` → stop (do not hand-port).
- **Verification:** Branch `unit/pr-7871-focus-storm` exists, one commit, `cargo check` green in its worktree, `Cargo.toml` untouched vs `upstream/main`.
- **Proven through:** seam is the patch itself plus `cargo test -p mux`; live echo-loop proof is U4, not this worktree.
- **Runtime evidence:** unverified — `git apply --check --exclude=Cargo.toml` of current #7871 on current `upstream/main`, then `cargo check` / `cargo test -p mux` in the worktree.
- **Checkpoint:** `auto — cargo check` (and mux tests if the patch adds them) in the 7871 worktree.
- **Do not:** build or run a GUI from this worktree; merge yet; restart mux.

### U2. Carry #8131 as `unit/pr-8131-tabbar-lua-once`

- **Goal:** Independent unit: marshal tabs/panes once per tab-bar rebuild, passing `cargo check` alone on current `upstream/main`.
- **Requirements:** R2
- **Dependencies:** None (parallel with U1)
- **Files:**
  - Modify (via `git am`): `wezterm-gui/src/tabbar.rs` only
  - Test: none in the PR — `Test expectation: none in-tree — carried patch has no test file; compile in this unit, live soak in U4`
- **Approach:** Second git worktree, branch `unit/pr-8131-tabbar-lua-once` from the **same** current `upstream/main` as U1. `git am` #8131 as-is. Do not apply on `build`. Do not restack on `unit/tab-bar-rows` or `unit/tab-bar-row-status`. Do not fold in title memo, coalesce, or OSC 0. Keep the PR's twice-per-tab `compute_tab_title` and its two documented behavior changes (in-rebuild mutation of shared `tabs`/`panes`; sequence-build failure falls back for the whole rebuild).
- **Patterns to follow:** Same carry path as U1. `call_format_tab_title` / `TabBarState::new` at `wezterm-gui/src/tabbar.rs` on `build` are the conflict surface **after** merge, not the apply base.
- **Test scenarios:**
  - *Happy path:* patch applies on `upstream/main`; `cargo check` (at least `-p wezterm-gui`) green in the worktree.
  - *Edge case:* `tab-bar-row-status` signature (`left_status: &[String]`, `TabBarItem::LeftStatus(usize)`) is **absent** on this branch — that is correct; conflict is U3's.
  - *Error path:* `cargo check` fails on 8131-alone → only then consider changing base to `unit/tab-bar-row-status`; until that failure, do not stack.
- **Verification:** Branch exists, one commit, `cargo check` green in its worktree, files limited to `wezterm-gui/src/tabbar.rs`.
- **Proven through:** compile in the unit worktree; live marshal-cost proof is U4 on `build`.
- **Runtime evidence:** unverified — `git apply --check` of current #8131 on current `upstream/main`, then `cargo check` in the worktree.
- **Checkpoint:** `auto — cargo check` in the 8131 worktree.
- **Do not:** build from this worktree; merge yet; rename the branch `unit/tabbar-title-memo`.

### U3. Merge both into `build`, ledger, BACKLOG

- **Goal:** `build` contains both units; ledger matches `git branch --list 'unit/*'`; BACKLOG no longer lists `pr-7763-focus-storm`.
- **Requirements:** R3
- **Dependencies:** U1, U2
- **Files:**
  - Modify (merge, not a direct engine commit): as brought in by the two unit merges
  - Modify (fork-only): `PATCHES.md`, `BACKLOG.md`, `CLAUDE.md`, `AGENTS.md` (identical pair: add `docs/plans/` to the fork-only exception list so this plan-doc is the same class as `PATCHES.md`)
  - Resolve during merge: `wezterm-gui/src/tabbar.rs` — keep row-status slice/row-indexed statuses; keep 8131's once-per-rebuild Lua sequences / `TabTitleContext` / private `build`
- **Approach:** In the primary checkout (`build`), merge `--no-ff` both units. Prefer 7871 first (clean), then 8131 (isolates the expected `tabbar.rs` conflict with `tab-bar-row-status`). Resolve in that 8131 `--no-ff` merge commit; do **not** rebase 8131 onto row-status to hide the conflict (that forges a fake `base`). The merge message names `tab-bar-row-status`, `wezterm-gui/src/tabbar.rs`, and what was kept from each side (row-status slice/row-indexed statuses; 8131's once-per-rebuild Lua sequences / `TabTitleContext` / private `build`). The 8131 `PATCHES.md` row notes that partner and that the resolution is in the merge commit, not on the unit branch. A later reassemble reconstructs the join; do not merge an old `build` tip back in to keep it (`git show <old-merge>:wezterm-gui/src/tabbar.rs` is a hint only). Then a separate `build:` commit (or two) for ledger + BACKLOG + the fork-only list. Engine hunks must arrive only through the merge commits.
- **Patterns to follow:** `0e7d31ae3` / `09427450c` (`Merge unit/… into build`) then `de610430e` / `716fa6dfd` (`build: record …`). PATCHES columns: `slug | branch | base | side | status | why | files | proving test`. Status `carried`. `base` is `upstream/main` for both.
- **Test scenarios:**
  - *Happy path:* `git branch --list 'unit/*'` names the two new units plus existing `tab-bar-rows` / `tab-bar-row-status`; PATCHES has a row each; `cargo check` on `build` is green (now includes row-status + both carries).
  - *Edge case:* 8131 merge conflict in `tabbar.rs` only; 7871 merge does not touch `tabbar.rs`.
  - *Error path:* conflict cannot keep both row-status behavior and 8131's once-per-rebuild sequences → stop. Do not rebase 8131 onto `tab-bar-row-status` to hide the conflict.
- **Verification:** Two `--no-ff` merges on `build`; two `carried` rows; BACKLOG `pr-7763-focus-storm` gone with an absorb note pointing at `pr-7871-focus-storm`; `tabbar-title-memo` still present and narrowed (marshal-once is the 8131 unit; remaining: input memo, event-loop coalesce, OSC 0 double-alert).
- **Proven through:** `git log --merges -2` plus PATCHES ↔ `unit/*` name match; `cargo check` on `build`.
- **Runtime evidence:** unverified until the 8131 merge conflict is actually resolved and `cargo check` on `build` is run.
- **Checkpoint:** `auto — cargo check` on `build` after both merges and the ledger commit.
- **Ledger rows to write (wording may be tightened, claims may not):**

  `pr-7871-focus-storm` | `unit/pr-7871-focus-storm` | `upstream/main` | both | carried | GUI pins a core after a pane is added to a mux tab (SetFocusedPane/PaneFocused echo). Carried from wezterm/wezterm#7871 (ankitson; successor of #7763), minus the Cargo.toml chrono clock hunk already in tree. GUI half is live against the running mux; server half waits for mux restart. | the six source files listed in U1 | With the one GUI on `human-main`, a two-pane mux tab (split or wait): GUI under 10% of a core for an hour and `wezterm cli list` snapshots show a stable `is_active` per tab (`~/.local/state/wezterm-storm/`).

  `pr-8131-tabbar-lua-once` | `unit/pr-8131-tabbar-lua-once` | `upstream/main` | gui | carried | `format-tab-title` marshalled every tab and pane on every call, twice per tab per rebuild. Carried from wezterm/wezterm#8131 (iliaal; option 2 of #8086). Not title memo, not coalesce, not OSC 0. Conflicts with `unit/tab-bar-row-status` in `wezterm-gui/src/tabbar.rs`; resolution is in the `--no-ff` merge commit, not on this unit branch. | `wezterm-gui/src/tabbar.rs` | Proven with the 7871 soak on the live window. Does not claim the config-to-Lua conversion cost is gone.

- **BACKLOG edits:**
  - Remove `### pr-7763-focus-storm`. In the Open preamble or a one-line note: absorbed by `unit/pr-7871-focus-storm` (carried #7871, not #7763 `44a8f93`).
  - Keep `tabbar-title-memo`. Narrow `why`: marshal-once is `pr-8131-tabbar-lua-once`; remaining is memoize each tab title on its inputs, coalesce title updates in one event-loop turn, and the OSC 0 double-alert (`2026-09-07-osc0-double-alert-no-title-coalescing.md`). The OSC-vs-own-unit question stays open.

### U4. Install GUI from `build`, prove on the live window

- **Goal:** The daily ViTerm window no longer pins a core after a two-pane mux tab exists, without restarting the mux server.
- **Requirements:** R4, R5
- **Dependencies:** U3
- **Files:** none in the repo. Build and install from `build` per `CLAUDE.md` (release `wezterm-gui`, `ci/deploy.sh`, replace `~/.local/opt/viterm-engine.app`). Installing the app bundle may copy a new `wezterm-mux-server` binary; that does **not** restart the running server — leave the process alone.
- **Approach:** Build from `build` only. Replace the engine, relaunch the **existing** ViTerm launcher so it execs the new `wezterm-gui` with `connect human-main`. One GUI. Trigger: split a pane into a tab that already has one, or wait on an existing two-pane mux tab. Do not animate a 10 Hz OSC title in that tab (that path is remaining `tabbar-title-memo` cost and would confound the echo-loop proof). Watch CPU and `is_active` for an hour. If a storm is already live before the proof, recover with `move-pane-to-new-tab` (not in scope to implement) and then run the proof on a fresh two-pane tab.
- **Patterns to follow:** `CLAUDE.md` building/installing and "Trying a change without disturbing the running session" — throwaway GUI is only for rendering probes, unused here. Inbox 2026-09-07 recovery command stays the runbook. Do **not** follow the 2026-09-06 inbox tight loop that attaches a second `connect human-main`.
- **Test scenarios:**
  - *Happy path:* after split (or on an existing two-pane tab), GUI stays under 10% of a core for an hour; watcher captures do not trip the 70% storm path; `list1..3.json`-style polls show a stable `is_active` pane per tab.
  - *Edge case:* agent panes with slow title changes (Claude ~1 Hz) may add some CPU; they must not pin a core. A 10 Hz spinner is out of this proof.
  - *Error path / recovery:* if the core pins, `wezterm cli move-pane-to-new-tab --pane-id <one pane>` must still stop the flip; do not restart mux; record that GUI-only 7871 did not suffice.
  - *Integration:* `wezterm cli list` against `human-main.sock`; watcher `~/.local/state/wezterm-storm/`.
- **Verification:** Human judges the hour-long soak. Mux server pid unchanged across the proof.
- **Proven through:** none in-tree — live rehearsal on the daily window, gated by engine-replace consent.
- **Rollback:** restore the previous `viterm-engine.app` (or fall back to `/Applications/WezTerm.app` per launcher). Source tree: revert the two merges if abandoning the carry. Mux server is untouched, so session topology is not rolled back by this unit.
- **Runtime evidence:** unverified until the live soak — this is the test that inbox 2026-09-06 left open (GUI half vs unpatched server).
- **Checkpoint:** `gate — live window + watcher → under 10% for an hour and stable is_active: continue (plan complete); still pinning a core: record, recover with move-pane-to-new-tab, hold mux restart (not in this plan); unknown / consent refused: skip install, U1–U3 stay landed; no second GUI; no mux restart`.

  Warrant: only after U3 is there a `build` binary that contains both carries; only the human can consent to replace the daily engine and judge an hour of the real window; later mux restart depends on this proof and is explicitly out of this plan; the agent cannot supply that hour or that consent inside granted authority.

## Scope Boundaries

- Do not carry #7763 `44a8f93`.
- Do not stack the two units on each other or on `tab-bar-row-status` unless U2 `cargo check` on main fails.
- Do not implement title memo, event-loop coalesce, or OSC 0 single-alert.
- Do not implement live-storm recovery (`move-pane-to-new-tab` remains the runbook).
- Do not attach a second GUI to `human-main`.
- Do not restart the mux server in this plan.
- Do not bump `CODEC_VERSION`.
- Do not wait for wez to merge the PRs.
- Do not prove from a unit worktree binary.
- Do not commit engine patches directly on `build`.
- Do not use the 2026-09-06 inbox second-client tight loop.
- Do not chase the 2026-09-06 zoom / `WindowInvalidated` alternative producer as a unit here (2026-09-07 recurrence had no zoom; U4 records if a storm still starts).
- Do not fix #7871's `list-clients` FOCUS staleness for non-activating clients (multi-client; one GUI here).

### Deferred to Follow-Up Work

- Remaining `tabbar-title-memo` (input memo, coalesce, OSC 0): stays `BACKLOG.md` Open, narrowed in U3.
- Mux restart to activate 7871 `sessionhandler` / server-side `NotifyMux`: later ops step, after U4, not a unit in this plan. Same class as BACKLOG's original "server half goes live only when the mux server is restarted".
- #7871 `list-clients` FOCUS tracking for non-activating clients: record in the PATCHES `why` for `pr-7871-focus-storm`; no unit until a second viewer is a workflow (same topology decision as deferred `viewer-selection-isolation`).
- Zoom-as-producer discriminating tests from inbox 2026-09-06: only if U4 storms without a two-pane `is_active` flip.

## System-Wide Impact

- **Interaction graph:** Paint → `advise_focus` → `SetFocusedPane` → sessionhandler → `PaneFocused` → client reconcile / `TermWindow` title rebuild → `TabBarState::new` → `format-tab-title`. Attention plugin remains the Lua handler; this plan does not change bootstrap config. `human-main` mux process stays the January-or-current server until a later restart.
- **Error propagation:** #8131 sequence-build failure logs and falls back to built-in titles for that rebuild (PR behavior). #7871 frontend dead-pane guard logs instead of spawning reconcile. `git am` reject is a stop, not a silent skip.
- **State lifecycle risks:** Focus is in-memory (`ClientInfo.focused_pane_id`, client `focused_remote_pane_id`), not durable. Pre-setting the cache breaks the echo and also the incidental multi-client FOCUS tracking — accepted. Replacing `viterm-engine.app` does not rewrite mux session state. A failed soak must not be "fixed" by restarting mux inside this plan.
- **API surface parity:** No new config option, no new Lua method, no codec field. Launchers do not gain `--config` flags.
- **Integration coverage:** `cargo test -p mux` for 7871 tests; live GUI soak for the echo and marshal. No in-tree test for `call_format_tab_title` cost.
- **Unchanged invariants:** `CODEC_VERSION` 45; `tab_bar_rows` / per-row status Lua (`set_left_status` / `set_right_status` optional row); one GUI on `human-main`; mux unix socket and session topology; default `format-tab-title` callback arguments (`tab`, `tabs`, `panes`, config, hover, max width) — 8131 shares the `tabs`/`panes` tables within a rebuild but does not change the callback signature.

## Build Execution Contract

- **Closed decisions:** Carry #7871 not #7763. Two units, exact branch names `unit/pr-7871-focus-storm` and `unit/pr-8131-tabbar-lua-once`, both from current `upstream/main`, not stacked. `git am`, drop chrono `clock` hunk. Merge `--no-ff` into `build`. Ledger `carried`; drop BACKLOG `pr-7763-focus-storm` as absorbed; keep remaining `tabbar-title-memo`. Prove GUI-only on the real window. No second GUI. No mux restart in this plan. Recovery of a live storm remains `move-pane-to-new-tab`. Do not wait for upstream merge. Primary checkout stays on `build`. Do not build from unit worktrees. Worktrees are git worktrees, not a new type. **Join:** record the 8131/`tab-bar-row-status` `tabbar.rs` join in that `--no-ff` merge commit and on the 8131 PATCHES row; a later reassemble reconstructs it (`git show <old-merge>:wezterm-gui/src/tabbar.rs` is a hint only). Do not merge an old `build` tip to keep the resolution. Do not rebase 8131 onto row-status. Do not enable `git rerere` as policy.
- **Builder autonomy:** Worktree directory names under `~/Development/_worktrees/` (historical location). Merge order (prefer 7871 then 8131). How to strip `Cargo.toml` from the mailbox if `filterdiff` is missing. Folding a multi-commit `.patch` to one author-preserving commit. Exact `cargo check` vs `-p wezterm-gui` for U2 (must at least check `wezterm-gui`). PATCHES proving-test wording as long as it names two-pane mux tab, <10%/hour, stable `is_active`. Conflict resolution details that keep row-status slices and 8131's once-per-rebuild sequences. One vs two `build:` ledger commits.
- **Verify at contact:**
  - After `git fetch upstream`, #7871 / #8131 still unmerged on `upstream/main` → if a PR landed verbatim, skip that `git am` and retire that unit; continue the other.
  - Chrono `clock` still in workspace `Cargo.toml` → still drop the hunk; if `clock` is gone, stop (do not add it on a unit branch unless the unit cannot compile, and then it is a different change).
  - `CODEC_VERSION` still 45 and 7871 still does not bump it → if the patch now bumps codec, **stop** (would force mux restart + matching GUI; contradicts R5).
  - #8131 `git apply --check` on `upstream/main` `tabbar.rs` (not on `build`) → if apply fails on main, stop; do not "fix it" by stacking on row-status unless `cargo check` of a clean apply also fails.
  - Conflict partner at U3 is `unit/tab-bar-row-status` in `wezterm-gui/src/tabbar.rs`, **not** `unit/tab-bar-rows` (origin conversation named rows; Phase 1 corrected this). Fallback: still resolve on the `build` merge; still do not restack.
  - `git fetch upstream main:main` is a fast-forward → if not, **stop** (do not rewind `main`; never commit to `main`).
- **Stop conditions:**
  - `git am` of filtered 7871 rejects on current `upstream/main`.
  - 7871 patch bumps `CODEC_VERSION`.
  - Local `main` cannot fast-forward to `upstream/main`.
  - 8131 cannot be conflict-resolved on the `build` merge without dropping row-status behavior or 8131's once-per-rebuild sequences.
  - Need for a second GUI or a mux restart in order to continue **this** plan (those needs mean the plan is done or failed, not expanded).
- **Authority boundaries:**
  - Replace `~/.local/opt/viterm-engine.app` and relaunch ViTerm → needs consent (U4); without it, skip U4, leave U1–U3 landed.
  - Restart mux server → forbidden here; no local fallback that "is" a restart. Hold the server-half proof.
  - Second GUI on `human-main` → forbidden; throwaway GUI with its own config/socket is allowed only for rendering probes (not used in U4).
  - `move-pane-to-new-tab` on a live storm → permitted as recovery, not as the proving trigger's only path.
- **Expected gate map:**
  - U1 → `cargo check` green in 7871 worktree; mux tests green if present; `Cargo.toml` diff empty vs `upstream/main`.
  - U2 → `cargo check` green in 8131 worktree; `tabbar.rs` only.
  - U3 → `cargo check` green on `build`; merge conflicts resolved; PATCHES ↔ `unit/*`; BACKLOG as specified. Permitted temporary failure: merge conflict in `tabbar.rs` **during** the 8131 merge, before resolution.
  - U4 → hour-long soak; mux pid unchanged. Permitted failure: soak fails → record + recover; not a license to restart mux.
- **Human inventory:**
  - Replace engine + relaunch GUI → **consent**. Grant: after U3 `cargo check` on `build` is green, copy packaged app over `~/.local/opt/viterm-engine.app` and relaunch the existing ViTerm launcher (one client, `connect human-main`). Effects: GUI process replaced; mux process not restarted. Limits: no second GUI; no mux restart. Without grant: skip U4.
  - Hour-long proof → **judgment**. Probe: watcher under `~/.local/state/wezterm-storm/` plus visible window. Pass: GUI under 10% of a core for an hour on a two-pane mux tab without a 10 Hz title source, `is_active` stable. Fail: core pin → recover with `move-pane-to-new-tab`, hold mux restart, plan ends with that record. Builder prediction (pre-commit before the soak): GUI-only 7871 plus 8131 keeps CPU under 10% in that shape. Pending work while waiting: none (U4 is last). Budget: none.
  - No **content** item. No **assistance** item (no device prompt).
  - **Calibration:** "under 10% for an hour" is from inbox 2026-09-05/07 (storm 94–101% vs calm 0–2%), settled in the origin conversation. Property: distinguish core-pin from idle/title-churn baseline. Not re-asked. Worktree paths are operational, not calibration.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| GUI-only 7871 does not stop the loop against the running mux | U4 records that; recover with `move-pane-to-new-tab`; mux restart is a later ops step, not a silent expansion of this plan |
| 8131 merge conflict with row-status is larger than `new`/`build` | Resolve keeping both behaviors; stop if a behavior must be dropped; do not restack |
| Origin named `tab-bar-rows` as the conflict partner | Phase 1: it is `tab-bar-row-status`; verify at contact repeats this |
| 10 Hz title source confounds the soak | Exclude it from U4; remaining cost is `tabbar-title-memo` |
| Zoom/`WindowInvalidated` alternative producer (inbox 2026-09-06) | 2026-09-07 had no zoom; if U4 storms without `is_active` flip, record, do not invent a zoom unit here |
| `list-clients` FOCUS staleness | One GUI; out of topology; do not attach a second GUI to measure |
| Fetching a moving PR tip | Verify unmerged; am current `.patch`; if wez merged meanwhile, retire that unit |
| Building from a unit worktree | Closed: prove from `build` only |
| Inbox tight loop attaches a second GUI | Superseded by `CLAUDE.md`; U4 relaunches the one launcher |
| Attention plugin acks on focus | One GUI; no preview client on `human-main` |

## Bug-trace / confidence cross-check

| Bug / requirement (inbox) | Plan clause | Inbox expected | Match? |
|---|---|---|---|
| 2026-09-05: split into mux tab → 95% in `TabBarState::new`, 1.3 GB/h, mux idle | U1 cuts echo; U4 two-pane soak | Storm gone after GUI half, maybe without mux restart | Yes (mux restart held; U4 is the unverified claim) |
| 2026-09-05: destroyed-pane `frontend.rs` case | #7871 includes the `get_pane` guard | Carry the whole PR | Yes |
| 2026-09-06 alt producer: zoomed tab / `WindowInvalidated` | Scope: do not build a zoom unit; U4 records if storm without `is_active` flip | Discriminating test (a): GUI half, then split | Partial by design — 2026-09-07 had no zoom |
| 2026-09-06 tight loop: second `connect human-main` | U4 relaunch one engine; CLAUDE.md | Inbox is stale vs current rules | Yes — plan does not copy the tight loop |
| 2026-09-06: drop Cargo.toml chrono hunk; no codec bump | U1 + verify at contact | Same | Yes |
| 2026-09-06 unit 1 was `pr-7763-focus-storm` | Carry #7871; absorb 7763 in BACKLOG | Origin settled 7871 | Yes |
| 2026-09-07: `activate-pane` does not stop the loop | Not used as recovery or as the proof's success path | Split-apart does | Yes |
| 2026-09-07: `move-pane-to-new-tab` stops it | Scope: runbook only, not implemented | Recovery without relaunch | Yes |
| 2026-09-07: loop re-formed after GUI relaunch | U1 GUI half should prevent re-form; U4 is a relaunch | Relaunch was not sufficient then | Yes — that is why the carry exists |
| 2026-09-07: 2.15 ms/tab config→Lua; 42 conversions/rebuild | U2 is marshal-once only; memo stays BACKLOG | Inbox "first thing" was config-once | **Intentional slice** — do not claim 2.15 ms is gone |
| 2026-09-07: OSC 0 raises two alerts | Deferred with `tabbar-title-memo` | Own unit or fold into memo | Yes |
| 2026-09-07: 10 Hz spinner saturates GUI | Not U4's trigger; 8131 reduces marshal, not config cost | Separate from focus echo | Yes |
| `viewer-selection-isolation` / second GUI | Forbidden in this plan | Deferred 2026-09-20 | Yes |
| Pass: GUI <10% for an hour, stable `is_active` | U4 + PATCHES proving tests | Inbox 2026-09-06 Repro / test | Yes |

## External research

Leaned in only for PR file lists and merge status (2026-09-20): #7871 still **open** (ankitson, ~7 files, +184/−23; includes `Cargo.toml`). #8131 still **open** (iliaal, 1 file `wezterm-gui/src/tabbar.rs`, +105/−64). Local fork procedure is solid (two `--no-ff` merges already on `build`). Representation-integrity lane skipped: no codec/wire change; 7871 does not bump `CODEC_VERSION`.
