"""Bounded line protocol and owned vendor-process lifecycle."""
import collections
import json
import os
import queue
import signal
import subprocess
import threading
import time

MAX_FRAME = 8 * 1024 * 1024

class OversizedFrame(ValueError):
    pass

def read_frame(reader):
    data = reader.readline(MAX_FRAME + 1)
    if len(data) > MAX_FRAME:
        raise OversizedFrame('JSON frame exceeds 8 MiB')
    return data or None

def skip_line(reader):
    while True:
        chunk = reader.readline(65536)
        if not chunk or chunk.endswith('\n' if isinstance(chunk, str) else b'\n'):
            return

def refusal(identity):
    return {'id': identity, 'error': {'code': -32601, 'message': 'Communication delivery cannot approve actions or execute tools'}}

def encode(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode()

class Wire:
    def __init__(self, command, args, cwd, environment=(), deadline=None, stop=None):
        self.stop = stop or threading.Event()
        self.deadline = deadline
        self.pending = collections.deque()
        self.sequence = 0
        self.stopped = False
        self.rx, self.writes, self.written = queue.Queue(128), queue.Queue(1), queue.Queue(1)
        env = dict(os.environ)
        env.update(dict(environment))
        options = {}
        if __import__('sys').platform.startswith('linux'):
            import ctypes
            libc = ctypes.CDLL(None, use_errno=True)
            parent = os.getpid()
            def parent_death():
                if libc.prctl(1, signal.SIGKILL, 0, 0, 0) != 0:
                    os._exit(127)
                if os.getppid() != parent:
                    os.kill(os.getpid(), signal.SIGKILL)
            options['preexec_fn'] = parent_death
        self.child = subprocess.Popen([command] + list(args), cwd=cwd, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=None, start_new_session=os.name != 'nt', **options)
        self.reader = threading.Thread(target=self._reader, daemon=True)
        self.writer = threading.Thread(target=self._writer, daemon=True)
        self.reader.start()
        self.writer.start()

    @property
    def pid(self):
        return self.child.pid

    def _put(self, target, value):
        while not self.stopped:
            try:
                target.put(value, timeout=.1)
                return
            except queue.Full:
                pass

    def _reader(self):
        try:
            while not self.stopped:
                line = read_frame(self.child.stdout)
                if line is None:
                    raise RuntimeError('Vendor process closed stdout')
                self._put(self.rx, json.loads(line))
        except Exception as error:
            self._put(self.rx, error)

    def _writer(self):
        while not self.stopped:
            try:
                frame = self.writes.get(timeout=.1)
            except queue.Empty:
                continue
            try:
                self.child.stdin.write(frame)
                self.child.stdin.flush()
                self._put(self.written, True)
            except Exception as error:
                self._put(self.written, error)
                return

    def _deadline(self):
        return min(time.monotonic() + 30, self.deadline or float('inf'))

    def send(self, value):
        if self.stopped:
            raise RuntimeError('Vendor process closed')
        frame = encode(value) + b'\n'
        if len(frame) > MAX_FRAME:
            raise OversizedFrame('JSON frame exceeds 8 MiB')
        self.writes.put_nowait(frame)
        deadline = self._deadline()
        while time.monotonic() < deadline:
            if self.stop.is_set():
                raise RuntimeError('Worker stopped')
            try:
                result = self.written.get(timeout=.1)
                if isinstance(result, Exception):
                    raise result
                return
            except queue.Empty:
                pass
        raise TimeoutError('Timed out writing to vendor')

    def receive(self, timeout):
        try:
            value = self.rx.get(timeout=timeout)
        except queue.Empty:
            return None
        if isinstance(value, Exception):
            raise value
        return value

    def event(self, timeout):
        return self.pending.popleft() if self.pending else self.receive(timeout)

    def request(self, method, params, stop=None):
        return self.exchange(method, params, False)

    def pi_request(self, method, params, stop=None):
        return self.exchange(method, params, True)

    def exchange(self, method, params, pi):
        self.sequence += 1
        identity = str(self.sequence) if pi else self.sequence
        self.send(dict(params, id=identity, type=method) if pi else {'id': identity, 'method': method, 'params': params})
        deadline = self._deadline()
        while time.monotonic() < deadline:
            if self.stop.is_set():
                raise RuntimeError('Worker stopped')
            value = self.receive(.1)
            if value is None:
                continue
            if value.get('id') == identity and 'method' not in value:
                if 'error' in value or (pi and value.get('success') is not True):
                    raise RuntimeError('Vendor request failed: ' + json.dumps(value))
                return value.get('data' if pi else 'result')
            if 'id' in value and 'method' in value:
                self.send(refusal(value['id']))
            else:
                if len(self.pending) >= 512:
                    raise RuntimeError('Vendor notification queue full')
                self.pending.append(value)
        raise TimeoutError('Timed out: ' + method)

    def close(self):
        if self.stopped:
            return
        self.stopped = True
        if self.child.poll() is None:
            if os.name == 'nt':
                subprocess.run(['taskkill', '/PID', str(self.pid), '/T', '/F'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            else:
                try:
                    os.killpg(self.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    self.child.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(self.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
            if self.child.poll() is None:
                self.child.kill()
        self.child.wait()
        for thread in (self.reader, self.writer):
            thread.join(timeout=.5)
        for stream in (self.child.stdin, self.child.stdout):
            if not self.reader.is_alive() and not self.writer.is_alive():
                stream.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
