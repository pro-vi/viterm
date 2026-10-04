local socket = assert(os.getenv('MUX_PUSH_SOCKET'))
assert(socket:match('^/private/tmp/vmr%.') or socket:match('^/tmp/vmr%.'),
  'a disposable socket is required')
return {
  unix_domains = {{name = 'mux-push-test', socket_path = socket}},
  initial_cols = 80,
  initial_rows = 50,
  scrollback_lines = 200000,
  default_prog = {'sleep', '1200'},
}
