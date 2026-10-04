# Disposable mux push checks

Build the selected checkout's server and CLI, then pass both binaries explicitly:

```sh
python3 fork/mux-push-harness/run.py \
  --server target/debug/wezterm-mux-server --cli target/debug/wezterm \
  --source . --output target/qa/mux-pushes \
  --rows 200000 --clients 1,100,1,100,1 --seconds 10 --require-silent
```

The harness uses a minimal config, synthetic output and its own temporary Unix
socket. It never auto-starts a default server. It terminates only the server
process it starts. Requirements: Python 3.9 or later, `ps`, `lsof` and `zstd`.

Each case writes at 20 Hz and records server CPU seconds and render-frame
counts. A subscribed positive control must receive foreground and unpolled
background-pane updates. The disconnect check requires the server's Unix
descriptor count to return to baseline under continuing output.

`results.json` records the source commit, branch, dirty status, binary version
and binary hashes. A version that does not contain the selected commit hash
is rejected. If a cached build reports an earlier commit, force its version
build script to rerun before repeating the check.

Use repeated one-client measurements to assess normal variation before
comparing 100 clients. CPU time includes the whole server; it does not isolate
the dirty-row query. Unit branches provide independent evidence; repeat the
check on the integrated `build` branch before installing an engine.
