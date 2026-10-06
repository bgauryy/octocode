"""One catalog supplies validation, command help, MCP and host tool discovery."""
import copy
import json
from functools import lru_cache
from pathlib import Path
from . import validation

SCRIPTS = Path(__file__).resolve().parent.parent
SKILL = SCRIPTS.parent / 'OPERATING.md'


def _resolve(value, definitions, refs=()):
    if isinstance(value, dict):
        if '$ref' in value:
            ref = value['$ref']
            if not ref.startswith('#/$defs/') or set(value) - {'$ref', 'description'}:
                raise ValueError('Unsupported catalog reference')
            if ref in refs:
                raise ValueError('Circular catalog reference: ' + ref)
            result = _resolve(definitions[ref[len('#/$defs/'):]], definitions, refs + (ref,))
            if 'description' in value:
                result['description'] = value['description']
            return result
        return {key: _resolve(item, definitions, refs) for key, item in value.items()}
    if isinstance(value, list):
        return [_resolve(item, definitions, refs) for item in value]
    return value


@lru_cache(maxsize=1)
def _catalog():
    raw = json.loads((SCRIPTS / 'catalog.json').read_text(encoding='utf-8'))
    value = _resolve(raw, raw.pop('$defs', {}))
    definitions = {item['name']: item for item in value['commands']}
    definitions['record']['inputSchema']['oneOf'] = [
        {'properties': {'type': {'const': item['type']}, 'data': copy.deepcopy(item['dataSchema'])}}
        for item in value['recordTypes'] if item['writer'] == 'record']
    for item in value['commands']:
        if 'inputSchema' in item:
            validation.check_schema(item['inputSchema'])
    for tool in value['tools']:
        source = definitions[tool['name']]
        tool.update(description=source['description'], inputSchema=copy.deepcopy(source['inputSchema']))
    for item in value['recordTypes']:
        validation.check_schema(item['dataSchema'])
        validation.validate(item['dataSchema'], item['example'])
    from . import database
    value['database'] = copy.deepcopy(database.schema())
    value['database']['sql'] = database.SQL
    value['database']['leasePathComparison'] = {
        'algorithm': 'Unicode canonical caseless per component (NFD, full casefold, NFD)',
        'unicodeVersion': [16, 0, 0], 'normalizationUnicodeVersion': [16, 0, 0],
        'policy': 'Case and normalization aliases conflict on every filesystem; access paths and workspace containment stay case-preserving.'}
    value['pagination'] = {'maxItems': 100, 'targetBytes': 16384, 'continuation':
        'Run next.command with next.input unchanged. A single oversized row remains intact with an explicit budget diagnostic.'}
    return value


def catalog():
    return copy.deepcopy(_catalog())


def definition(name):
    for item in _catalog()['commands']:
        if item['name'] == name:
            return copy.deepcopy(item)
    raise ValueError('Unknown command: ' + name + '; use --help')


def record_type(name):
    for item in _catalog()['recordTypes']:
        if item['type'] == name:
            return copy.deepcopy(item)
    raise ValueError('Unknown record type: ' + name)


def _data_fields(schema):
    required = schema.get('required', [])
    result = {'required': required, 'optional': [key for key in schema['properties'] if key not in required],
              'generic': schema.get('additionalProperties', False)}
    nested = {key: _data_fields(value) for key, value in schema['properties'].items() if 'properties' in value}
    if nested:
        result['nested'] = nested
    if 'oneOf' in schema:
        result['variants'] = schema['oneOf']
    return result


def type_summary():
    """Complete field inventory, without full schemas/examples or SQL."""
    return [{'type': item['type'], **_data_fields(item['dataSchema']), 'writer': item['writer']}
            for item in _catalog()['recordTypes']]


