#!/bin/zsh
# run.sh <wezterm-mux-server> <wezterm>
# Two checks of how a mux server takes its unix socket, each on a throwaway
# socket, with every pane running sleep:
#   two servers: start A, then B on the same path, both in the foreground (no
#     --daemonize, so no pid file lock). Prints the socket's inode before and
#     after B, whether each is still running, and whether `cli list` (with the
#     given wezterm) reaches a server.
#   two domains: one server whose config names the same path in two unix
#     domains. Prints whether it is still running and whether `cli list`
#     reaches it.
# Ends what it started by pid.
set -u
srv=$1 cli=$2

listed() {
  WEZTERM_UNIX_SOCKET=$1 $cli cli --no-auto-start list --format json 2>/dev/null |
    python3 -c 'import json,sys; print("cli list reached a server with", len(json.load(sys.stdin)), "pane(s)")' 2>/dev/null ||
    echo "cli list failed"
}

wait_for_socket() { for i in {1..100}; do [[ -S $1 ]] && return; sleep 0.1; done; }

echo "== two servers"
realm=$(mktemp -d "${TMPDIR:-/tmp}/wezbind.XXXX")
cat > $realm/cfg.lua <<LUA
return {
  unix_domains = { { name = 'bind', socket_path = '$realm/sock' } },
  default_prog = { '/bin/sleep', '3600' },
}
LUA
$srv --config-file $realm/cfg.lua > $realm/a.log 2>&1 &
a=$!
wait_for_socket $realm/sock
i1=$(stat -f %i $realm/sock)
$srv --config-file $realm/cfg.lua > $realm/b.log 2>&1 &
b=$!
sleep 3
echo "inode before B: $i1, after B: $(stat -f %i $realm/sock 2>/dev/null)"
kill -0 $a 2>/dev/null && echo "A running" || echo "A exited"
kill -0 $b 2>/dev/null && echo "B running" || echo "B exited: $(/usr/bin/grep -o -m1 'in use by another server.*' $realm/b.log)"
listed $realm/sock
kill $a $b 2>/dev/null; wait 2>/dev/null; rm -rf $realm

echo "== two domains, one path"
realm=$(mktemp -d "${TMPDIR:-/tmp}/wezdup.XXXX")
cat > $realm/cfg.lua <<LUA
return {
  unix_domains = {
    { name = 'one', socket_path = '$realm/sock' },
    { name = 'two', socket_path = '$realm/sock' },
  },
  default_prog = { '/bin/sleep', '3600' },
}
LUA
$srv --config-file $realm/cfg.lua > $realm/s.log 2>&1 &
p=$!
sleep 3
kill -0 $p 2>/dev/null && echo "server running" || echo "server exited: $(/usr/bin/grep -o -m1 'problem spawning listeners' $realm/s.log)"
listed $realm/sock
kill $p 2>/dev/null; wait 2>/dev/null; rm -rf $realm
