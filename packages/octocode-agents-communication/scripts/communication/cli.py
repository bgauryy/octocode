"""CLI ingress: bounded JSON input, catalog discovery and command dispatch."""
import argparse
import json
import sqlite3
import sys
import time
from pathlib import Path
from . import catalog

MAX_FRAME = 8 * 1024 * 1024


def emit(value):
    sys.stdout.write(json.dumps(value, ensure_ascii=False, separators=(',', ':'), allow_nan=False) + '\n')
    sys.stdout.flush()


def output(value):
    from .store import strip_nulls
    emit(strip_nulls(value))


def parse_json(argument='{}'):
    if argument == '-':
        raw = sys.stdin.buffer.read(MAX_FRAME + 1)
        if len(raw) > MAX_FRAME:
            raise ValueError('JSON input exceeds 8 MiB')
        argument = raw.decode('utf-8')
    elif len(argument.encode('utf-8')) > MAX_FRAME:
        raise ValueError('JSON input exceeds 8 MiB')

    def invalid_constant(_):
        raise ValueError('Invalid JSON number')

    try:
        result = json.loads(argument, parse_constant=invalid_constant)
        # Reject lone surrogate escapes; JSON strings and SQLite require valid UTF-8.
        json.dumps(result, ensure_ascii=False).encode('utf-8')
        return result
    except (json.JSONDecodeError, UnicodeError) as error:
        raise ValueError('Invalid JSON input: ' + str(error).split(':', 1)[0]) from None


def _arity(values, minimum, maximum):
    if not minimum <= len(values) <= maximum:
        raise ValueError('Unexpected positional arguments; use --help or schema')


def _arguments(argv=None):
    parser = argparse.ArgumentParser(prog='agents-communication', add_help=False, allow_abbrev=False)
    parser.add_argument('--workspace', type=Path, default=Path('.'))
    parser.add_argument('--database', type=Path)
    for name in ('session', 'vendor', 'model', 'name', 'prompt', 'tools'):
        parser.add_argument('--' + name)
    parser.add_argument('--duration-ms', type=int)
    for name in ('trace', 'managed', 'compact'):
        parser.add_argument('--' + name, action='store_true')
    parser.add_argument('-h', '--help', action='store_true')
    parser.add_argument('-V', '--version', action='version', version='agents-communication 0.1.0 (Python)')
    parser.add_argument('args', nargs='*')
    args = parser.parse_intermixed_args(argv)
    if args.duration_ms is not None and args.duration_ms < 0:
        parser.error('--duration-ms must be nonnegative')
    return args


