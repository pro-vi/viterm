#!/bin/zsh
# Checks that the version wezterm reports follows the commit it is built from,
# in a plain checkout and in a linked worktree, without editing the version
# crate between builds.
#
#   run.sh <repo-to-clone> <commit>
#
# It clones <repo-to-clone> into a throwaway directory, checks out <commit>
# there and in a linked worktree of that clone, and in each one builds
# wezterm-version with one job after each step below, reading the tag the
# build script set (WEZTERM_CI_TAG in its output file) and comparing it with
# what `git show` says for HEAD:
#   cold build; an empty commit; a switch to another branch; refs packed, then
#   a commit; a detached HEAD; then, with the version crate cleaned out of the
#   target, a .tag file (the tag must be its contents), then the .tag file
#   removed.
# Each checkout builds into a target of its own: cargo (as of 1.94) gives the
# version crate of both checkouts one build directory in a shared target, so the
# second would read the first's state. VERSION_HARNESS_TARGET names a prefix
# for those targets to keep between runs, so the dependencies are not rebuilt
# each time; the version crate is cleaned out at each start, so it is cold.
# Prints one line per check and a count of failures, and exits 1 if any build
# failed or any tag was wrong.
set -u
src=${1:?repo to clone}
commit=${2:?commit to build}
work=$(mktemp -d "${TMPDIR:-/tmp}/version-stamp.XXXXXX")
trap 'rm -rf "$work"' EXIT
fails=0

expected() { git -C "$1" -c core.abbrev=8 show -s --format=%cd-%h --date=format:%Y%m%d-%H%M%S; }

built_tag() {
  local dir=$1 target=$2
  (cd "$dir" && CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=$target cargo build -q -p wezterm-version) || return 1
  local out=$(ls -t "$target"/debug/build/wezterm-version-*/output | head -1)
  sed -n 's/^cargo:rustc-env=WEZTERM_CI_TAG=//p' "$out"
}

check() {
  local label=$1 dir=$2 target=$3 want=$4
  local got
  got=$(built_tag "$dir" "$target") || { echo "FAIL $label: build failed"; fails=$((fails+1)); return; }
  if [[ $got == $want ]]; then
    echo "ok   $label: $got"
  else
    echo "FAIL $label: built $got, HEAD is $want"
    fails=$((fails+1))
  fi
}

git clone -q --no-checkout "$src" "$work/repo"
git -C "$work/repo" -c advice.detachedHead=false checkout -q -b probe-main "$commit"
git -C "$work/repo" worktree add -q -b probe-wt "$work/wt" "$commit"
# The branches to switch to carry the same build script as <commit>, so the
# script under test is the one that runs after the switch.
for b in probe-other probe-other-wt; do
  git -C "$work/repo" branch -q $b "$(git -C "$work/repo" -c user.name=probe -c user.email=probe@localhost commit-tree "$commit^{tree}" -p "$commit" -m $b)"
done

for kind in checkout worktree; do
  if [[ $kind == checkout ]]; then dir=$work/repo; other=probe-other; else dir=$work/wt; other=probe-other-wt; fi
  target=${VERSION_HARNESS_TARGET:-$work/target}-$kind
  g() { git -C "$dir" -c user.name=probe -c user.email=probe@localhost "$@"; }
  (cd "$dir" && CARGO_TARGET_DIR=$target cargo clean -q -p wezterm-version)
  check "$kind cold build" "$dir" "$target" "$(expected $dir)"
  sleep 1; g commit -q --allow-empty -m probe
  check "$kind after a commit" "$dir" "$target" "$(expected $dir)"
  g switch -q "$other"
  check "$kind after switching branch" "$dir" "$target" "$(expected $dir)"
  g pack-refs --all
  sleep 1; g commit -q --allow-empty -m probe
  check "$kind after packing refs and a commit" "$dir" "$target" "$(expected $dir)"
  g -c advice.detachedHead=false checkout -q "$commit"
  check "$kind on a detached HEAD" "$dir" "$target" "$(expected $dir)"
  # The build script watches .tag only after it has found one, so a .tag added
  # to a built checkout does not rerun it; a CI runner writes .tag before its
  # first build, so this pair starts with the version crate cleaned out.
  (cd "$dir" && CARGO_TARGET_DIR=$target cargo clean -q -p wezterm-version)
  echo "probe-tag-$kind" > "$dir/.tag"
  check "$kind cold build with a .tag file" "$dir" "$target" "probe-tag-$kind"
  rm "$dir/.tag"
  check "$kind after removing .tag" "$dir" "$target" "$(expected $dir)"
done

echo "$fails failed"
[[ $fails == 0 ]]
