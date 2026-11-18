#!/bin/zsh
# One line-cache-harness run: a throwaway mux server whose one pane prints
# numbered lines at a steady rate, a GUI attached to it, and the GUI's memory
# footprint read before and after the lines arrive.
#
# Usage: run.zsh <scenario> <out-dir> <gui-binary> <server-engine-dir> [lines] [limit-mb]
#   scenario: steady | resize (the window is also resized once, 3 s after attach)
#   out-dir: must not exist yet and must contain "/line-cache-harness/" (the
#            socket lives in it, so keep the path under 100 bytes)
#   server-engine-dir: holds wezterm-mux-server and wezterm (the CLI)
#   lines: how many lines the pane prints in the measured phase (default 100000)
#   limit-mb: the run fails when the GUI grows by more than this (default 32)
#
# The pane first prints 5000 warm-up lines, five times the 1000-line
# scrollback the config sets, so a cache that keeps its limit is already full
# when the first reading is taken and the measured lines can only replace
# what it holds.
set -u
here=${0:A:h}
scenario=$1; run=$2; gui_bin=$3; srv_dir=$4; total=${5:-100000}; limit_mb=${6:-32}
[[ $run == */line-cache-harness/* ]] || { echo "out-dir must contain /line-cache-harness/"; exit 2; }
[[ -e $run ]] && { echo "out-dir already exists: $run"; exit 2; }
case $scenario in
  resize) export PROBE_RESIZE_AT=3 ;;
  steady) unset PROBE_RESIZE_AT ;;
  *) echo "unknown scenario $scenario"; exit 2 ;;
esac
mkdir -p $run || exit 2
sock=$run/sock
unset WEZTERM_UNIX_SOCKET WEZTERM_PANE WEZTERM_CONFIG_FILE
export PROBE_SOCK=$sock PROBE_PRINT=$here/print.py PROBE_GO=$run/go PROBE_DONE=$run/done
export PROBE_TOTAL=$total PROBE_BURST=20 PROBE_PAUSE=0.02 PROBE_WARM=5000
lim() { perl -e 'alarm shift; exec @ARGV or die "cannot run $ARGV[0]: $!\n"' "$@"; }
cli() { WEZTERM_UNIX_SOCKET=$sock lim 10 $srv_dir/wezterm --config-file $here/probe.lua cli --no-auto-start "$@"; }
nap() { perl -e 'select(undef,undef,undef,shift)' $1; }
stop() { kill $1 2>/dev/null; for i in {1..30}; do kill -0 $1 2>/dev/null || return 0; nap 0.1; done; kill -9 $1 2>/dev/null; }
mb() { python3 $here/footprint.py $1 | awk '{printf "%.1f", $1/1048576}'; }
# Waits until a file exists; fails after the given number of seconds.
await() { local t=$SECONDS; while [[ ! -e $1 ]]; do (( SECONDS - t < $2 )) || return 1; nap 0.2; done; }

$srv_dir/wezterm-mux-server --config-file $here/probe.lua > $run/server.log 2>&1 &
srv=$!
echo "server $srv" > $run/pids
for i in {1..100}; do [[ -S $sock ]] && break; nap 0.1; done
[[ -S $sock ]] || { echo "no socket"; stop $srv; exit 1; }
# The server starts with a window whose one pane runs default_prog, the printer.
panes=$(cli list --format json | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')
if [[ $panes != 1 ]]; then
  cli spawn --new-window > /dev/null || { echo "could not start the printer pane"; stop $srv; exit 1; }
fi

WEZTERM_LOG=info $gui_bin --config-file $here/probe.lua connect probe > $run/gui.stderr 2>&1 &
gui=$!
echo "gui $gui" >> $run/pids
finish() { stop $gui; stop $srv; rm -f $HOME/.local/share/wezterm/gui-sock-$gui; }
nap 10
kill -0 $gui 2>/dev/null || { echo "the GUI exited; see $run/gui.stderr"; stop $srv; exit 1; }

touch $run/go
await $run/done.warm 120 || { echo "the warm-up lines did not finish"; finish; exit 1; }
nap 6
echo "seconds,footprint_mb" > $run/footprint.csv
before=$(mb $gui)
echo "0,$before" >> $run/footprint.csv
touch $run/go.main
start=$SECONDS
deadline=$(( total / 200 + 120 ))
while [[ ! -e $run/done ]] && (( SECONDS - start < deadline )); do
  nap 2
  echo "$(( SECONDS - start )),$(mb $gui)" >> $run/footprint.csv
done
printed=$(cat $run/done 2>/dev/null || echo 0)
nap 6
after=$(mb $gui)
echo "$(( SECONDS - start )),$after" >> $run/footprint.csv
finish

python3 - $scenario $printed $before $after $limit_mb <<'PY' | tee $run/result.txt
import sys
scenario, printed, before, after, limit = sys.argv[1], int(sys.argv[2]), float(sys.argv[3]), float(sys.argv[4]), float(sys.argv[5])
grew = after - before
per_line = grew * 1048576 / printed if printed else 0
print("scenario %s: %d lines printed; GUI footprint %.1f MB -> %.1f MB (grew %.1f MB, %.0f bytes a line)"
      % (scenario, printed, before, after, grew, per_line))
ok = printed > 0 and grew <= limit
print("%s GUI growth stays under %.0f MB" % ("PASS" if ok else "FAIL", limit))
sys.exit(0 if ok else 1)
PY
exit $pipestatus[1]
