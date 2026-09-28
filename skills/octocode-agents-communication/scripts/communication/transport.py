"""Local native adapters. Transport acceptance never means model acknowledgement."""
import base64
import hashlib
import http.client
import ipaddress
import json
import os
import re
import socket
import stat
import struct
import time
import urllib.parse
import uuid
from pathlib import Path
from .wire import MAX_FRAME, encode, refusal

RECEIPTS = {'claude': 'socket-write-only', 'codex': 'jsonrpc', 'grok': 'acp-turn-completion', 'opencode': 'http'}

def requires_action(mode):
    return mode in ('claude', 'grok')

def capabilities(mode):
    if mode not in RECEIPTS:
        return {'nativeInput': False, 'acceptanceReceipt': 'host-confirmation', 'readReceipt': False}
    value = {'nativeInput': True, 'passiveInjection': not requires_action(mode), 'actionWake': True, 'acceptanceReceipt': RECEIPTS[mode], 'readReceipt': False}
    if mode in ('codex', 'opencode'):
        value['wakePrerequisite'] = 'loaded-thread' if mode == 'codex' else 'existing-idle-session-in-workspace'
    if mode == 'codex':
        value['activeInjection'] = True
    return value

def loopback(endpoint, scheme, allow_localhost):
    uri = urllib.parse.urlsplit(endpoint)
    host = uri.hostname
    try:
        ip = ipaddress.ip_address('127.0.0.1' if allow_localhost and host == 'localhost' else host)
        port = uri.port
        valid = ip.is_loopback and port and uri.scheme == scheme and uri.path in ('', '/') and not uri.query and not uri.fragment and not uri.username and not uri.password and '@' not in uri.netloc and '#' not in endpoint and '?' not in endpoint
    except (ValueError, TypeError):
        valid = False
    if not valid:
        raise ValueError('endpoint must be %s://<loopback-IP%s>:<port>/; no credentials, path, query or fragment' % (scheme, '|localhost' if allow_localhost else ''))
    return str(ip), port

def owned_socket(path):
    if not hasattr(socket, 'AF_UNIX') or not hasattr(os, 'geteuid'):
        raise ValueError('Native Unix socket delivery is unavailable on this platform; use host hooks')
    info = os.lstat(path)
    if not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.geteuid():
        raise ValueError('Endpoint must be a socket owned by this OS user, not a symlink')

def validate(mode, endpoint=None):
    if mode == 'raw':
        if endpoint is not None:
            raise ValueError('Raw hooks do not have an endpoint')
        return
    if not endpoint:
        raise ValueError('Native attachment requires endpoint')
    if mode == 'opencode':
        loopback(endpoint, 'http', False)
    elif mode in ('claude', 'grok') or (mode == 'codex' and endpoint.startswith('unix://')):
        path = endpoint.removeprefix('unix://')
        if not os.path.isabs(path) or len(os.fsencode(path)) > 103:
            raise ValueError('Unix socket endpoint must be an absolute path of at most 103 bytes')
        if mode == 'grok':
            owned_socket(path)
    elif mode == 'codex':
        loopback(endpoint, 'ws', True)
    else:
        raise ValueError('Unknown native transport')

def validate_session(session):
    if not re.fullmatch(r'ses_[A-Za-z0-9_]{1,252}', session):
        raise ValueError('OpenCode vendorSession must be its existing ses_ identifier')

def unix(path):
    owned_socket(path)
    stream = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    stream.settimeout(5)
    stream.connect(path)
    return stream

def codex_peer_turn(session, content):
    return {'threadId': session, 'input': [], 'toolOutput': {'name': 'octocode_peer_messages', 'output': content}}

def codex_peer_items(session, content):
    return {'threadId': session, 'items': [{'type': 'function_call_output', 'name': 'octocode_peer_messages', 'output': content}]}