def _length(key, value):
    if key == 'prompt':
        return
    byte_units = key in ('reasoning', 'content')
    limit = {'reasoning': 512, 'content': 1024 * 1024, 'body': 16384, 'reply': 16384, 'path': 4096}.get(key, 256)
    used = len(value.encode('utf-8')) if byte_units else len(value.encode('utf-16-le')) // 2
    if used > limit:
        maximum = str(limit // (1024 * 1024)) + ' MiB' if limit % (1024 * 1024) == 0 else str(limit)
        raise ValueError('Invalid %s: %s %s exceeds %s' % (key, used, 'UTF-8 bytes' if byte_units else 'UTF-16 units', maximum))


def stored_lengths(value):
    if isinstance(value, dict):
        for key, item in value.items():
            if isinstance(item, str) and key in ('reasoning', 'content', 'body', 'reply', 'path'):
                _length(key, item)
            else:
                stored_lengths(item)
    elif isinstance(value, list):
        for item in value:
            stored_lengths(item)


def validate(schema, value):
    stored_lengths(value)
    validation.validate(schema, value)


def command(name, value):
    if name == 'record' and isinstance(value, dict) and value.get('type') in ('memory', 'event') and 'data' in value:
        validation.validate(record_type(value['type'])['dataSchema'], value['data'])
    if name == 'send_message' and isinstance(value, dict) and 'replyTo' in value:
        raise ValueError('Replies use complete {message:ID,reply:answer} only. For progress send a new FYI with to, replyRequired:false and conversationId; omit replyTo.')
    if name == 'send_message' and isinstance(value, dict) and (('to' in value) == ('topic' in value)):
        raise ValueError('Supply exactly one recipient: to or topic')
    if name == 'complete' and isinstance(value, dict) and 'reasoning' in value and 'reply' not in value:
        raise ValueError('Invalid complete: to handle without replying, omit reasoning and use message or messages only. Do not add a reply merely to satisfy validation; handled informational messages need no reply.')
    item = definition(name)
    stored_lengths({key: item for key, item in value.items() if key != 'data'} if name == 'record' and isinstance(value, dict) else value)
    if 'inputSchema' in item:
        validation.validate(item['inputSchema'], value)


def text(value, key):
    result = value.get(key)
    if not isinstance(result, str):
        raise ValueError('Missing string: ' + key)
    if not result.strip():
        raise ValueError('Invalid ' + key + ': blank')
    _length(key, result)
    return result


def ttl(value, default, maximum=86400000):
    result = value.get('ttlMs', default)
    if type(result) is not int or not 1000 <= result <= maximum:
        raise ValueError('Invalid TTL')
    return result


def selected_tools(selection=None):
    tools = _catalog()['tools']
    if selection is None:
        return copy.deepcopy(tools)
    presets = {'messaging': 'peers,set_status,send_message,notify_all,inbox,complete,fetch',
               'review': 'peers,set_status,send_message,notify_all,inbox,complete,fetch,record,share_document,read_document,context',
               'editing': 'peers,set_status,send_message,notify_all,inbox,complete,fetch,record,share_document,read_document,context,locks,lock,lock_many,renew,unlock'}
    names = presets.get(selection, selection).split(',')
    available = {tool['name'] for tool in tools}
    for name in names:
        if name not in available:
            raise ValueError('Unknown selected tool: ' + name + '; use schema to inspect tools')
    if len(names) != len(set(names)):
        raise ValueError('Duplicate selected tool')
    return copy.deepcopy([tool for tool in tools if tool['name'] in names])


def delivery_batch_limit():
    return definition('confirm_delivery')['inputSchema']['properties']['items']['maxItems']


def skill_instructions(vendor=None):
    value = SKILL.read_text(encoding='utf-8')
    return value if vendor is None else value[value.index('# '):]


def worker_skill(selection=None):
    value = skill_instructions('worker')
    names = {tool['name'] for tool in selected_tools(selection)}
    if 'lock' not in names and 'lock_many' not in names:
        start = value.index('## Edit with ownership')
        end = value.index('\n## ', start + 3)
        value = value[:start] + value[end + 1:]
        start = value.index('```mermaid')
        end = value.index('## Discover and choose work', start)
        value = value[:start] + value[end:]
    return value


def help():
    return {'package': '@octocodeai/octocode-agents-communication', 'implementation': 'Python',
        'usage': 'npx -y @octocodeai/octocode-agents-communication <command> [json|-] --workspace <path> [--database <file>] [--session <id>]',
        'commands': [item['name'] for item in _catalog()['commands']],
        'discover': ['skill', '<command> --help', 'schema types --compact', 'schema type <name>', 'db info'],
        'toolProfiles': {'messaging': 'messages/status', 'review': 'messaging + documents/context', 'editing': 'review + leases'}}
