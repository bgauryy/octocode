"""Validator for the finite JSON Schema vocabulary used by the command catalog.

No schema is authored here. Unsupported validation keywords fail closed during
catalog loading, so a contract extension cannot silently skip a new constraint.
Error messages contain locations and rules, never rejected payloads.
"""
import math
import re

KEYWORDS = frozenset(('type', 'properties', 'required', 'additionalProperties',
    'enum', 'const', 'minimum', 'maximum', 'exclusiveMinimum', 'exclusiveMaximum',
    'minLength', 'maxLength', 'pattern', 'minItems', 'maxItems', 'uniqueItems',
    'items', 'minProperties', 'maxProperties', 'oneOf', 'anyOf', 'allOf', 'not',
    'if', 'then', 'else', 'dependentRequired', 'description', 'default', 'format',
    '$schema', 'title'))


def check_schema(schema):
    if isinstance(schema, bool):
        return
    if not isinstance(schema, dict):
        raise ValueError('Invalid catalog schema')
    unknown = set(schema) - KEYWORDS
    if unknown:
        raise ValueError('Unsupported catalog schema keyword: ' + ', '.join(sorted(unknown)))
    for sub in schema.get('properties', {}).values():
        check_schema(sub)
    for key in ('items', 'additionalProperties', 'not', 'if', 'then', 'else'):
        if key in schema:
            check_schema(schema[key])
    for key in ('oneOf', 'anyOf', 'allOf'):
        for sub in schema.get(key, []):
            check_schema(sub)


def _equal(a, b):
    if isinstance(a, bool) != isinstance(b, bool):
        return False
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(_equal(x, y) for x, y in zip(a, b))
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(_equal(a[k], b[k]) for k in a)
    return a == b


def validate(schema, value, location=''):
    def fail(rule):
        raise ValueError(('Invalid input: ' + (location or 'input') + ' ' + rule)[:200])

    if schema is True:
        return
    if schema is False:
        fail('is not allowed')
    types = {'object': lambda v: isinstance(v, dict), 'array': lambda v: isinstance(v, list),
             'string': lambda v: isinstance(v, str), 'boolean': lambda v: isinstance(v, bool),
             'null': lambda v: v is None,
             'number': lambda v: type(v) in (int, float) and math.isfinite(v),
             # Strict: whole floats such as 5.0 would reach int-only store code.
             'integer': lambda v: type(v) is int}
    kind = schema.get('type')
    if kind and not any(types[k](value) for k in (kind if isinstance(kind, list) else [kind])):
        fail('has invalid type')
    if 'enum' in schema and not any(_equal(value, item) for item in schema['enum']):
        fail('is not an allowed value')
    if 'const' in schema and not _equal(value, schema['const']):
        fail('does not match required value')
    if isinstance(value, dict):
        missing = [key for key in schema.get('required', []) if key not in value]
        if missing:
            fail('is missing required property ' + missing[0])
        if len(value) < schema.get('minProperties', 0) or len(value) > schema.get('maxProperties', math.inf):
            fail('has invalid property count')
        props = schema.get('properties', {})
        for key, item in value.items():
            child = location + '/' + key.replace('~', '~0').replace('/', '~1')
            if key in props:
                validate(props[key], item, child)
            elif schema.get('additionalProperties') is False:
                fail('has additional properties')
            elif isinstance(schema.get('additionalProperties'), dict):
                validate(schema['additionalProperties'], item, child)
        for key, needed in schema.get('dependentRequired', {}).items():
            if key in value and any(k not in value for k in needed):
                fail('is missing a dependent property')
    if isinstance(value, list):
        if len(value) < schema.get('minItems', 0) or len(value) > schema.get('maxItems', math.inf):
            fail('has invalid item count')
        if schema.get('uniqueItems') and any(_equal(v, previous) for i, v in enumerate(value) for previous in value[:i]):
            fail('must contain unique items')
        if 'items' in schema:
            for i, item in enumerate(value):
                validate(schema['items'], item, location + '/' + str(i))
    if isinstance(value, str):
        if len(value) < schema.get('minLength', 0) or len(value) > schema.get('maxLength', math.inf):
            fail('has invalid string length')
        if 'pattern' in schema and re.search(schema['pattern'], value) is None:
            fail('does not match pattern')
    if type(value) in (int, float):
        if value < schema.get('minimum', -math.inf) or value > schema.get('maximum', math.inf):
            fail('is outside numeric bounds')
        if value <= schema.get('exclusiveMinimum', -math.inf) or value >= schema.get('exclusiveMaximum', math.inf):
            fail('is outside exclusive numeric bounds')

    def matches(sub):
        try:
            validate(sub, value, location)
            return True
        except ValueError:
            return False

    for key, rule in (('allOf', all), ('anyOf', any)):
        if key in schema and not rule(matches(sub) for sub in schema[key]):
            fail('does not satisfy ' + key)
    if 'oneOf' in schema and sum(matches(sub) for sub in schema['oneOf']) != 1:
        fail('must match exactly one alternative')
    if 'not' in schema and matches(schema['not']):
        fail('matches a forbidden alternative')
    if 'if' in schema:
        branch = 'then' if matches(schema['if']) else 'else'
        if branch in schema:
            validate(schema[branch], value, location)