def run(argv=None):
    from . import database
    from .store import Store
    args = _arguments(argv)
    if args.help or not args.args:
        return output(catalog.help() if not args.args else catalog.definition(' '.join(args.args)))
    command, rest = args.args[0], args.args[1:]
    if args.compact and not (command == 'schema' and rest == ['types']):
        raise ValueError('--compact is supported only for schema types')
    if args.managed and command != 'mcp':
        raise ValueError('--managed is supported only for mcp')
    if args.tools is not None:
        if command not in ('mcp', 'run') and not (command == 'schema' and rest == ['tools']):
            raise ValueError('--tools is supported only for mcp, run, and schema tools')
        catalog.selected_tools(args.tools)
    # Reject flags the selected operation cannot consume before opening storage.
    name = ' '.join(args.args[:2]) if command == 'db' or (command == 'schema' and rest == ['tools']) or (command == 'inbox' and rest and rest[0] == 'wait') else command
    allowed = set(catalog.definition(name).get('cliFlags', [])) | {'help', 'version'}
    for token in (sys.argv[1:] if argv is None else argv):
        if token.startswith('--') and token[2:].split('=', 1)[0] not in allowed:
            raise ValueError('{} is not supported for {}'.format(token.split('=', 1)[0], name))
    if command == 'skill':
        _arity(rest, 0, 0)
        return output({'instructions': catalog.skill_instructions(args.vendor),
                       'packageRoot': str(catalog.SCRIPTS.parent),
                       'referenceRoot': str(catalog.SCRIPTS / 'docs')})
    if command == 'schema':
        if not rest:
            value = catalog.catalog()
        elif rest == ['tools']:
            value = catalog.selected_tools(args.tools)
        elif rest == ['types']:
            value = catalog.type_summary() if args.compact else catalog.catalog()['recordTypes']
        elif len(rest) == 2 and rest[0] == 'type':
            value = catalog.record_type(rest[1])
        else:
            value = catalog.definition(' '.join(rest))
        return emit(value)
    if command in ('mcp', 'run', 'listen', 'host-hook', 'host-config'):
        _arity(rest, 0, 0)
        if command == 'mcp':
            from . import mcp
            return mcp.run(args)
        if command == 'run':
            from . import proxy
            return proxy.run(args)
        if command == 'listen':
            from . import dispatch
            return dispatch.listen(args)
        from . import host_hooks
        return host_hooks.run(args) if command == 'host-hook' else host_hooks.config(args)
    if command == 'view':
        _arity(rest, 0, 1)
        value = parse_json(rest[0] if rest else '{}')
        catalog.command('view', value)
        from . import view
        return view.run(args, value)
    if command == 'db':
        _arity(rest, 1, 2)
        action = rest[0]
        if action == 'protocol':
            _arity(rest, 1, 1)
            return output({'protocol': (catalog.SCRIPTS / 'docs/DB.md').read_text(encoding='utf-8'), 'database': catalog.catalog()['database']})
        if action == 'info':
            _arity(rest, 1, 1)
            return output(database.inspect(database.path(args.database), args.workspace))
        if action not in ('retention', 'compact', 'export', 'migrate'):
            raise ValueError('Use db info, db protocol, db export, db retention, db compact, or db migrate')
        if action == 'export':
            _arity(rest, 2, 2)
        value = parse_json(rest[1] if len(rest) == 2 else '{}')
        catalog.command('db ' + action, value)
        path = database.path(args.database)
        if action == 'migrate':
            from .migration import migrate
            return output(migrate(path, Path(value['backup'])))
        if action == 'export':
            return output(database.export(path, Path(catalog.text(value, 'path'))))
        from . import retention
        return output(retention.report(path, value) if action == 'retention' else retention.compact(path))
    wait = command == 'inbox' and bool(rest) and rest[0] == 'wait'
    _arity(rest, int(wait), 2 if wait else 1)
    value = parse_json(rest[int(wait)] if len(rest) > int(wait) else '{}')
    name = 'inbox wait' if wait else command
    catalog.command(name, value)
    if command == 'activity':
        from . import activity
        return output(activity.read(args.workspace, value))
    session = args.session or ''
    if command not in ('join', 'peers', 'prune', 'health') and not session.strip():
        raise ValueError('--session required')
    read_only = command in ('binding', 'fetch', 'peers', 'inbox', 'read_document', 'context', 'check_paths', 'locks', 'check_write', 'health', 'completion-check')
    store = Store(database.path(args.database), args.workspace, read_only, command == 'join')
    try:
        if wait:
            deadline = time.monotonic() + value.get('timeoutMs', 30000) / 1000
            while True:
                result = store.inbox(session, value.get('after', 0))
                if result.get('items') or time.monotonic() >= deadline:
                    return output(result)
                time.sleep(min(0.25, max(0, deadline - time.monotonic())))
        if command in ('attach', 'retry_delivery', 'record_usage'):
            return output(getattr(store, command)(session, value))
        if command == 'completion-check':
            return output(store.completion_check(session, value))
        if command == 'confirm_delivery':
            store.finish_dispatch(session, value['items'], None)
            return output({'submitted': True})
        if command in ('hook', 'dispatch'):
            from . import dispatch
            if command == 'hook':
                return dispatch.hook(store, session, value)
            return output(dispatch.dispatch(store, session))
        return output(store.call(session, command, value))
    finally:
        store.db.close()


def main():
    try:
        run()
    except BrokenPipeError:
        # A consumer closing a pipe is normal; suppress shutdown's second flush.
        import os
        os.dup2(os.open(os.devnull, os.O_WRONLY), sys.stdout.fileno())
    except KeyboardInterrupt:
        raise SystemExit(130) from None
    except (ValueError, OSError, sqlite3.Error, RuntimeError) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1) from None
    except Exception as error:
        # Same exit contract as expected errors; the type names the defect for a report.
        print('Internal error ({}): {}'.format(type(error).__name__, error), file=sys.stderr)
        raise SystemExit(1) from None
