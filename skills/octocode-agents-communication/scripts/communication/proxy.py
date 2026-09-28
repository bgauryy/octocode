"""Explicitly launched, isolated vendor workers using the bound communication tools."""
import hashlib
import json
import os
import shutil
import sys
import tempfile
import time
import uuid
from pathlib import Path
from . import catalog, database, dispatch, transport
from .wire import Wire, encode, refusal

PROXY_INSTRUCTIONS = 'You are a managed communication worker; the host owns identity, presence, live lease renewal and delivery. Acquire leases before editing and unlock when done; expired leases must be reacquired. Use bound tools for the user task. Initiate messages, broadcasts or subscriptions only when the task authorizes them. Follow the supplied skill for send and complete decisions; incomplete work stays pending. Call inbox only for requested recovery. End each turn after handling delivered work or reporting a blocker.'

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
    from .cli import output
    with dispatch.claim_delivery_owner(store.database, identity), tempfile.TemporaryDirectory() as directory:
        store.attach(identity, {'transport': 'raw'})
        stop = dispatch.stop_event()
        deadline = time.monotonic() + args.duration_ms / 1000 if args.duration_ms is not None else None
        parent = os.getppid()
        tools = catalog.selected_tools(args.tools)
        scripts = Path(__file__).resolve().parent.parent
        binary = str(scripts / 'communication.py')
        mcp_args = ['mcp', '--workspace', store.workspace, '--database', str(store.database), '--session', identity]
        if args.tools:
            mcp_args += ['--tools', args.tools]
        mcp = {'command': sys.executable, 'args': ['-B', binary] + mcp_args}
        tool_names = [t['name'] for t in tools]
        cwd = str(Path(directory).resolve())
        claude_socket = str(Path(cwd) / 'inbox.sock') if vendor == 'claude' else None
        if claude_socket:
            if not hasattr(os, 'geteuid'):
                raise ValueError('Managed Claude peer delivery requires local Unix sockets; use a host-wired communication binding on this platform')
            transport.validate('claude', claude_socket)
        environment = {}
        if vendor == 'pi':
            extension = str(Path(directory) / 'communication.mjs')
            shutil.copyfile(scripts / 'pi-extension.mjs', extension)
            shutil.copyfile(scripts / 'cli-command.mjs', Path(directory) / 'cli-command.mjs')
            environment['OCTOCODE_COMMUNICATION_BINDING'] = encode({'binary': binary, 'workspace': store.workspace, 'database': str(store.database), 'session': identity, 'tools': tools}).decode()
            vendor_args = ['--mode', 'rpc', '--model', model, '--thinking', 'off', '--system-prompt', PROXY_INSTRUCTIONS, '--no-session', '--no-extensions', '--no-skills', '--no-prompt-templates', '--no-context-files', '--no-builtin-tools', '--extension', extension]
        elif vendor == 'codex':
            vendor_args = ['app-server']
        else:
            vendor_args = ['-p', '--messaging-socket-path', claude_socket, '--settings', encode({'disableAllHooks': True, 'autoMemoryEnabled': False}).decode(), '--system-prompt', PROXY_INSTRUCTIONS, '--model', model, '--input-format', 'stream-json', '--output-format', 'stream-json', '--verbose', '--no-session-persistence', '--setting-sources', '', '--strict-mcp-config', '--mcp-config', encode({'mcpServers': {'agents_communication': mcp}}).decode(), '--tools', '', '--allowedTools', 'mcp__agents_communication__*', '--permission-mode', 'dontAsk', '--disable-slash-commands']
        with Wire(vendor, vendor_args, cwd, environment, deadline, stop) as host:
            vendor_session = ''
            if vendor == 'codex':
                host.request('initialize', {'clientInfo': {'name': 'octocode-agents-communication', 'version': '0.1.0'}})
                host.send({'method': 'initialized', 'params': {}})
                store.call(identity, 'heartbeat', {'renewLeases': True})
                config = host.request('config/read', {'includeLayers': False}).get('config', {})
                plugins = {key: {'enabled': False} for key in config.get('plugins', {})}
                servers = {key: {'enabled': False} for key in config.get('mcp_servers', {})}
                servers['agents_communication'] = dict(mcp, enabled=True, enabled_tools=tool_names, default_tools_approval_mode='approve')
                discovered = host.request('skills/list', {'cwds': [cwd], 'forceReload': True})
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
                store.call(identity, 'heartbeat', {'renewLeases': True})
                started = host.request('thread/start', {'model': model, 'cwd': cwd, 'approvalPolicy': 'never', 'sandbox': 'read-only', 'ephemeral': True, 'baseInstructions': PROXY_INSTRUCTIONS, 'developerInstructions': '', 'config': {'mcp_servers': servers, 'plugins': plugins, 'project_doc_max_bytes': 0, 'skills': {'config': skills}, 'web_search': 'disabled', 'features': {'code_mode': {'enabled': False}, 'shell_tool': False, 'apply_patch_freeform': False, 'multi_agent': False, 'memories': False, 'hooks': False, 'apps': False, 'skill_search': False}}})
                vendor_session = started.get('thread', {}).get('id')
                if not vendor_session:
                    raise ValueError('No vendor session ID')
                store.call(identity, 'heartbeat', {'renewLeases': True, 'vendorSession': vendor_session})
            elif vendor == 'pi':
                vendor_session = host.pi_request('get_state', {}).get('sessionId')
                if not vendor_session:
                    raise ValueError('No Pi session ID')
                store.call(identity, 'heartbeat', {'renewLeases': True, 'vendorSession': vendor_session})
            output({'type': 'ready', 'session': identity, 'vendor': vendor, 'pid': host.pid, 'leases': 'managed'})
            peers = store.peer_context(identity, 'worker', 'session')
            if vendor == 'codex' and peers:
                host.request('thread/inject_items', transport.codex_peer_items(vendor_session, peers))
                peers = ''
            guidance = 'Available communication tools: %s\n\n%s\n\nBound communication identity: %s. %s\n\nUser task:\n%s' % (encode(tool_names).decode(), catalog.worker_skill(), identity, peers, prompt)
            if vendor == 'codex':
                host.request('turn/start', {'threadId': vendor_session, 'input': [{'type': 'text', 'text': guidance}], 'effort': 'low'})
            else:
                deliver(host, vendor, vendor_session, guidance)
            busy, heartbeat, poll, calls, pi_error = True, time.monotonic(), time.monotonic(), {}, None
            while not stop.is_set() and (deadline is None or time.monotonic() < deadline):
                if os.name != 'nt' and os.getppid() != parent:
                    break
                if time.monotonic() - heartbeat >= 15:
                    store.call(identity, 'heartbeat', {'renewLeases': True})
                    heartbeat = time.monotonic()
                event = host.event(.1)
                if event is not None:
                    was_busy = busy
                    persist_usage(store, identity, vendor, vendor_session, event)
                    params, message = event.get('params') or {}, event.get('message') or {}
                    item, turn = params.get('item') or {}, params.get('turn') or {}
                    if args.trace:
                        trace_usage(vendor, event)
                        for record in trace_tools(identity, event, calls):
                            output(record)
                        output({'type': 'protocol', 'method': event.get('method', event.get('type')), 'itemType': item.get('type'), 'status': turn.get('status')})
                    if vendor == 'pi':
                        if event.get('type') == 'response' and event.get('success') is False:
                            raise RuntimeError('Pi command failed: ' + str(event.get('error')))
                        if event.get('type') == 'message_end' and message.get('role') == 'assistant':
                            pi_error = message.get('errorMessage') if message.get('stopReason') in ('error', 'aborted') else None
                            if message.get('stopReason') in ('error', 'aborted') and pi_error is None:
                                pi_error = message.get('stopReason')
                            for piece in message.get('content', []):
                                if piece.get('type') == 'text':
                                    output({'type': 'text', 'text': piece.get('text')})
                        if event.get('type') == 'agent_settled':
                            if pi_error is not None:
                                raise RuntimeError('Pi turn failed: ' + str(pi_error))
                            busy = False
                            output({'type': 'turn-completed', 'vendor': vendor})
                    if event.get('method') == 'item/completed' and item.get('type') == 'agentMessage':
                        output({'type': 'text', 'text': item.get('text')})
                    if event.get('method') == 'turn/completed':
                        if turn.get('status') == 'failed':
                            raise RuntimeError('Vendor turn failed: ' + str(turn.get('error')))
                        busy = False
                        output({'type': 'turn-completed', 'vendor': vendor})
                    if event.get('method') == 'error' and params.get('willRetry') is not True:
                        raise RuntimeError('Vendor error: ' + str(params))
                    if 'method' in event and 'id' in event:
                        host.send(refusal(event['id']))
                    if event.get('type') == 'system' and event.get('subtype') == 'init':
                        vendor_session = event.get('session_id')
                        if not vendor_session:
                            raise ValueError('No vendor session ID')
                        store.call(identity, 'heartbeat', {'renewLeases': True, 'vendorSession': vendor_session})
                    if event.get('type') == 'assistant':
                        for piece in message.get('content', []):
                            if piece.get('type') == 'text':
                                output({'type': 'text', 'text': piece.get('text')})
                    if event.get('type') == 'result':
                        if event.get('is_error') is True:
                            raise RuntimeError('Vendor turn failed: ' + str(event))
                        busy = False
                        output({'type': 'turn-completed', 'vendor': vendor})
                    if was_busy and not busy:
                        store.call(identity, 'heartbeat', {'renewLeases': True, 'status': 'available'})
                        heartbeat = time.monotonic()
                if vendor_session and (not busy or vendor in ('claude', 'codex')) and time.monotonic() - poll >= .1:
                    poll = time.monotonic()
                    items = store.stage(identity, 'managed:' + vendor, action_only=False if busy and vendor == 'codex' else None)
                    if items:
                        busy = True
                        store.call(identity, 'heartbeat', {'renewLeases': True, 'status': 'busy'})
                        heartbeat = time.monotonic()
                        output({'type': 'delivery', 'messages': [i['id'] for i in items]})
                        try:
                            content = dispatch.with_peers(items, store.peer_context(identity, 'worker', 'session'))
                            if vendor == 'claude':
                                transport.claude(claude_socket, vendor_session, items[0]['dispatchToken'], content)
                            else:
                                deliver(host, vendor, vendor_session, content, action=any(item.get('wake') == 'action' for item in items))
                        except Exception as error:
                            store.finish_dispatch(identity, items, str(error))
                            raise
                        store.finish_dispatch(identity, items)

