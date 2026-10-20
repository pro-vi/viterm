#!/bin/zsh
# run-probe.sh <dir holding wezterm-gui and wezterm-mux-server> <label> [local|mux]
# Counts tab bar rebuilds per OSC 0 title write. Runs one throwaway GUI with
# probe.lua (no plugin; in mux mode, attached to a throwaway mux server on a
# socket of its own), waits until writer.sh has written its titles, and prints
# the format-tab-title call counts at the start and end markers plus the
# GUI's CPU time between them. One rebuild calls the handler twice per tab.
# Logs go to target/qa/title-coalesce/<label>-<mode>. Ends what it started by pid.
set -u
bin=$1 label=$2 mode=${3:-local}
Q=${0:A:h}
out=${Q:h:h}/target/qa/title-coalesce/$label-$mode
rm -rf $out; mkdir -p $out
export PROBE_WRITER=$Q/writer.sh PROBE_WRITES=${PROBE_WRITES:-40} PROBE_GAP=${PROBE_GAP:-1} PROBE_TABS=${PROBE_TABS:-11}
unset WEZTERM_UNIX_SOCKET WEZTERM_PANE PROBE_DOMAIN
spid= realm=
if [[ $mode == mux ]]; then
  realm=$(mktemp -d "${TMPDIR:-/tmp}/wezprobe.XXXX")
  export PROBE_DOMAIN=$realm/sock
  export PROBE_DELAY=${PROBE_DELAY:-12}
  $bin/wezterm-mux-server --config-file $Q/probe.lua > $out/server.log 2>&1 &
  spid=$!
  for i in {1..200}; do [[ -S $PROBE_DOMAIN ]] && break; sleep 0.1; done
  [[ -S $PROBE_DOMAIN ]] || { echo "server did not bind"; kill $spid; exit 1; }
  $bin/wezterm-gui --config-file $Q/probe.lua connect probe --class probe > $out/gui.log 2>&1 &
else
  $bin/wezterm-gui --config-file $Q/probe.lua start --class probe > $out/gui.log 2>&1 &
fi
gpid=$!
cpu() { ps -o time= -p $gpid | awk -F'[:.]' '{ if (NF==3) print $1*60+$2+$3/100; else print $1*3600+$2*60+$3+$4/100 }'; }
limit=$((SECONDS + 120 + PROBE_WRITES * 2))
c0=
while (( SECONDS < limit )); do
  if [[ -z $c0 ]] && /usr/bin/grep -q 'PROBE phase=start' $out/gui.log; then c0=$(cpu); fi
  /usr/bin/grep -q 'PROBE phase=end' $out/gui.log && break
  kill -0 $gpid 2>/dev/null || break
  sleep 0.2
done
c1=$(cpu)
kill $gpid 2>/dev/null; [[ -n $spid ]] && kill $spid 2>/dev/null
wait 2>/dev/null
[[ -n $realm ]] && rm -rf $realm
/usr/bin/grep -o 'PROBE .*' $out/gui.log
[[ -n $c0 && -n $c1 ]] && echo "cpu_seconds_between_markers=$(( c1 - c0 ))" || echo "cpu_seconds_between_markers=unknown"
