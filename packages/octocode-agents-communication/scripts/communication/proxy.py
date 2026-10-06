"""Explicitly launched, isolated vendor workers using the bound communication tools."""
import hashlib
import os
import shutil
import sys
import tempfile
import time
import uuid
from pathlib import Path
from . import catalog, database, dispatch, timing, transport
from .wire import Wire, encode, refusal

PROXY_INSTRUCTIONS = (
    'The host manages your identity, presence, live lease renewal and delivery. '
    'Use bound tools for the user task. '
    'Initiate messages, broadcasts or subscriptions only when the task authorizes them. '
    'Follow the supplied skill for leases, replies and completion; unfinished work stays pending. '
    'Call inbox only for requested recovery. '
    'When no actionable work remains, end the turn. '
    'The host waits for mail and starts the next turn; do not sleep or poll to remain active.'
)
USAGE_FIELDS = (
    ('inputTokens', ('inputTokens', 'input_tokens', 'input')),
    ('outputTokens', ('outputTokens', 'output_tokens', 'output')),
    ('cachedInputTokens', ('cachedInputTokens', 'cache_read_input_tokens', 'cacheRead')),
    ('cacheWriteTokens', ('cacheWriteTokens', 'cache_creation_input_tokens', 'cacheWrite')),
)
PI_CONTEXT_FIELDS = ('input', 'cacheRead', 'cacheWrite')


def emit(record):
    from .cli import output
    output(record)


def emit_texts(message):
    for piece in message.get('content', []):
        if piece.get('type') == 'text':
            emit({'type': 'text', 'text': piece.get('text')})


def run(args):
    from .store import Store
    if not args.vendor:
        raise ValueError('--vendor required')
    input = {'vendor': args.vendor, 'model': args.model, 'prompt': args.prompt}
    if args.duration_ms is not None:
        input['durationMs'] = args.duration_ms
    if args.name:
        input['name'] = args.name
    catalog.command('run', input)
    model, prompt = catalog.text(input, 'model'), catalog.text(input, 'prompt')
    store = Store(database.path(args.database), args.workspace, create=True)
    session = store.call(args.session, 'resume', {'vendor': args.vendor}) if args.session else store.call('', 'join', {'vendor': args.vendor, 'name': args.name or args.vendor})
    identity = session['id']
    store.call(identity, 'heartbeat', {'renewLeases': True, 'task': (prompt.splitlines() or [''])[0][:256], 'status': 'busy'})
    try:
        worker(args, store, identity, args.vendor, model, prompt)
    finally:
        store.call(identity, 'leave', {})
        store.db.close()


def worker(args, store, identity, vendor, model, prompt):
    with dispatch.claim_delivery_owner(store.database, identity), tempfile.TemporaryDirectory() as directory:
        store.attach(identity, {'transport': 'raw'})
        stop = dispatch.stop_event()
        deadline = timing.deadline_after(args.duration_ms)
        parent = os.getppid()
        handler = WORKERS[vendor](args, store, identity, model, prompt, directory)
        loop = Loop(handler, stop, deadline, parent)
        with Wire(vendor, handler.argv(), handler.cwd, handler.environment, deadline, stop) as host:
            handler.host = host
            loop.launch()
            loop.run()