def deliver(host, vendor, session, content, action=True):
    if vendor == 'codex':
        host.request('turn/start', transport.codex_peer_turn(session, content)) if action else host.request('thread/inject_items', transport.codex_peer_items(session, content))
    elif vendor == 'pi':
        host.pi_request('prompt', {'message': content})
    else:
        host.send({'type': 'user', 'message': {'role': 'user', 'content': content}, 'session_id': session, 'parent_tool_use_id': None})

def trace_usage(vendor, event):
    from .cli import output
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
        output({'type': 'usage', 'vendor': vendor, 'scope': scope, 'messageId': message.get('id'), 'usage': usage})

def persist_usage(store, identity, vendor, vendor_session, event):
    message, params = event.get('message') or {}, event.get('params') or {}
    if vendor == 'codex' and event.get('method') == 'thread/tokenUsage/updated':
        usage, scope = params.get('tokenUsage', {}).get('total'), 'cumulative'
    elif vendor == 'claude' and event.get('type') == 'result':
        usage, scope = event.get('usage'), 'turn'
    elif vendor == 'pi' and event.get('type') == 'message_end' and message.get('role') == 'assistant':
        usage, scope = message.get('usage'), 'request'
    else:
        return
    if not isinstance(usage, dict):
        return
    keydata = [vendor_session, usage] if scope == 'cumulative' else [vendor_session, event.get('uuid'), message.get('timestamp')] if event.get('uuid') is not None or message.get('timestamp') is not None else [vendor_session, str(uuid.uuid4())]
    record = {'key': hashlib.sha256(encode(keydata)).hexdigest(), 'scope': scope}
    for field, names in [('inputTokens', ('inputTokens', 'input_tokens', 'input')), ('outputTokens', ('outputTokens', 'output_tokens', 'output')), ('cachedInputTokens', ('cachedInputTokens', 'cache_read_input_tokens', 'cacheRead')), ('cacheWriteTokens', ('cacheWriteTokens', 'cache_creation_input_tokens', 'cacheWrite'))]:
        for key in names:
            if type(usage.get(key)) is int and usage[key] >= 0:
                record[field] = usage[key]
                break
    value = params.get('tokenUsage', {}).get('last', {}).get('inputTokens')
    if type(value) is int and value >= 0:
        record['contextTokens'] = value
    if vendor == 'pi' and all(type(usage.get(key)) is int and usage[key] >= 0 for key in ('input', 'cacheRead', 'cacheWrite')):
        record['contextTokens'] = sum(usage[key] for key in ('input', 'cacheRead', 'cacheWrite'))
    store.record_usage(identity, record)

