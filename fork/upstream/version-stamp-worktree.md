# version: rerun the build script when HEAD moves, also in a worktree

`wezterm-version/build.rs` watched only the file of the ref HEAD resolved to,
looked up in the repository's own git directory. In a linked worktree that
directory holds HEAD but not the branch refs, which live in the common
directory, so nothing was watched and the reported version stayed at the commit
of whichever build first ran the script, through every later commit and branch
switch. In any checkout, switching to another branch did not rerun the script
either, because the old branch's ref did not change, and neither did moving a
detached HEAD.

The script now watches HEAD and its reflog in the worktree's own git directory,
and the checked-out branch's ref in the common one. The reflog gains an entry
on every commit, checkout and reset, which also covers a commit to a branch
whose ref was packed, where the new loose ref did not exist when the script last
ran. `packed-refs` is not watched: it changes only on `pack-refs`, `gc` or
deleting a packed ref, none of which moves HEAD. Paths that do not exist are
still skipped, because cargo reruns a build script on every build for a missing
`rerun-if-changed` path. The `.tag` override is unchanged.

Two cases stay unwatched: a packed branch in a repository that never had a HEAD
reflog (`core.logAllRefUpdates=false` from the start), and a packed branch
moved by `git update-ref` from another worktree. Watching for them would rerun
the script, and rebuild everything that depends on the crate, whenever any
branch moves.

Validation, with a script that clones the repository, checks out the commit
under test in a plain checkout and in a linked worktree of that clone, and
builds `wezterm-version` with one job after each step, comparing the
`WEZTERM_CI_TAG` the build script set with `git show` for HEAD:

| step | plain checkout, before / after | linked worktree, before / after |
|---|---|---|
| cold build | ok / ok | ok / ok |
| an empty commit | ok / ok | stale / ok |
| switch to another branch | stale / ok | stale / ok |
| pack refs, then a commit | ok / ok | stale / ok |
| detached HEAD | stale / ok | ok / ok |
| cold build with a `.tag` file | ok / ok | ok / ok |
| `.tag` removed | ok / ok | ok / ok |

"Stale" means the build reported the commit of an earlier build. Before the
change, the worktree's detached HEAD is ok only because that step checks out
the commit the stale build had kept. `cargo fmt --all -- --check` passes;
`cargo test -p wezterm-version` has no tests to run.

No cargo test fits: what changes is which files cargo watches between builds,
so the check has to drive cargo across git operations.

An AI coding agent wrote the change and the validation script, and ran it.
Nothing was tested by hand. No upstream submission has been made.