class VendorWorker:
    """One vendor's launch arguments, handshake, event protocol and delivery."""
    name = ''
    deliver_while_busy = True

    def __init__(self, args, store, identity, model, prompt, directory):
        self.args, self.store, self.identity = args, store, identity
        self.model, self.prompt, self.directory = model, prompt, directory
        self.tools = catalog.selected_tools(args.tools)
        self.scripts = Path(__file__).resolve().parent.parent
        self.binary = str(self.scripts / 'communication.py')
        mcp_args = ['mcp', '--workspace', store.workspace, '--database', str(store.database), '--session', identity]
        if args.tools:
            mcp_args += ['--tools', args.tools]
        self.mcp = {'command': sys.executable, 'args': ['-B', self.binary] + mcp_args}
        self.tool_names = [t['name'] for t in self.tools]
        self.cwd = str(Path(directory).resolve())
        self.environment = {}
        self.host, self.session, self.busy = None, '', True

    def heartbeat(self, **fields):
        self.store.call(self.identity, 'heartbeat', dict({'renewLeases': True}, **fields))

    def argv(self):
        raise NotImplementedError

    def start(self):
        """Vendor handshake before the first turn; may bind the vendor session."""

    def prime(self, peers):
        """Returns the peer context still owed to the first turn."""
        return peers

    def begin(self, guidance):
        self.deliver(guidance)

    def deliver(self, content, action=True):
        raise NotImplementedError

    def offer(self, items, content):
        self.deliver(content, action=any(item.get('wake') == 'action' for item in items))

    def stage_action_only(self):
        return None

    def on_event(self, event, params, message, item, turn):
        raise NotImplementedError

    def completed(self):
        self.busy = False
        emit({'type': 'turn-completed', 'vendor': self.name})


class CodexWorker(VendorWorker):
    name = 'codex'

    def argv(self):
        return ['app-server']

    def start(self):
        host = self.host
        host.request('initialize', {'clientInfo': {'name': 'octocode-agents-communication', 'version': '0.1.0'}})
        host.send({'method': 'initialized', 'params': {}})
        self.heartbeat()
        config = host.request('config/read', {'includeLayers': False}).get('config', {})
        plugins = {key: {'enabled': False} for key in config.get('plugins', {})}
        servers = {key: {'enabled': False} for key in config.get('mcp_servers', {})}
        servers['agents_communication'] = dict(self.mcp, enabled=True, enabled_tools=self.tool_names, default_tools_approval_mode='approve')
        skills = self.disabled_skills()
        self.heartbeat()
        started = host.request('thread/start', self.thread(servers, plugins, skills))
        self.session = started.get('thread', {}).get('id')
        if not self.session:
            raise ValueError('No vendor session ID')
        self.heartbeat(vendorSession=self.session)

    def disabled_skills(self):
        discovered = self.host.request('skills/list', {'cwds': [self.cwd], 'forceReload': True})
        if not isinstance(discovered.get('data'), list):
            raise ValueError('Invalid Codex skills list')
        skills = []
        for entry in discovered['data']:
            if not isinstance(entry.get('skills'), list):
                raise ValueError('Invalid Codex skills entry')
            for skill in entry['skills']:
                if not isinstance(skill.get('path'), str):
                    raise ValueError('Missing Codex skill path')
                skills.append({'path': skill['path'], 'enabled': False})
        return skills

    def thread(self, servers, plugins, skills):
        features = {
            'code_mode': {'enabled': False}, 'shell_tool': False, 'apply_patch_freeform': False,
            'multi_agent': False, 'memories': False, 'hooks': False, 'apps': False, 'skill_search': False,
        }
        config = {
            'mcp_servers': servers, 'plugins': plugins, 'project_doc_max_bytes': 0,
            'skills': {'config': skills}, 'web_search': 'disabled', 'features': features,
        }
        return {
            'model': self.model, 'cwd': self.cwd, 'approvalPolicy': 'never', 'sandbox': 'read-only',
            'ephemeral': True, 'baseInstructions': PROXY_INSTRUCTIONS, 'developerInstructions': '',
            'config': config,
        }

    def prime(self, peers):
        if peers:
            self.host.request('thread/inject_items', transport.codex_peer_items(self.session, peers))
        return ''

    def begin(self, guidance):
        self.host.request('turn/start', {'threadId': self.session, 'input': [{'type': 'text', 'text': guidance}], 'effort': 'low'})

    def deliver(self, content, action=True):
        if action:
            self.host.request('turn/start', transport.codex_peer_turn(self.session, content))
        else:
            self.host.request('thread/inject_items', transport.codex_peer_items(self.session, content))

    def stage_action_only(self):
        return False if self.busy else None

    def on_event(self, event, params, message, item, turn):
        method = event.get('method')
        if method == 'item/completed' and item.get('type') == 'agentMessage':
            emit({'type': 'text', 'text': item.get('text')})
        if method == 'turn/completed':
            if turn.get('status') == 'failed':
                raise RuntimeError('Vendor turn failed: ' + str(turn.get('error')))
            self.completed()
        if method == 'error' and params.get('willRetry') is not True:
            raise RuntimeError('Vendor error: ' + str(params))


