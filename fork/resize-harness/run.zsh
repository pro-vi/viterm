#!/bin/zsh
# One resize-harness run: a throwaway mux server, split tabs created through
# its CLI, a GUI attached to it, a scripted burst of window resizes, then pane
# sizes as the server and the GUI each report them, and the GUI's stats.
#
# Usage: run.zsh <scenario> <out-dir> <gui-binary> <server-engine-dir> [split-tabs]
#   scenario: burst | slow | zoom | attach | cli-split
#   out-dir: must contain "resize-harness" (the socket lives in it)
#   server-engine-dir: holds wezterm-mux-server and wezterm (the CLI)
set -u
here=${0:A:h}
scenario=$1; run=$2; gui_bin=$3; srv_dir=$4; ntabs=${5:-20}
[[ $run == *resize-harness* ]] || { echo "out-dir must contain resize-harness"; exit 2; }
burst=$(python3 -c 'print(",".join(["%dx%d" % (1000+i*70, 700+i*35) for i in range(10)] + ["%dx%d" % (1100+i*55, 760+i*25) for i in range(9,-1,-1)]))')
export PROBE_STATS=10 PROBE_PAD_KEYS=${PROBE_PAD_KEYS:-0} PROBE_ZOOM=
wait_s=32
case $scenario in
  burst|cli-split) export PROBE_SIZES=$burst PROBE_STEP=0.04 PROBE_START=3 ;;
  slow)   export PROBE_SIZES="1000x700,1300x850,1100x760,1500x950,1200x800,1100x760" PROBE_STEP=2.0 PROBE_START=3 ;;
  zoom)   export PROBE_SIZES=$burst PROBE_STEP=0.04 PROBE_START=3 PROBE_ZOOM=1 ;;
  attach) export PROBE_SIZES=$burst PROBE_STEP=0.04 PROBE_START=0.1 ;;
  *) echo "unknown scenario $scenario"; exit 2 ;;
esac
rm -rf $run; mkdir -p $run
sock=$run/sock
unset WEZTERM_UNIX_SOCKET WEZTERM_PANE WEZTERM_CONFIG_FILE
export PROBE_SOCK=$sock
lim() { perl -e 'alarm shift; exec @ARGV or die "cannot run $ARGV[0]: $!\n"' "$@"; }
cli() { WEZTERM_UNIX_SOCKET=$sock lim 10 $srv_dir/wezterm --config-file $here/probe.lua cli --no-auto-start "$@"; }
stop() { kill $1 2>/dev/null; for i in {1..30}; do kill -0 $1 2>/dev/null || return 0; perl -e 'select(undef,undef,undef,0.1)'; done; kill -9 $1 2>/dev/null; }

$srv_dir/wezterm-mux-server --config-file $here/probe.lua > $run/server.log 2>&1 &
srv=$!
echo "server $srv" > $run/pids
for i in {1..100}; do [[ -S $sock ]] && break; perl -e 'select(undef,undef,undef,0.1)'; done
[[ -S $sock ]] || { echo "no socket"; stop $srv; exit 1; }

# The server starts with a window holding one tab; that tab is the
# single-pane tab, and the split tabs join the same window.
win=$(cli list --format json | python3 -c 'import json,sys; l=json.load(sys.stdin); print(l[0]["window_id"] if l else "")')
if [[ -z $win ]]; then
  cli spawn --new-window -- /bin/sleep 100000 > /dev/null || { stop $srv; exit 1; }
  win=$(cli list --format json | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["window_id"])')
fi
last_right=
for i in $(seq 1 $ntabs); do
  p=$(cli spawn --window-id $win -- /bin/sleep 100000)
  r=$(cli split-pane --pane-id $p --right -- /bin/sleep 100000)
  last_right=$r
  if [[ -n $PROBE_ZOOM ]] && (( i % 2 == 1 )); then cli zoom-pane --pane-id $r --zoom; fi
done
cli list --format json > $run/server-before.json

# The trace line from apply_dimensions records the window's terminal size,
# which check.py takes as the size every tab must have.
WEZTERM_LOG="info,wezterm_gui::termwindow::resize=trace" $gui_bin --config-file $here/probe.lua connect probe > $run/gui.stderr 2>&1 &
gui=$!
echo "gui $gui" >> $run/pids
perl -e 'select(undef,undef,undef,shift)' $wait_s
if [[ $scenario == cli-split ]]; then
  # A new pane reaches an attached GUI when its first output does (the client
  # resyncs on output from a pane it does not know), so this one prints once.
  cli split-pane --pane-id $last_right --bottom -- /bin/sh -c 'echo split; exec /bin/sleep 100000' > $run/cli-split-pane
  perl -e 'select(undef,undef,undef,4)'
fi
cli list --format json > $run/server-after.json
WEZTERM_UNIX_SOCKET=$HOME/.local/share/wezterm/gui-sock-$gui lim 10 $srv_dir/wezterm --config-file $here/probe.lua cli --no-auto-start list --format json > $run/gui-after.json 2>/dev/null
stop $gui; stop $srv
rm -f $HOME/.local/share/wezterm/gui-sock-$gui
python3 $here/check.py $scenario $run
