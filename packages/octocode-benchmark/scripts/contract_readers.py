"""Reader-tool sets the benchmark preflight expects, derived from the generated tool contract."""
import json
from pathlib import Path

CONTRACT = Path(__file__).resolve().parents[2] / 'octocode-config' / 'contract' / 'tool-contract.json'


def has_classification(value):
    if isinstance(value, dict):
        return 'classification' in value.get('properties', {}) or any(has_classification(v) for v in value.values())
    return isinstance(value, list) and any(has_classification(v) for v in value)


def contract_readers():
    """(readers, local_readers, classifier_readers): MCP read tools other than clasify,
    the local-family subset, and the readers that take classification."""
    tools = [t for t in json.loads(CONTRACT.read_text())['tools'] if not t['cliOnly'] and not t['beta'] and t['name'] != 'clasify']
    readers = {t['name'] for t in tools}
    local = {t['name'] for t in tools if t['family'] == 'local'}
    classifier_readers = {t['name'] for t in tools if has_classification(t['inputSchema'])}
    return readers, local, classifier_readers