class PiWorker(VendorWorker):
    name = 'pi'
    deliver_while_busy = False

    def __init__(self, *args):
        super().__init__(*args)
        self.extension = str(Path(self.directory) / 'communication.mjs')
        shutil.copyfile(self.scripts / 'pi-extension.mjs', self.extension)
        shutil.copyfile(self.scripts / 'cli-command.mjs', Path(self.directory) / 'cli-command.mjs')
        binding = {'binary': self.binary, 'workspace': self.store.workspace, 'database': str(self.store.database), 'session': self.identity, 'tools': self.tools}
        self.environment['OCTOCODE_COMMUNICATION_BINDING'] = encode(binding).decode()
        self.error = None

    def argv(self):
        return [
            '--mode', 'rpc', '--model', self.model, '--thinking', 'off',
            '--system-prompt', PROXY_INSTRUCTIONS,
            '--no-session', '--no-extensions', '--no-skills', '--no-prompt-templates',
            '--no-context-files', '--no-builtin-tools',
            '--extension', self.extension,
        ]

    def start(self):
        self.session = self.host.pi_request('get_state', {}).get('sessionId')
        if not self.session:
            raise ValueError('No Pi session ID')
        self.heartbeat(vendorSession=self.session)

    def deliver(self, content, action=True):
        self.host.pi_request('prompt', {'message': content})

    def on_event(self, event, params, message, item, turn):
        kind = event.get('type')
        if kind == 'response' and event.get('success') is False:
            raise RuntimeError('Pi command failed: ' + str(event.get('error')))
        if kind == 'message_end' and message.get('role') == 'assistant':
            reason = message.get('stopReason')
            failed = reason in ('error', 'aborted')
            self.error = message.get('errorMessage') if failed else None
            if failed and self.error is None:
                self.error = reason
            emit_texts(message)
        if kind == 'agent_settled':
            if self.error is not None:
                raise RuntimeError('Pi turn failed: ' + str(self.error))
            self.completed()


class ClaudeWorker(VendorWorker):
    name = 'claude'

    def __init__(self, *args):
        super().__init__(*args)
        self.socket = str(Path(self.cwd) / 'inbox.sock')
        if not hasattr(os, 'geteuid'):
            raise ValueError('Managed Claude peer delivery requires local Unix sockets; use a host-wired communication binding on this platform')
        transport.validate('claude', self.socket)

    def argv(self):
        settings = encode({'disableAllHooks': True, 'autoMemoryEnabled': False}).decode()
        servers = encode({'mcpServers': {'agents_communication': self.mcp}}).decode()
        return [
            '-p', '--messaging-socket-path', self.socket,
            '--settings', settings,
            '--system-prompt', PROXY_INSTRUCTIONS,
            '--model', self.model,
            '--input-format', 'stream-json', '--output-format', 'stream-json', '--verbose',
            '--no-session-persistence', '--setting-sources', '',
            '--strict-mcp-config', '--mcp-config', servers,
            '--tools', '', '--allowedTools', 'mcp__agents_communication__*',
            '--permission-mode', 'dontAsk', '--disable-slash-commands',
        ]

    def deliver(self, content, action=True):
        self.host.send({'type': 'user', 'message': {'role': 'user', 'content': content}, 'session_id': self.session, 'parent_tool_use_id': None})

    def offer(self, items, content):
        transport.claude(self.socket, self.session, items[0]['dispatchToken'], content)

    def on_event(self, event, params, message, item, turn):
        kind = event.get('type')
        if kind == 'system' and event.get('subtype') == 'init':
            self.session = event.get('session_id')
            if not self.session:
                raise ValueError('No vendor session ID')
            self.heartbeat(vendorSession=self.session)
        if kind == 'assistant':
            emit_texts(message)
        if kind == 'result':
            if event.get('is_error') is True:
                raise RuntimeError('Vendor turn failed: ' + str(event))
            self.completed()


