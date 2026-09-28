"""One catalog supplies validation, command help, MCP and host tool discovery."""
import copy
import json
from functools import lru_cache
from pathlib import Path
from . import validation

SCRIPTS = Path(__file__).resolve().parent.parent
SKILL = SCRIPTS.parent / 'SKILL.md'


def _resolve(value, definitions):
    if isinstance(value, dict):
        if '$ref' in value:
            ref = value['$ref']
            if not ref.startswith('#/$defs/') or set(value) - {'$ref', 'description'}:
                raise ValueError('Unsupported catalog reference')
            result = copy.deepcopy(definitions[ref[len('#/$defs/'):]])
            if 'description' in value:
                result['description'] = value['description']
            return result
        return {key: _resolve(item, definitions) for key, item in value.items()}
    if isinstance(value, list):
        return [_resolve(item, definitions) for item in value]
    return value


@lru_cache(maxsize=1)
def _catalog():
    raw = json.loads((SCRIPTS / 'catalog.json').read_text(encoding='utf-8'))
    value = _resolve(raw, raw.pop('$defs', {}))
    definitions = {item['name']: item for item in value['commands']}
    for action in ('get', 'list', 'set'):
        definitions['entity ' + action]['inputSchema']['properties']['entity']['enum'] = [
            item['name'] for item in value['entities'] if action != 'set' or item.get('set') is not None]
    for item in value['commands']:
        if 'inputSchema' in item:
            validation.check_schema(item['inputSchema'])
    for tool in value['tools']:
        source = definitions[tool['name']]
        tool.update(description=source['description'], inputSchema=copy.deepcopy(source['inputSchema']))
    for item in value['entities']:
        if item['agentIdField'] not in item['fields']:
            raise ValueError('Entity references undeclared agent ID field')
        for action in ('list', 'set'):
            if item.get(action) is not None:
                validation.check_schema(item[action])
    from . import database
    value['database'] = copy.deepcopy(database.schema())
    value['database']['sql'] = database.SQL
    entities = {item['table']: item['name'] for item in value['entities']}
    for relationship in value['database']['relationships']:
        target = relationship['references']
        if target['table'] in entities:
            target['entity'] = entities[target['table']]
    for item in value['entities']:
        item['relationships'] = [relation for relation in value['database']['relationships'] if relation['table'] == item['table']]
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


def entity(name):
    for item in _catalog()['entities']:
        if item['name'] == name:
            return copy.deepcopy(item)
    raise ValueError('Unknown entity: ' + name)


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
    if name == 'send_message' and isinstance(value, dict) and 'replyTo' in value:
        raise ValueError('Replies use complete {message:ID,reply:answer} only. For progress send a new FYI with to, replyRequired:false and conversationId; omit replyTo.')
    if name == 'complete' and isinstance(value, dict) and 'reasoning' in value and 'reply' not in value:
        raise ValueError('Invalid complete: to handle without replying, omit reasoning and use message or messages only. Do not add a reply merely to satisfy validation; handled informational messages need no reply.')
    item = definition(name)
    stored_lengths(value)
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
    presets = {'messaging': 'peers,set_status,send_message,inbox,complete',
               'review': 'peers,set_status,send_message,inbox,complete,share_document,read_document,context',
               'editing': 'peers,set_status,send_message,inbox,complete,share_document,read_document,context,locks,lock,lock_many,renew,unlock'}
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


def worker_skill():
    return skill_instructions('worker')


def help():
    return {'package': '@octocodeai/octocode-agents-communication', 'implementation': 'Python',
        'usage': 'scripts/agents-communication <command> [json|-] --workspace <path> [--database <file>] [--session <id>]',
        'commands': [item['name'] for item in _catalog()['commands']],
        'discover': ['skill', '<command> --help', 'schema entity <name>', 'db info'],
        'toolProfiles': {'messaging': 'messages/status', 'review': 'messaging + documents/context', 'editing': 'review + leases'}}
