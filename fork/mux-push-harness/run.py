"""Measure explicitly selected binaries on disposable mux sockets only."""

import argparse
import hashlib
from contextlib import ExitStack
import json
import os
from pathlib import Path
import selectors
import shutil
import socket
import subprocess
import tempfile
import threading
import time


def leb(value: int) -> bytes:
    result = bytearray()
    while value >= 128:
        result.append((value & 127) | 128)
        value >>= 7
    result.append(value)
    return bytes(result)


def frame(identifier: int, payload: bytes, serial: int = 1) -> bytes:
    header = leb(serial) + leb(identifier)
    return leb(len(header) + len(payload)) + header + payload


class Clients:
    def __init__(self, path: str, count: int) -> None:
        self.selector = selectors.DefaultSelector()
        self.sockets = []
        self.buffers = [bytearray() for _ in range(count)]
        self.counts = [0] * count
        self.identifiers = [set() for _ in range(count)]
        self.render_panes = {}
        self.decode_panes = False
        self.errors = []
        self.stop = threading.Event()
        for index in range(count):
            peer = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            peer.connect(path)
            peer.setblocking(False)
            self.sockets.append(peer)
            self.selector.register(peer, selectors.EVENT_READ, index)
        self.thread = threading.Thread(target=self.drain, daemon=True)
        self.thread.start()

    def parse(self, index: int) -> None:
        data = self.buffers[index]
        consumed = 0
        while consumed < len(data):
            position = consumed
            values = []
            sizes = []
            for _ in range(3):
                value = shift = size = 0
                while position < len(data):
                    byte = data[position]
                    position += 1
                    size += 1
                    value |= (byte & 127) << shift
                    shift += 7
                    if not byte & 128:
                        break
                else:
                    del data[:consumed]
                    return
                values.append(value)
                sizes.append(size)
            end = consumed + sizes[0] + (values[0] & ~(1 << 63))
            if end > len(data):
                break
            self.identifiers[index].add(values[2])
            if values[2] == 25:
                self.counts[index] += 1
                if self.decode_panes:
                    payload = bytes(data[position:end])
                    if values[0] & (1 << 63):
                        payload = subprocess.run(['zstd', '-q', '-d', '-c'], input=payload,
                                                 capture_output=True, check=True).stdout
                    pane = shift = 0
                    for byte in payload:
                        pane |= (byte & 127) << shift
                        shift += 7
                        if not byte & 128:
                            break
                    self.render_panes[pane] = self.render_panes.get(pane, 0) + 1
            consumed = end
        del data[:consumed]

    def drain(self) -> None:
        try:
            while not self.stop.is_set():
                for key, _ in self.selector.select(0.05):
                    try:
                        data = key.fileobj.recv(1 << 20)
                    except BlockingIOError:
                        continue
                    if not data:
                        self.selector.unregister(key.fileobj)
                        continue
                    self.buffers[key.data].extend(data)
                    self.parse(key.data)
        except Exception as error:
            self.errors.append(type(error).__name__)

    def subscribe(self, pane: int) -> None:
        self.decode_panes = True
        # codec::GetPaneRenderChanges (24) carries one varbincode usize.
        self.sockets[0].sendall(frame(24, leb(pane)))

    def close(self) -> None:
        self.stop.set()
        self.thread.join(2)
        if self.thread.is_alive():
            raise RuntimeError('client drain thread did not stop')
        for peer in self.sockets:
            peer.close()
        self.selector.close()
        if self.errors:
            raise RuntimeError('client drain failed: ' + ','.join(self.errors))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--server', required=True, type=Path)
    parser.add_argument('--cli', required=True, type=Path)
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--rows', type=int, default=200000)
    parser.add_argument('--clients', default='1,100,1,100,1')
    parser.add_argument('--seconds', type=float, default=10)
    parser.add_argument('--require-silent', action='store_true')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    records = []

    def emit(**result):
        records.append(result)
        print(json.dumps(result), flush=True)
        (args.output / 'results.json').write_text(json.dumps(records, indent=2) + '\n')

    source = args.source.resolve()
    version = subprocess.check_output([str(args.server.resolve()), '--version'], text=True).strip()
    sha = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
    if sha[:8] not in version:
        raise RuntimeError('binary version does not match the selected source commit')
    def digest(path):
        checksum = hashlib.sha256()
        with path.open('rb') as binary:
            for chunk in iter(lambda: binary.read(1 << 20), b''):
                checksum.update(chunk)
        return checksum.hexdigest()
    emit(step='binary', version=version, sha=sha,
         server_sha256=digest(args.server), cli_sha256=digest(args.cli),
         branch=subprocess.check_output(['git', '-C', str(source), 'branch', '--show-current'], text=True).strip(),
         dirty=bool(subprocess.check_output(['git', '-C', str(source), 'status', '--porcelain'])),
         server=str(args.server.resolve()), cli=str(args.cli.resolve()))
    with ExitStack() as cleanup:
        realm = Path(tempfile.mkdtemp(prefix='vmr.', dir='/private/tmp'))
        cleanup.callback(shutil.rmtree, realm)
        socket_path = str(realm / 'sock')
        if not socket_path.startswith('/private/tmp/vmr.'):
            raise RuntimeError('a disposable socket is required')
        feed = realm / 'feed.txt'
        feed.touch()
        env = dict(os.environ, WEZTERM_UNIX_SOCKET=socket_path, MUX_PUSH_SOCKET=socket_path)
        env.pop('WEZTERM_PANE', None)
        wrapper = Path(__file__).with_name('wrapper.lua')
        server_log = cleanup.enter_context((args.output / 'server.log').open('w'))
        server = subprocess.Popen([str(args.server.resolve()), '--config-file', str(wrapper)],
                                  env=env, stdout=server_log, stderr=subprocess.STDOUT)

        def cli(*arguments):
            completed = subprocess.run([str(args.cli.resolve()), '--skip-config', 'cli',
                                        '--prefer-mux', '--no-auto-start', *arguments],
                                       env=env, capture_output=True, text=True, timeout=10)
            if completed.returncode:
                raise RuntimeError('disposable CLI failed: ' + arguments[0])
            return completed.stdout

        def cpu():
            value = subprocess.check_output(['ps', '-o', 'cputime=', '-p', str(server.pid)], text=True).strip()
            seconds = 0.0
            for part in value.split(':'):
                seconds = seconds * 60 + float(part)
            return seconds

        def descriptors():
            output = subprocess.run(['lsof', '-nP', '-a', '-p', str(server.pid), '-U', '-Ff'],
                                    capture_output=True, text=True, check=True).stdout
            return sum(line.startswith('f') for line in output.splitlines())

        def settle(pane, sentinel):
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                if sentinel in cli('get-text', '--pane-id', str(pane)):
                    return
                time.sleep(0.1)
            raise RuntimeError('disposable pane did not reach sentinel')

        clients = None
        try:
            deadline = time.monotonic() + 15
            while not Path(socket_path).exists():
                if server.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError('disposable server did not start')
                time.sleep(0.1)
            pane = int(cli('spawn', '--new-window', '--', 'tail', '-n', '+1', '-f', str(feed)).strip())
            with feed.open('a') as output:
                for row in range(args.rows):
                    output.write(f'row {row:08d} ' + 'x' * 60 + '\n')
                output.write('FILL_COMPLETE\n')
            settle(pane, 'FILL_COMPLETE')
            time.sleep(0.5)
            emit(step='filled', requested_rows=args.rows, socket=socket_path, pid=server.pid)

            for index, count in enumerate(map(int, args.clients.split(','))):
                clients = Clients(socket_path, count)
                time.sleep(0.5)
                clients.counts = [0] * count
                before, started = cpu(), time.monotonic()
                writes = 0
                with feed.open('a', buffering=1) as output:
                    while time.monotonic() - started < args.seconds:
                        writes += 1
                        output.write(f'event {index}:{writes} ' + 'y' * 60 + '\n')
                        time.sleep(max(0, started + writes / 20 - time.monotonic()))
                    sentinel = f'DONE_{index}'
                    output.write(sentinel + '\n')
                settle(pane, sentinel)
                time.sleep(0.5)
                used = cpu() - before
                pushes = sum(clients.counts)
                emit(step='cpu', clients=count, writes=writes, cpu_s=round(used, 4), pushes=pushes)
                if args.require_silent and pushes != 0:
                    raise RuntimeError('silent clients received render pushes')
                clients.close()
                clients = None

            # One render request must activate output for another, unpolled pane.
            clients = Clients(socket_path, 1)
            clients.subscribe(pane)
            deadline = time.monotonic() + 5
            while clients.counts[0] == 0 and time.monotonic() < deadline:
                time.sleep(0.05)
            if clients.counts[0] == 0:
                raise RuntimeError('render subscriber did not receive its initial delta')
            other_feed = realm / 'background.txt'
            other_feed.touch()
            other = int(cli('spawn', '--new-window', '--', 'tail', '-n', '+1', '-f', str(other_feed)).strip())
            with other_feed.open('a') as output:
                output.write('SUBSCRIBER_CONTROL\n')
            settle(other, 'SUBSCRIBER_CONTROL')
            time.sleep(0.3)
            if clients.render_panes.get(other, 0) == 0:
                raise RuntimeError('subscriber lost background pane output')
            emit(step='subscriber', render_pushes=clients.counts[0], background_pane=other,
                 render_panes=clients.render_panes,
                 identifiers=sorted(clients.identifiers[0]))
            clients.close()
            clients = None

            time.sleep(0.2)
            baseline = descriptors()
            clients = Clients(socket_path, 100)
            with feed.open('a') as output:
                output.write('BEFORE_DISCONNECT\n')
            clients.close()
            clients = None
            started = time.monotonic()
            remaining = descriptors()
            deadline = started + 5
            while remaining > baseline and time.monotonic() < deadline:
                with feed.open('a') as output:
                    output.write('during disconnect\n')
                time.sleep(0.05)
                remaining = descriptors()
            emit(step='disconnect', baseline_fds=baseline, final_fds=remaining,
                 drain_s=round(time.monotonic() - started, 3))
            if remaining > baseline:
                raise RuntimeError('closed connections remained under output load')
        finally:
            try:
                if clients is not None:
                    clients.close()
            finally:
                server.terminate()
                try:
                    server.wait(5)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait(5)


if __name__ == '__main__':
    main()