class WebSocket:
    """RFC 6455 client for local plaintext sockets, with strict bounded framing."""
    LIMIT = 1024 * 1024
    def __init__(self, endpoint):
        validate('codex', endpoint)
        self.socket = unix(endpoint[7:]) if endpoint.startswith('unix://') else socket.create_connection(loopback(endpoint, 'ws', True), timeout=3)
        self.deadline = time.monotonic() + 5
        self.buffer = bytearray()
        key = base64.b64encode(os.urandom(16)).decode()
        host = 'localhost' if endpoint.startswith('unix://') else urllib.parse.urlsplit(endpoint).netloc
        request = 'GET / HTTP/1.1\r\nHost: %s\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: %s\r\nSec-WebSocket-Version: 13\r\n\r\n' % (host, key)
        self.write(request.encode())
        while b'\r\n\r\n' not in self.buffer:
            if len(self.buffer) > 16384:
                raise ValueError('WebSocket response headers too large')
            self.buffer.extend(self.recv(4096))
        headers, remainder = bytes(self.buffer).split(b'\r\n\r\n', 1)
        self.buffer = bytearray(remainder)
        lines = headers.decode('latin1').split('\r\n')
        fields = dict(line.split(':', 1) for line in lines[1:])
        fields = {k.lower(): v.strip() for k, v in fields.items()}
        expected = base64.b64encode(hashlib.sha1((key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest()).decode()
        if len(lines[0].split()) < 2 or lines[0].split()[1] != '101' or fields.get('sec-websocket-accept') != expected or fields.get('upgrade', '').lower() != 'websocket' or 'upgrade' not in fields.get('connection', '').lower():
            raise ValueError('Invalid WebSocket upgrade response')
    def timeout(self):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('Codex I/O timed out')
        self.socket.settimeout(remaining)
    def recv(self, size):
        self.timeout()
        data = self.socket.recv(size)
        if not data:
            raise EOFError('Codex endpoint closed')
        return data
    def exact(self, size):
        while len(self.buffer) < size:
            self.buffer.extend(self.recv(min(65536, size - len(self.buffer))))
        value = bytes(self.buffer[:size])
        del self.buffer[:size]
        return value
    def write(self, data):
        self.timeout()
        self.socket.sendall(data)
    def send_frame(self, opcode, payload):
        if len(payload) > self.LIMIT:
            raise ValueError('Codex frame exceeds 1 MiB')
        mask = os.urandom(4)
        size = len(payload)
        header = bytes([0x80 | opcode, 0x80 | size]) if size < 126 else (bytes([0x80 | opcode, 0xFE]) + struct.pack('!H', size) if size <= 65535 else bytes([0x80 | opcode, 0xFF]) + struct.pack('!Q', size))
        self.write(header + mask + bytes(c ^ mask[i % 4] for i, c in enumerate(payload)))
    def send(self, value):
        self.send_frame(1, encode(value))
    def read(self):
        chunks = bytearray()
        active = None
        while True:
            first, second = self.exact(2)
            opcode, final = first & 15, bool(first & 128)
            if first & 112 or second & 128:
                raise ValueError('Invalid WebSocket server frame')
            size = second & 127
            if size == 126:
                size = struct.unpack('!H', self.exact(2))[0]
            elif size == 127:
                size = struct.unpack('!Q', self.exact(8))[0]
            if size > self.LIMIT or len(chunks) + size > self.LIMIT:
                raise ValueError('Codex frame exceeds 1 MiB')
            if opcode >= 8 and (not final or size > 125):
                raise ValueError('Invalid WebSocket control frame')
            payload = self.exact(size)
            if opcode == 8:
                raise EOFError('Codex endpoint closed')
            if opcode == 9:
                self.send_frame(10, payload)
                continue
            if opcode == 10:
                continue
            if opcode in (1, 2):
                if active is not None:
                    raise ValueError('Unexpected WebSocket message start')
                active = opcode
            elif opcode != 0 or active is None:
                raise ValueError('Unexpected WebSocket continuation')
            chunks.extend(payload)
            if final:
                if active == 1:
                    return json.loads(chunks.decode('utf-8'))
                chunks.clear()
                active = None
    def close(self):
        self.socket.close()

class Codex:
    def __init__(self, endpoint):
        self.socket, self.sequence = WebSocket(endpoint), 0
        self.request('initialize', {'clientInfo': {'name': 'octocode-communication-dispatch', 'version': '0.1.0'}, 'capabilities': {'experimentalApi': True}})
        self.socket.send({'method': 'initialized', 'params': {}})
    def request(self, method, params):
        self.sequence += 1
        identity = self.sequence
        self.socket.deadline = time.monotonic() + 5
        self.socket.send({'id': identity, 'method': method, 'params': params})
        while True:
            value = self.socket.read()
            if value.get('id') == identity and 'method' not in value:
                if value.get('error') is not None:
                    raise RuntimeError('Codex %s: %s' % (method, value['error']))
                return value.get('result')
            if 'id' in value and 'method' in value:
                self.socket.send(refusal(value['id']))
    def ready(self, session, workspace):
        thread = self.request('thread/read', {'threadId': session, 'includeTurns': False}).get('thread', {})
        if thread.get('id') != session:
            raise ValueError('Codex thread/read returned a different or missing thread identity')
        if not thread.get('cwd') or str(Path(thread['cwd']).resolve(strict=True)) != workspace:
            raise ValueError('Codex thread/read returned a different workspace')
        status = thread.get('status', {}).get('type')
        if status not in ('idle', 'active', 'notLoaded', 'systemError'):
            raise ValueError('Codex thread/read returned an unknown runtime status')
        return status in ('idle', 'active')
    def start_turn(self, session, content):
        turn = self.request('turn/start', codex_peer_turn(session, content)).get('turn', {})
        if not turn.get('id') or turn.get('status') not in ('inProgress', 'completed', 'failed', 'interrupted'):
            raise ValueError('Codex turn/start returned no valid acceptance receipt; inspect before retrying')
    def inject(self, session, content):
        self.request('thread/inject_items', codex_peer_items(session, content))
    def close(self):
        self.socket.close()

def claude(endpoint, session, token, content):
    path = endpoint.removeprefix('unix://')
    with unix(path) as stream:
        if os.environ.get('CLAUDE_CODE_MESSAGING_SOCKET') == path and 'CLAUDE_CODE_MESSAGING_TOKEN' in os.environ:
            stream.sendall(encode({'type': 'auth', 'token': os.environ['CLAUDE_CODE_MESSAGING_TOKEN']}) + b'\n')
        stream.sendall(encode({'type': 'user', 'session_id': session, 'from': 'octocode-communication', 'priority': 'next', 'uuid': token, 'msg_id': token, 'message': {'role': 'user', 'content': content}}) + b'\n')
        stream.shutdown(socket.SHUT_WR)


class DeadlineSocket(socket.socket):
    """Bound the full HTTP exchange, including a peer trickling body/header bytes."""
    def remaining(self):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('Native I/O timed out')
        self.settimeout(remaining)
    def recv(self, *args):
        self.remaining()
        return super().recv(*args)
    def recv_into(self, *args):
        self.remaining()
        return super().recv_into(*args)
    def sendall(self, *args):
        self.remaining()
        return super().sendall(*args)

def deadline_connection(address, timeout=None, source_address=None):
    deadline = time.monotonic() + 5
    original = socket.create_connection(address, min(timeout or 2, 2), source_address)
    stream = DeadlineSocket(original.family, original.type, original.proto, fileno=original.detach())
    stream.deadline = deadline
    return stream

class OpenCode:
    def __init__(self, endpoint, session, workspace):
        validate('opencode', endpoint)
        validate_session(session)
        self.endpoint, self.session, self.workspace = endpoint, session, workspace
        self.host, self.port = loopback(endpoint, 'http', False)
        self.headers = {}
        password = os.environ.get('OPENCODE_SERVER_PASSWORD', '')
        if password:
            if os.environ.get('OCTOCODE_OPENCODE_AUTH_ENDPOINT') != endpoint:
                raise ValueError('Set OCTOCODE_OPENCODE_AUTH_ENDPOINT to the exact attached endpoint before using OPENCODE_SERVER_PASSWORD')
            username = os.environ.get('OPENCODE_SERVER_USERNAME', 'opencode')
            if not username or ':' in username or any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in username + password) or len((username + password).encode()) > 8192:
                raise ValueError('Invalid OpenCode authentication configuration')
            self.headers['Authorization'] = 'Basic ' + base64.b64encode((username + ':' + password).encode()).decode()
        self.connection = http.client.HTTPConnection(self.host, self.port, timeout=5)
        self.connection._create_connection = deadline_connection
    def request(self, method, path, body=None):
        connection = self.connection
        if connection.sock is not None:
            connection.sock.deadline = time.monotonic() + 5
        try:
            headers = dict(self.headers)
            data = encode(body) if body is not None else None
            if data is not None:
                headers['Content-Type'] = 'application/json'
            connection.request(method, path + '?' + urllib.parse.urlencode({'directory': self.workspace}), body=data, headers=headers)
            response = connection.getresponse()
            if sum(len(k) + len(v) + 4 for k, v in response.getheaders()) > 16384:
                raise ValueError('OpenCode response headers too large')
            content = response.read(1024 * 1024 + 1)
            if len(content) > 1024 * 1024:
                raise ValueError('OpenCode response exceeds 1 MiB')
            return response.status, content
        except BaseException:
            connection.close()
            raise
    def get(self, path):
        status, content = self.request('GET', path)
        if status != 200:
            raise RuntimeError('OpenCode preflight returned HTTP %s; no message submitted' % status)
        return json.loads(content)
    def idle(self):
        session = self.get('/session/' + self.session)
        if session.get('id') != self.session or not session.get('directory') or str(Path(session['directory']).resolve(strict=True)) != self.workspace:
            raise ValueError('OpenCode session/workspace mismatch; no message submitted')
        statuses = self.get('/session/status')
        if not isinstance(statuses, dict):
            raise ValueError('Invalid OpenCode status map; no message submitted')
        status = statuses.get(self.session, {'type': 'idle'}).get('type')
        if status not in ('idle', 'busy', 'retry'):
            raise ValueError('Unknown OpenCode runtime status; no message submitted')
        return status == 'idle'
    def submit(self, content, action):
        status, body = self.request('POST', '/session/%s/%s' % (self.session, 'prompt_async' if action else 'message'), {'noReply': not action, 'parts': [{'type': 'text', 'text': content}]})
        if status != (204 if action else 200):
            raise RuntimeError('OpenCode submission returned HTTP %s; inspect before retrying' % status)
        if not action:
            receipt = json.loads(body)
            info = receipt.get('info', {})
            if info.get('sessionID') != self.session or info.get('role') != 'user' or not str(info.get('id', '')).startswith('msg_') or not any(part.get('type') == 'text' and part.get('text') == content for part in receipt.get('parts', [])):
                raise ValueError('OpenCode receipt did not match submitted message/session; inspect before retrying')
    def close(self):
        self.connection.close()

class Grok:
    def __init__(self, endpoint, session, workspace):
        validate('grok', endpoint)
        uuid.UUID(session)
        self.socket = unix(endpoint.removeprefix('unix://'))
        self.session, self.buffer, self.sequence, self.pending = session, bytearray(), 0, None
        deadline = time.monotonic() + 10
        self.send_frame({'type': 'register', 'client_type': 'octocode-communication', 'mode': 'stdio', 'capabilities': {}})
        registered = False
        while time.monotonic() < deadline:
            frame = self.frame()
            if frame is None:
                time.sleep(.005)
                continue
            if frame.get('type') == 'registered':
                if frame.get('leader_protocol_version') != 1:
                    raise ValueError('Unsupported Grok leader protocol; version 1 is required')
                registered = True
                if frame.get('ready') is not False:
                    break
            elif frame.get('type') == 'leader_ready' and registered:
                break
            else:
                raise ValueError('Unexpected Grok leader registration response')
        else:
            raise TimeoutError('Grok leader registration timed out')
        result = self.request('initialize', {'protocolVersion': 1, 'clientInfo': {'name': 'octocode-communication', 'version': '0.1.0'}, 'clientCapabilities': {}}, deadline)
        if result.get('protocolVersion') != 1:
            raise ValueError('Unsupported Grok ACP protocol; version 1 is required')
        info = self.request('_x.ai/session/info', {'sessionId': session}, deadline).get('result', {})
        if info.get('sessionId') != session or not info.get('cwd') or str(Path(info['cwd']).resolve(strict=True)) != workspace:
            raise ValueError('Grok session metadata does not match identity/workspace; no message staged')
    def send_frame(self, value):
        data = encode(value)
        if len(data) > MAX_FRAME:
            raise ValueError('Grok frame exceeds 8 MiB')
        self.socket.settimeout(5)
        self.socket.sendall(struct.pack('!I', len(data)) + data)
    def send_rpc(self, value):
        self.send_frame({'type': 'acp', 'payload': encode(value).decode()})
    def frame(self):
        while True:
            if len(self.buffer) >= 4:
                size = struct.unpack('!I', self.buffer[:4])[0]
                if size > MAX_FRAME:
                    raise ValueError('Grok frame exceeds 8 MiB')
                if len(self.buffer) >= size + 4:
                    value = json.loads(bytes(self.buffer[4:size + 4]))
                    del self.buffer[:size + 4]
                    return value
            self.socket.settimeout(.001)
            try:
                chunk = self.socket.recv(65536)
            except (socket.timeout, BlockingIOError, InterruptedError):
                return None
            if not chunk:
                raise EOFError('Grok leader closed; delivery may have succeeded; inspect before retrying')
            self.buffer.extend(chunk)
    def response(self, identity):
        for _ in range(64):
            frame = self.frame()
            if frame is None:
                return None
            if frame.get('type') == 'acp':
                rpc = json.loads(frame['payload'])
                if 'method' in rpc and 'id' in rpc:
                    self.send_rpc(dict(refusal(rpc['id']), jsonrpc='2.0'))
                elif rpc.get('id') == identity:
                    if 'error' in rpc:
                        raise RuntimeError('Grok ACP request failed: ' + json.dumps(rpc['error']))
                    if 'result' not in rpc:
                        raise ValueError('Grok ACP response lacks result')
                    return rpc['result']
            elif frame.get('type') != 'pong':
                raise ValueError('Unsupported Grok leader frame')
        return None
    def request(self, method, params, deadline):
        self.sequence += 1
        self.send_rpc({'jsonrpc': '2.0', 'id': self.sequence, 'method': method, 'params': params})
        while time.monotonic() < deadline:
            result = self.response(self.sequence)
            if result is not None:
                return result
            time.sleep(.005)
        raise TimeoutError('Grok %s timed out' % method)
    def submit(self, content, token):
        if self.pending:
            raise RuntimeError('Grok already has an in-flight delivery')
        self.sequence += 1
        self.pending = self.sequence, time.monotonic() + 300
        self.send_rpc({'jsonrpc': '2.0', 'id': self.sequence, 'method': 'session/prompt', 'params': {'sessionId': self.session, 'prompt': [{'type': 'text', 'text': content}], '_meta': {'verbatim': True, 'promptId': token}}})
    def poll(self):
        if not self.pending:
            return None
        identity, deadline = self.pending
        value = self.response(identity)
        if value is not None:
            if not isinstance(value.get('stopReason'), str):
                raise ValueError('Grok prompt response lacks stopReason')
            self.pending = None
            return value
        if time.monotonic() >= deadline:
            raise TimeoutError('Grok prompt timed out; delivery may have succeeded; inspect before retrying')
        return None
    def close(self):
        self.socket.close()

class NativeDelivery:
    def __init__(self, mode, endpoint, session, workspace):
        if mode not in RECEIPTS:
            raise ValueError('Unknown native transport: ' + mode)
        self.mode, self.endpoint, self.session, self.workspace = mode, endpoint, session, workspace
        self.backend = Codex(endpoint) if mode == 'codex' else Grok(endpoint, session, workspace) if mode == 'grok' else OpenCode(endpoint, session, workspace) if mode == 'opencode' else None
    def matches(self, mode, endpoint, session, workspace):
        return (self.mode, self.endpoint, self.session, self.workspace) == (mode, endpoint, session, workspace)
    def prepare(self):
        return self.backend.ready(self.session, self.workspace) if self.mode == 'codex' else self.backend.idle() if self.mode == 'opencode' else True
    def offer(self, content, token, action):
        if requires_action(self.mode) and not action:
            raise ValueError(self.mode + ' cannot inject passive-only context')
        if self.mode == 'claude':
            claude(self.endpoint, self.session, token, content)
        elif self.mode == 'codex':
            (self.backend.start_turn if action else self.backend.inject)(self.session, content)
        elif self.mode == 'opencode':
            self.backend.submit(content, action)
        else:
            self.backend.submit(content, token)
            return None
        return {'kind': RECEIPTS[self.mode], 'turn_requested': action}
    def poll(self, token):
        receipt = self.backend.poll()
        if receipt is None:
            return None
        result = {'kind': RECEIPTS[self.mode], 'turn_requested': True, 'stop_reason': receipt['stopReason']}
        meta, record = receipt.get('_meta', {}), {'key': 'grok-' + token, 'scope': 'turn'}
        for field, source in [('inputTokens', 'inputTokens'), ('outputTokens', 'outputTokens'), ('cachedInputTokens', 'cachedReadTokens'), ('cacheWriteTokens', 'cacheCreationTokens')]:
            value = meta.get('usage', {}).get(source)
            if type(value) is int and value >= 0:
                record[field] = value
        if len(record) > 2:
            model = meta.get('modelId')
            if isinstance(model, str) and 0 < len(model) <= 256:
                record['model'] = model
            result['usage'] = record
        return result
    def close(self):
        if self.backend:
            self.backend.close()
