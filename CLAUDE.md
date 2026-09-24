# pro-vi/viterm — a fork that carries patches, not a fork that diverges

This checkout builds the terminal this machine runs — ViTerm is the name of
that terminal, and this fork of wezterm/wezterm is its engine. It is **not** an
upstream contribution checkout, and the workflow in `CONTRIBUTING.md` — commit to a
branch, open a pull request against wezterm/wezterm — describes upstream's
process, not this one. Read this file first.

If you are reading this, you are on the `build` branch. This file, `PATCHES.md`,
`BACKLOG.md` and `docs/plans/` exist only there, on purpose: a unit branch must
contain nothing but its own change. `.inbox/` holds local investigation notes
and is deliberately untracked.

## The four kinds of branch

| branch | holds |
|---|---|
| `main` | a pure mirror of `upstream/main`. Never commit here. |
| `unit/<slug>` | exactly one change, based on `upstream/main`, written so it could be sent upstream as it stands |
| `build` | `upstream/main` plus `git merge --no-ff` of each active unit, plus the fork-only files. **Every local build comes from here.** |
| everything else | upstream's own branches, fetched. None of them is ours. |

`PATCHES.md` is the ledger: one row per unit that has a branch, with its base,
side, status, the files it touches, why it exists and the test that proves it. A
unit without a row in that file is a unit nobody will be able to judge in six
months. `BACKLOG.md` holds the units that have no branch yet; an entry moves from
there to the ledger at the moment its branch is created, so the ledger can always
be checked against `git branch --list 'unit/*'`.

## Adding a change

The question that decides where a new change goes is **not** what it is about.
It is: *does this need another unit's code in order to compile and work?*

- **No** — and this is the usual answer, even for a change on the same subject
  as an existing unit: branch from `upstream/main`.
- **Yes** — branch from the unit it needs, and name that unit in the `base`
  column of the new unit's ledger row.
  `unit/tab-bar-row-status` is the example: there is no second row to put a
  status on without `unit/tab-bar-rows`.

```bash
git switch -c unit/<slug> upstream/main     # or off the unit it truly needs
# … one change, one commit, message written for an upstream reader:
#   no mention of bootstrap, ViTerm, this machine, or a plan …
cargo check                                  # the unit must pass on its own
git switch build
git merge --no-ff unit/<slug>
# add its row to PATCHES.md and commit that separately
```

Two rules that are easy to break by accident:

- **Do not commit to `build` directly.** A change that exists only on `build`
  can never be sent upstream and will be lost the next time `build` is
  reassembled. Fork-only files (`CLAUDE.md`, `PATCHES.md`, `BACKLOG.md`,
  `docs/plans/`) are the sole exception. Engine patches land on `build` only as
  the contents of the `--no-ff` merge commits, conflict resolution included —
  never as a follow-up commit after the merge.
- **Do not build from a unit branch.** Its binary is missing every other unit,
  so what you observe is not what the machine runs. Editing and `cargo check`
  in a unit worktree is fine; never install or prove from one.

## Merging units into `build`

`build` is `upstream/main` plus `git merge --no-ff` of each active unit, plus
the fork-only files. Units are never rebased onto `build`.

| situation | do |
|---|---|
| Independent units (both `upstream/main`) | merge in any order |
| PATCHES `base` is another unit | merge the base first |
| Same-file conflict, both based on `upstream/main` | resolve in that `--no-ff` merge; do not rebase the incoming unit onto the partner |
| Unit tip does not contain current `upstream/main` | rebase onto its PATCHES `base` before merging |

If a unit based on `upstream/main` conflicts with an already-merged unit
(`tabbar.rs` between a carried tabbar patch and `unit/tab-bar-row-status` is
the example), the merge message names the other unit, the file, and what was
kept from each side. The incoming unit's `PATCHES.md` row notes the conflict
partner and that the resolution is in the merge commit, not in the unit
branch. Rebasing the incoming unit onto the conflict partner to hide the
conflict forges a fake `base`.

## Toggling a unit

Not a runtime flag. Reassemble: reset `build` to current `upstream/main`, merge
every unit still wanted, restore fork-only files, edit `PATCHES.md` (drop or
mark the row). Rebuild the engine. A stacked leaf cannot stay if its base is
dropped. Config defaults (`tab_bar_rows = 1`) turn the behavior off; they do
not drop the merge.