def trace_tools(session, event, calls):
    records = []
    item = (event.get('params') or {}).get('item') or {}
    if item.get('type') == 'mcpToolCall':
        if event.get('method') == 'item/started':
            records.append({'type': 'tool-call', 'callId': item.get('id'), 'server': item.get('server'), 'tool': item.get('tool'), 'input': item.get('arguments')})
        elif event.get('method') == 'item/completed':
            records.append({'type': 'tool-result', 'callId': item.get('id'), 'server': item.get('server'), 'tool': item.get('tool'), 'result': item.get('result'), 'error': item.get('error')})
    if event.get('type') == 'tool_execution_start':
        records.append({'type': 'tool-call', 'callId': event.get('toolCallId'), 'tool': event.get('toolName'), 'input': event.get('args')})
    if event.get('type') == 'tool_execution_end':
        records.append({'type': 'tool-result', 'callId': event.get('toolCallId'), 'tool': event.get('toolName'), 'result': event.get('result'), 'isError': event.get('isError')})
    for item in (event.get('message') or {}).get('content', []):
        if not isinstance(item, dict):
            continue
        if item.get('type') == 'tool_use' and isinstance(item.get('id'), str) and isinstance(item.get('name'), str):
            calls[item['id']] = item['name']
            records.append({'type': 'tool-call', 'callId': item['id'], 'tool': item['name'], 'input': item.get('input')})
        elif item.get('type') == 'tool_result':
            records.append({'type': 'tool-result', 'callId': item.get('tool_use_id'), 'tool': calls.pop(item.get('tool_use_id'), None), 'result': item.get('content'), 'isError': item.get('is_error')})
    for record in records:
        record.update(session=session, at=dispatch.now())
    return records
