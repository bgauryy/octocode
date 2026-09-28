"""Loopback-only, token-scoped read-only dashboard."""
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import sys
import threading
import time
import uuid
from urllib.parse import unquote_to_bytes
from . import catalog, database, view_data
from .store import Store


def respond(stream, status, mime, body):
    headers = (f'HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {len(body)}\r\n'
               "Connection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n"
               "X-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\n"
               "Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'\r\n\r\n")
    stream.settimeout(2)
    stream.sendall(headers.encode() + body)


def decode_query(value):
    if re.search(r'%(?![0-9A-Fa-f]{2})', value):
        raise ValueError('Invalid percent encoding')
    return unquote_to_bytes(value.replace('+', ' ')).decode('utf-8')


def request(stream, store, host, prefix):
    stream.settimeout(.5)
    deadline, data = time.monotonic() + 2, bytearray()
    while b'\r\n\r\n' not in data:
        if len(data) >= 8192 or time.monotonic() > deadline:
            raise ValueError('Request header limit')
        chunk = stream.recv(min(1024, 8192 - len(data)))
        if not chunk:
            return
        data.extend(chunk)
    lines = data.decode('utf-8').split('\r\n')
    first = lines[0].split()
    if len(first) != 3 or first[0] != 'GET':
        return respond(stream, '405 Method Not Allowed', 'text/plain', b'Read-only view: GET only')
    headers = []
    for line in lines[1:]:
        if not line:
            break
        if ':' in line:
            key, value = line.split(':', 1)
            headers.append((key.lower(), value.strip()))
    hosts = [value for key, value in headers if key == 'host']
    if hosts != [host] or any(key == 'origin' and value != 'http://' + host for key, value in headers):
        return respond(stream, '403 Forbidden', 'text/plain', b'Local origin required')
    if not first[1].startswith(prefix):
        return respond(stream, '404 Not Found', 'text/plain', b'Not found')
    route = first[1][len(prefix):]
    assets = {'': ('index.html', 'text/html; charset=utf-8'), 'app.js': ('app.js', 'text/javascript; charset=utf-8'), 'style.css': ('style.css', 'text/css; charset=utf-8')}
    if route in assets:
        name, mime = assets[route]
        return respond(stream, '200 OK', mime, (Path(__file__).parent / 'dashboard' / name).read_bytes())
    try:
        if route == 'api/summary':
            value = view_data.summary(store)
        else:
            path, _, search = route.partition('?')
            if not path.startswith('api/'):
                raise ValueError('Unknown route')
            after, agent, filters = None, None, {}
            for part in filter(None, search.split('&')):
                key, separator, value = part.partition('=')
                if not separator:
                    raise ValueError('Invalid query')
                if key == 'after' and after is None:
                    after = value
                elif key == 'agent' and agent is None:
                    agent = value
                elif key in ('q', 'conversation', 'status') and key not in filters:
                    filters[key] = decode_query(value)
                else:
                    raise ValueError('Unknown or duplicate filter')
            value = view_data.page(store, path[4:], after, agent, filters)
        status = '200 OK'
    except Exception as error:
        status, value = '400 Bad Request', {'error': str(error)}
    return respond(stream, status, 'application/json', json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode())


def run(args, input):
    from .cli import output
    catalog.command('view', input)
    store = Store(database.path(args.database), args.workspace, read_only=True, create=False)
    stop = threading.Event()
    previous = {}
    for sig in (signal.SIGINT, signal.SIGTERM):
        previous[sig] = signal.signal(sig, lambda *_: stop.set())
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
            listener.bind(('127.0.0.1', input.get('port', 0)))
            listener.listen()
            listener.settimeout(.1)
            host = '127.0.0.1:' + str(listener.getsockname()[1])
            prefix = '/' + str(uuid.uuid4()) + '/'
            url = 'http://' + host + prefix
            output(dict(url=url, pid=os.getpid(), workspace=store.workspace, database=str(store.database), readOnly=True, stop='Ctrl+C'))
            if input.get('open', True):
                def browser():
                    command = ['open', url] if sys.platform == 'darwin' else (['cmd', '/C', 'start', '', url] if os.name == 'nt' else ['xdg-open', url])
                    try:
                        if subprocess.call(command, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL):
                            raise OSError('Browser launcher failed')
                    except OSError:
                        print('Browser could not open; use the printed local URL', file=sys.stderr)
                threading.Thread(target=browser, daemon=True).start()
            while not stop.is_set():
                try:
                    stream, _ = listener.accept()
                except socket.timeout:
                    continue
                with stream:
                    try:
                        request(stream, store, host, prefix)
                    except (OSError, ValueError) as error:
                        print('View request ended: ' + str(error), file=sys.stderr)
    finally:
        store.db.close()
        for sig, handler in previous.items():
            signal.signal(sig, handler)
