#!/usr/bin/env bash
# Rebuild `build` from upstream/main and the units PATCHES.md lists.
#
#   fork/reassemble.sh check       report each unit: ref, base, staleness
#   fork/reassemble.sh start       build build-next from upstream/main
#   fork/reassemble.sh continue    after resolving and committing a conflicted merge
#   fork/reassemble.sh compare     what build-next changes against build
#   fork/reassemble.sh promote     move build to build-next (build must be checked out)
#
# PATCHES.md is the only list of units. A row whose status says "not merged"
# is skipped; every other row is merged, in ledger order, with `--no-ff`.
# A unit whose base is another unit must come after that unit in the ledger.
#
# Nothing here touches `build` except `promote`, so a rebuild can be tried,
# thrown away (`git branch -D build-next`) and tried again.
#
# A conflict is resolved for you only when nothing about it is new: every
# conflicted file has, on both sides, exactly the bytes it had in the old
# build's merge of the same unit. That merge's result is then the answer, and
# the merge message says it was reused. Any other conflict stops the run:
# resolve it, commit the merge naming the unit it conflicts with, then run the
# `continue` command the stop message prints. The old merge is printed as a
# hint there, because its resolution was made against different input.
#
# A resolution that exists only in a merge commit is lost by a rebuild; the
# `compare` step is where that shows. Move such a fix into the unit it belongs
# to before promoting.
#
# Fork-only files (FORK_FILES below) are restored from the old build in one
# commit at the end. They exist only on `build`; no unit carries them.
#
# Bash 3.2 compatible: this runs under /bin/bash on macOS.
set -euo pipefail

FORK_FILES="AGENTS.md CLAUDE.md PATCHES.md BACKLOG.md docs/plans fork"
TARGET="${REASSEMBLE_TARGET:-build-next}"
UPSTREAM="upstream/main"

top="$(git rev-parse --show-toplevel)"
cd "$top"
state="$(cd "$(git rev-parse --git-dir)" && pwd)/reassemble"
# Notes from unit_ref, which runs in command substitutions; printed once.
notes="$(mktemp)"
trap 'rm -f "$notes"' EXIT

die() { printf 'reassemble: %s\n' "$*" >&2; exit 1; }
say() { printf '%s\n' "$*"; }

# One line per merged unit: "<slug> <base>", in ledger order.
ledger_units() {
  git show build:PATCHES.md | awk -F' \\| ' '
    /^## / && seen { exit }
    /^\| `[^`]+` \| `unit\// {
      seen = 1
      slug = $1; sub(/^\| /, "", slug); gsub(/`/, "", slug)
      base = $3; match(base, /`[^`]+`/)
      base = substr(base, RSTART + 1, RLENGTH - 2)
      if ($5 ~ /not merged/) next
      print slug, base
    }'
}

# The commit a unit points at. A local branch and origin's copy must agree
# when both exist, so a stale local branch cannot be merged by accident.
unit_ref() {
  local slug="$1" local_sha="" remote_sha=""
  local_sha="$(git rev-parse -q --verify "refs/heads/unit/$slug" || true)"
  remote_sha="$(git rev-parse -q --verify "refs/remotes/origin/unit/$slug" || true)"
  if [ -n "$local_sha" ] && [ -n "$remote_sha" ] && [ "$local_sha" != "$remote_sha" ]; then
    if git merge-base --is-ancestor "$remote_sha" "$local_sha"; then
      printf '%s\n' "unit/$slug"; return
    fi
    # A rebase onto a newer upstream rewrites every commit. The local branch
    # is that rewrite when each commit subject on origin's copy reappears on
    # it; anything else is work on origin this checkout has not seen.
    local missing
    missing="$(comm -23 \
      <(git log --format=%s "$(git merge-base "$local_sha" "$remote_sha")..$remote_sha" | sort -u) \
      <(git log --format=%s "$UPSTREAM..$local_sha" | sort -u))"
    if [ -z "$missing" ]; then
      printf 'note: unit/%s rewrites origin/unit/%s; publish it with --force-with-lease\n' \
        "$slug" "$slug" >> "$notes"
      printf '%s\n' "unit/$slug"; return
    fi
    die "unit/$slug and origin/unit/$slug have diverged, and origin has commits the local branch lacks: $missing"
  fi
  if [ -n "$local_sha" ]; then printf '%s\n' "unit/$slug"
  elif [ -n "$remote_sha" ]; then printf '%s\n' "origin/unit/$slug"
  else die "no branch for unit $slug (neither unit/$slug nor origin/unit/$slug)"
  fi
}