Reconstruct joins on the go. Do not merge an old `build` tip back in to keep
conflict resolutions — that brings dropped units and stale main. The previous
merge commit remains reachable (`reflog`, old SHA); `git show <old-merge>:path`
is a hint only. If hunks changed, re-evaluate. Do not enable `git rerere` as
policy.

## Building and installing

```bash
cargo build -p wezterm-gui                   # the fast loop, ~15 s incremental
cargo build --release -p wezterm-gui -p wezterm -p wezterm-mux-server -p strip-ansi-escapes
bash ci/deploy.sh                            # packages WezTerm-macos-<tag>/WezTerm.app
rm -rf ~/.local/opt/viterm-engine.app
cp -R WezTerm-macos-<tag>/WezTerm.app ~/.local/opt/viterm-engine.app
rm -rf WezTerm-macos-<tag> WezTerm-macos-<tag>.zip
```

`~/.local/opt/viterm-engine.app` is the engine: the binaries ViTerm runs. It is
deliberately not in `~/Applications`, because it is not an app anyone launches —
`~/Applications/ViTerm.app` is, and that one is a small generated launcher which
execs the engine's `wezterm-gui` with `connect human-main`. Both of bootstrap's
launchers (`bin/viterm` and the wrapper generated by `scripts/viterm-install.sh`)
prefer the engine when it exists and fall back to `/Applications/WezTerm.app`
when it does not, so renaming the engine rolls the machine back to a stock
build.

Anything a unit adds that the config must ask for — a new config option, a new
Lua method — is passed by those launchers as `--config name=value`, never
written into `configs/wezterm.lua`. The mux server reads that same config file,
and an option its own binary does not know makes it fall back to a default
config and bind the default socket instead of `human-main.sock`.

## Trying a change without disturbing the running session

A throwaway GUI proves a rendering change without touching the mux server, the
attention state or any shell rc file: a config of its own that loads no plugin,
defines no unix domain, and runs `sleep` rather than a shell.

```bash
env -u WEZTERM_UNIX_SOCKET -u WEZTERM_PANE \
  ./target/debug/wezterm-gui --config-file /path/to/throwaway.lua start --class probe
```

It opens on whichever display is not showing a fullscreen space, which is
usually the built-in one. `wezterm cli list` against its own socket
(`~/.local/share/wezterm/gui-sock-<pid>`) reports its tabs and pane sizes, and
a single window can be captured with `screencapture -x -o -l<window id>` once
the id is known. Kill it by the pid you started, never by pattern.

**Do not attach a second GUI to the live `human-main` mux to preview a change.**
The attention plugin acknowledges markers in the focused window, so a second
GUI can clear an "agent is waiting" state before the human ever sees it.

## When upstream moves

```bash
git fetch upstream
git switch main && git merge --ff-only upstream/main
git rebase upstream/main unit/<each>          # each unit whose base is upstream/main
                                               # and that no other unit is based on
git rebase --update-refs upstream/main unit/<leaf>   # a stack: rebase its last unit,
                                               # the units under it move with it
git switch build
git reset --hard upstream/main                 # never merge the old build tip back in
git merge --no-ff unit/<still wanted>          # base-first only when PATCHES base is a unit
# restore fork-only files; edit PATCHES.md; reconstruct joins in the merge commits
```

A unit based on another unit must never be rebased on its own: that gives it a
private copy of its base's commit, which no longer follows the base when the
base is fixed. When a base unit is amended, move what sits on it with
`git rebase --onto unit/<base> <old tip of base> unit/<slug>`. A stacked unit
can go upstream only after its base has landed, at which point its base becomes
`upstream/main`.

`git cherry upstream/main unit/<slug>` says when a unit has landed upstream
verbatim and can be retired (for a stacked unit it lists the base's commits
first; the unit's own commit is the last line); a unit reworked upstream needs the ledger edited
by hand.

## Where the rest of the story is

- `PATCHES.md` — what each unit is and how it was proven.
- `docs/plans/` — carry and fix plans that exist only on `build`.
- `.inbox/` — the investigations behind the units, notably the 2026-09-06 note
  tracing the ViTerm freeze to the focus-echo loop and proposing the unit list.
  These are notes, not rules; this file holds the rules. They stay on this
  machine, untracked, because they name hostnames, session ids and pids.
