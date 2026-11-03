"""Reader-tool sets the benchmark preflight expects, derived from the generated tool contract."""
import json
from pathlib import Path

CONTRACT = Path(__file__).resolve().parents[2] / 'octocode-config' / 'contract' / 'tool-contract.json'


def has_classification(value, root=None):
    """Inspect reachable schema positions, resolving references in their document."""
    root = value if root is None else root
    seen = set()

    def visit(schema):
        if not isinstance(schema, dict) or id(schema) in seen:
            return False
        seen.add(id(schema))
        if 'classification' in schema.get('properties', {}):
            return True
        reference = schema.get('$ref', '')
        if reference.startswith('#/'):
            target = root
            for part in reference[2:].split('/'):
                target = target.get(part.replace('~1', '/').replace('~0', '~')) if isinstance(target, dict) else None
            if visit(target):
                return True
        maps = ('properties', 'patternProperties', 'dependentSchemas')
        lists = ('allOf', 'anyOf', 'oneOf', 'prefixItems')
        singles = ('items', 'additionalProperties', 'contains', 'not', 'if', 'then', 'else')
        return (any(visit(child) for key in maps for child in schema.get(key, {}).values())
                or any(visit(child) for key in lists for child in schema.get(key, []))
                or any(visit(schema.get(key)) for key in singles))

    return visit(value)


def contract_readers():
    """(readers, local_readers, classifier_readers): MCP read tools other than clasify,
    the local-family subset, and the readers that take classification."""
    contract = json.loads(CONTRACT.read_text())
    tools = [t for t in contract['tools'] if not t['beta'] and t['name'] != 'clasify']
    readers = {t['name'] for t in tools}
    local = {t['name'] for t in tools if t['family'] == 'local'}
    classifier_readers = {t['name'] for t in tools if has_classification(t['inputSchema'], contract)}
    return readers, local, classifier_readers