base_ref() {
  case "$1" in
    upstream/main) printf '%s\n' "$UPSTREAM" ;;
    unit/*) unit_ref "${1#unit/}" ;;
    *) die "unrecognised base '$1' in PATCHES.md" ;;
  esac
}

cmd_check() {
  git fetch -q upstream && git fetch -q origin
  local stale=0 slug base ref bref behind merged=""
  while read -r slug base; do
    ref="$(unit_ref "$slug")"
    case " $merged " in *" ${base#unit/} "*) ;; *)
      [ "$base" = "upstream/main" ] || die "ledger lists $slug before its base $base" ;;
    esac
    bref="$(base_ref "$base")"
    behind="$(git rev-list --count "$ref..$bref")"
    printf '%-28s %-36s base=%-24s behind_base=%s\n' "$slug" "$ref" "$base" "$behind"
    [ "$behind" = 0 ] || stale=1
    merged="$merged $slug"
  done < <(ledger_units)
  sort -u "$notes"
  if [ "$stale" = 1 ]; then
    say ""
    say "Some units do not contain their base. Rebase them first (CLAUDE.md, 'When upstream moves'):"
    say "  a unit on upstream/main:   git rebase upstream/main unit/<slug>"
    say "  a stack:                   git rebase --update-refs upstream/main unit/<leaf>"
    return 1
  fi
}

merge_next() {
  local slug ref old
  while [ -s "$state/todo" ]; do
    slug="$(head -n 1 "$state/todo")"
    ref="$(unit_ref "$slug")"
    say "merge  $slug  ($ref)"
    if ! git merge --no-ff -q -m "Merge unit/$slug into build" "$ref"; then
      old="$(git log --format=%H --merges --first-parent "$(cat "$state/old")" \
             --grep="unit/$slug" | head -n 1)"
      if [ -n "$old" ] && reuse_resolution "$old"; then
        git commit -q -F - <<MSG
Merge unit/$slug into build

Every conflicted file had, on both sides, the same content as in the
previous build's merge of this unit ($old), so that resolution is reused
unchanged. Files: $(tr '\n' ' ' < "$state/reused")
MSG
        say "       reused the resolution from $old"
      else
        say ""
        say "Conflict merging unit/$slug. Files:"
        git diff --name-only --diff-filter=U | sed 's/^/  /'
        [ -z "$old" ] || say "The old build merged it in $old; 'git show $old:<file>' is a hint only."
        say "Resolve, 'git commit' (name the conflicting unit in the message), then:"
        say "  $state/reassemble.sh continue"
        exit 2
      fi
    fi
    sed -i '' 1d "$state/todo"
  done
  restore_fork_files
}

# Take the old merge's result for each conflicted file, but only when both
# sides of that file are byte-identical to the old merge's two parents.
# Resolves nothing and returns 1 if any file fails that test.
reuse_resolution() {
  local old="$1" f
  local files; files="$(git diff --name-only --diff-filter=U)"
  [ -n "$files" ] || return 1
  while IFS= read -r f; do
    [ "$(git rev-parse -q --verify "HEAD:$f")" = "$(git rev-parse -q --verify "$old^1:$f")" ] || return 1
    [ "$(git rev-parse -q --verify "MERGE_HEAD:$f")" = "$(git rev-parse -q --verify "$old^2:$f")" ] || return 1
  done <<< "$files"
  while IFS= read -r f; do
    git checkout "$old" -- "$f"
  done <<< "$files"
  printf '%s\n' "$files" > "$state/reused"
}

restore_fork_files() {
  local old; old="$(cat "$state/old")"
  # shellcheck disable=SC2086 # FORK_FILES is a word list on purpose
  git checkout "$old" -- $FORK_FILES
  git commit -q -m "build: restore the fork-only files" || true
  rm -rf "$state"
  say ""
  say "$TARGET is assembled. Next: fork/reassemble.sh compare"
}

cmd_start() {
  [ -z "$(git status --porcelain --untracked-files=no)" ] || die "working tree has changes"
  [ ! -d "$state" ] || die "a rebuild is in progress; 'continue' it or 'rm -rf $state'"
  ! git rev-parse -q --verify "refs/heads/$TARGET" >/dev/null \
    || die "$TARGET exists; delete it (git branch -D $TARGET) to start over"
  cmd_check || die "stale units; see above"
  mkdir -p "$state"
  cp "$0" "$state/reassemble.sh"
  git rev-parse build > "$state/old"
  ledger_units | awk '{print $1}' > "$state/todo"
  git switch -q -c "$TARGET" "$UPSTREAM"
  merge_next
}

cmd_continue() {
  [ -d "$state" ] || die "no rebuild in progress"
  [ -z "$(git diff --name-only --diff-filter=U)" ] || die "unresolved files remain"
  git rev-parse -q --verify MERGE_HEAD >/dev/null && die "merge not committed yet; run 'git commit'"
  sed -i '' 1d "$state/todo"
  merge_next
}

cmd_compare() {
  git rev-parse -q --verify "refs/heads/$TARGET" >/dev/null || die "no $TARGET branch"
  local files
  files="$(git diff --name-only build "$TARGET")"
  if [ -z "$files" ]; then
    say "identical: $TARGET has the same tree as build"
  else
    say "$TARGET differs from build in:"
    git diff --stat build "$TARGET" | sed 's/^/  /'
  fi
  say "units in build but not in $TARGET:"
  local slug base
  while read -r slug base; do
    git merge-base --is-ancestor "$(unit_ref "$slug")" "$TARGET" || say "  MISSING $slug"
  done < <(ledger_units)
}

cmd_promote() {
  [ "$(git branch --show-current)" = build ] || die "check out build first"
  [ -z "$(git status --porcelain --untracked-files=no)" ] || die "working tree has changes"
  git rev-parse -q --verify "refs/heads/$TARGET" >/dev/null || die "no $TARGET branch"
  local old; old="$(git rev-parse build)"
  git reset -q --hard "$TARGET"
  git branch -D -q "$TARGET"
  say "build moved from $old to $(git rev-parse --short HEAD); the old tip stays in the reflog."
  say "Publish with: git push --force-with-lease=build:$(git rev-parse origin/build) origin build"
}

case "${1:-}" in
  check) cmd_check ;;
  start) cmd_start ;;
  continue) cmd_continue ;;
  compare) cmd_compare ;;
  promote) cmd_promote ;;
  *) sed -n '2,8p' "$0"; exit 64 ;;
esac