WORKERS = {worker.name: worker for worker in (CodexWorker, PiWorker, ClaudeWorker)}


class Loop:
    """Shared worker lifecycle: first turn, heartbeats, vendor events and staged delivery."""

    def __init__(self, worker, stop, deadline, parent):
        self.worker, self.stop, self.deadline, self.parent = worker, stop, deadline, parent
        self.store, self.identity = worker.store, worker.identity
        self.periodic = timing.Heartbeat(worker.heartbeat)
        self.calls, self.poll = {}, 0

    def launch(self):
        worker = self.worker
        worker.start()
        emit({'type': 'ready', 'session': self.identity, 'vendor': worker.name, 'pid': worker.host.pid, 'leases': 'managed'})
        peers = worker.prime(self.store.peer_context(self.identity, 'worker', 'session'))
        worker.begin(self.guidance(peers))

    def guidance(self, peers):
        worker, store = self.worker, self.store
        base = catalog.worker_skill(worker.args.tools)
        binding = {'session': self.identity, 'workspace': store.workspace,
                   'coordinationScope': store.coordination_scope,
                   'branch': store.known(self.identity, True).get('branch'), 'tools': worker.tool_names,
                   'presence': 'managed', 'leaseRenewal': 'managed', 'delivery': 'managed',
                   'instructionsVersion': 1, 'instructionsSha256': hashlib.sha256(base.encode('utf-8')).hexdigest()}
        template = 'Available communication tools: %s\n\n%s\n\nCommunication binding: %s\n%s\nUser task:\n%s'
        return template % (encode(worker.tool_names).decode(), base, encode(binding).decode(), peers, worker.prompt)

    def run(self):
        self.periodic.reset()
        self.poll = time.monotonic()
        while timing.running(self.stop, self.deadline):
            if os.name != 'nt' and os.getppid() != self.parent:
                break
            self.periodic.tick()
            event = self.worker.host.event(.1)
            if event is not None:
                self.handle(event)
            self.deliver()

    def handle(self, event):
        worker = self.worker
        was_busy = worker.busy
        persist_usage(self.store, self.identity, worker.name, worker.session, event)
        params, message = event.get('params') or {}, event.get('message') or {}
        item, turn = params.get('item') or {}, params.get('turn') or {}
        if worker.args.trace:
            self.trace(event, item, turn)
        worker.on_event(event, params, message, item, turn)
        if 'method' in event and 'id' in event:
            worker.host.send(refusal(event['id']))
        if was_busy and not worker.busy:
            worker.heartbeat(status='available')
            self.periodic.reset()

    def trace(self, event, item, turn):
        trace_usage(self.worker.name, event)
        for record in trace_tools(self.identity, event, self.calls):
            emit(record)
        emit({'type': 'protocol', 'method': event.get('method', event.get('type')), 'itemType': item.get('type'), 'status': turn.get('status')})

    def deliver(self):
        worker = self.worker
        if not worker.session or (worker.busy and not worker.deliver_while_busy) or time.monotonic() - self.poll < .1:
            return
        self.poll = time.monotonic()
        items = self.store.stage(self.identity, 'managed:' + worker.name, action_only=worker.stage_action_only())
        if not items:
            return
        worker.busy = True
        worker.heartbeat(status='busy')
        self.periodic.reset()
        emit({'type': 'delivery', 'messages': [i['id'] for i in items]})
        try:
            worker.offer(items, dispatch.with_peers(items, self.store.peer_context(self.identity, 'worker', 'session')))
        except Exception as error:
            self.store.finish_dispatch(self.identity, items, str(error))
            raise
        self.store.finish_dispatch(self.identity, items)


def trace_usage(vendor, event):
    message, params = event.get('message') or {}, event.get('params') or {}
    if event.get('method') == 'thread/tokenUsage/updated':
        scope, usage = 'thread', params.get('tokenUsage')
    elif event.get('type') == 'result':
        scope, usage = 'result', event.get('usage')
    elif event.get('type') == 'assistant' or (event.get('type') == 'message_end' and message.get('role') == 'assistant'):
        scope, usage = 'message', message.get('usage')
    else:
        return
    if usage is not None:
        emit({'type': 'usage', 'vendor': vendor, 'scope': scope, 'messageId': message.get('id'), 'usage': usage})


