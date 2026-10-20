#!/bin/sh
# Writes $1 OSC 0 titles, $2 seconds apart, bracketed by user vars the
# probe config logs, then idles.
n=${1:-60}; gap=${2:-0.5}; delay=${3:-4}
uv() { printf '\033]1337;SetUserVar=%s=%s\007' "$1" "$(printf %s "$2" | base64)"; }
sleep $delay
uv phase start
sleep 1
i=0
while [ $i -lt $n ]; do
  printf '\033]0;title-%s\007' $i
  i=$((i+1))
  sleep $gap
done
sleep 2
uv phase end
exec sleep 3600