def counted(value):
    return type(value) is int and value >= 0


def usage_source(vendor, event, message, params):
    if vendor == 'codex' and event.get('method') == 'thread/tokenUsage/updated':
        return params.get('tokenUsage', {}).get('total'), 'cumulative'
    if vendor == 'claude' and event.get('type') == 'result':
        return event.get('usage'), 'turn'
    if vendor == 'pi' and event.get('type') == 'message_end' and message.get('role') == 'assistant':
        return message.get('usage'), 'request'
    return None, None


def usage_key(vendor_session, usage, scope, event, message):
    if scope == 'cumulative':
        keydata = [vendor_session, usage]
    elif event.get('uuid') is not None or message.get('timestamp') is not None:
        keydata = [vendor_session, event.get('uuid'), message.get('timestamp')]
    else:
        keydata = [vendor_session, str(uuid.uuid4())]
    return hashlib.sha256(encode(keydata)).hexdigest()


def persist_usage(store, identity, vendor, vendor_session, event):
    message, params = event.get('message') or {}, event.get('params') or {}
    usage, scope = usage_source(vendor, event, message, params)
    if not isinstance(usage, dict):
        return
    record = {'key': usage_key(vendor_session, usage, scope, event, message), 'scope': scope}
    for field, names in USAGE_FIELDS:
        key = next((key for key in names if counted(usage.get(key))), None)
        if key is not None:
            record[field] = usage[key]
    value = params.get('tokenUsage', {}).get('last', {}).get('inputTokens')
    if counted(value):
        record['contextTokens'] = value
    if vendor == 'pi' and all(counted(usage.get(key)) for key in PI_CONTEXT_FIELDS):
        record['contextTokens'] = sum(usage[key] for key in PI_CONTEXT_FIELDS)
    store.record_usage(identity, record)


def rpc_tool_records(event):
    item = (event.get('params') or {}).get('item') or {}
    if item.get('type') != 'mcpToolCall':
        return []
    if event.get('method') == 'item/started':
        return [{'type': 'tool-call', 'callId': item.get('id'), 'server': item.get('server'), 'tool': item.get('tool'), 'input': item.get('arguments')}]
    if event.get('method') == 'item/completed':
        return [{'type': 'tool-result', 'callId': item.get('id'), 'server': item.get('server'), 'tool': item.get('tool'), 'result': item.get('result'), 'error': item.get('error')}]
    return []


def execution_tool_records(event):
    if event.get('type') == 'tool_execution_start':
        return [{'type': 'tool-call', 'callId': event.get('toolCallId'), 'tool': event.get('toolName'), 'input': event.get('args')}]
    if event.get('type') == 'tool_execution_end':
        return [{'type': 'tool-result', 'callId': event.get('toolCallId'), 'tool': event.get('toolName'), 'result': event.get('result'), 'isError': event.get('isError')}]
    return []


def message_tool_records(event, calls):
    records = []
    for item in (event.get('message') or {}).get('content', []):
        if not isinstance(item, dict):
            continue
        if item.get('type') == 'tool_use' and isinstance(item.get('id'), str) and isinstance(item.get('name'), str):
            calls[item['id']] = item['name']
            records.append({'type': 'tool-call', 'callId': item['id'], 'tool': item['name'], 'input': item.get('input')})
        elif item.get('type') == 'tool_result':
            records.append({'type': 'tool-result', 'callId': item.get('tool_use_id'), 'tool': calls.pop(item.get('tool_use_id'), None), 'result': item.get('content'), 'isError': item.get('is_error')})
    return records


def trace_tools(session, event, calls):
    records = rpc_tool_records(event) + execution_tool_records(event) + message_tool_records(event, calls)
    for record in records:
        record.update(session=session, at=dispatch.now())
    return records
