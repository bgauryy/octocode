// A malformed `where` says what shape and operators it expects, in a bounded sentence.
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  matches,
  parseWhere,
  validatePredicates,
} from '../../dist/engine/evidence-predicates.mjs';

test('a bad where array shows the expected shape and the operators', () => {
  for (const bad of [
    [{ path: '/a', op: 'like', value: 1 }],
    [{ path: '/a' }],
    {},
  ]) {
    assert.throws(
      () => validatePredicates(bad),
      error => {
        assert.match(error.message, /^Invalid --where: /);
        assert.match(error.message, /\[\{"path":.*"op":.*"value":.*\}\]/);
        for (const op of ['eq', 'ne', 'contains', 'gte', 'lte', 'exists'])
          assert(error.message.includes(op), op);
        assert(error.message.split(/\s+/).length <= 40, error.message);
        return true;
      }
    );
  }
  assert.throws(() => parseWhere('[{'), /Invalid --where: .*Example/);
});

test('exists defaults value to true, value false means absent, other ops still need value', () => {
  const rows = [{ a: 1 }, { b: 2 }];
  const pick = filter => {
    const filters = [filter];
    validatePredicates(filters);
    return rows.filter(row => matches(row, filters)).length;
  };
  assert.equal(pick({ path: '/a', op: 'exists' }), 1);
  assert.equal(pick({ path: '/a', op: 'exists', value: true }), 1);
  assert.equal(pick({ path: '/a', op: 'exists', value: false }), 1);
  assert.deepEqual(
    rows.filter(row =>
      matches(row, [{ path: '/a', op: 'exists', value: false }])
    ),
    [{ b: 2 }]
  );
  for (const op of ['eq', 'ne', 'contains', 'gte', 'lte'])
    assert.throws(
      () => validatePredicates([{ path: '/a', op }]),
      /Invalid --where/,
      op
    );
  assert.throws(
    () => validatePredicates([{ path: '/a', op: 'exists', value: 'yes' }]),
    /boolean/
  );
});

test('ne is an advertised op: it keeps rows that differ', () => {
  const filters = [{ path: '/s', op: 'ne', value: 200 }];
  validatePredicates(filters);
  assert.deepEqual(
    [{ s: 200 }, { s: 404 }, {}].filter(row => matches(row, filters)),
    [{ s: 404 }, {}]
  );
});

test('a simple condition splits at the first operator after its path; a line break fails it', () => {
  const plain = text => JSON.parse(JSON.stringify(parseWhere(text)));
  const one = (path, op, value) => [{ path, op, value }];
  for (const [text, expected] of [
    ['response.status=0', one('/response/status', 'eq', 0)],
    ['a!=b', one('/a', 'ne', 'b')],
    ['a>=10', one('/a', 'gte', 10)],
    ['a<=10', one('/a', 'lte', 10)],
    ['request.url~login', one('/request/url', 'contains', 'login')],
    ['a=b=c', one('/a', 'eq', 'b=c')],
    ['a~b=c', one('/a', 'contains', 'b=c')],
    ['a!b=c', one('/a!b', 'eq', 'c')],
    ['a==1', one('/a', 'eq', '=1')],
    ['a<=>b', one('/a', 'lte', '>b')],
    ['!=x', one('/!', 'eq', 'x')],
    ['a=', one('/a', 'eq', '')],
    ['x~', one('/x', 'contains', '')],
    [' a = 1 \n', one('/a', 'eq', 1)],
    [
      'a.b=1&&c~x',
      [
        { path: '/a/b', op: 'eq', value: 1 },
        { path: '/c', op: 'contains', value: 'x' },
      ],
    ],
  ])
    assert.deepEqual(plain(text), expected, text);
  const [lineSeparator, paragraphSeparator] = [0x2028, 0x2029].map(code =>
    String.fromCharCode(code)
  );
  for (const text of [
    'a>b',
    '=x',
    '~',
    'a=x\ny',
    'body~a\rb',
    `a=x${lineSeparator}y`,
    `a${paragraphSeparator}=1`,
  ])
    assert.throws(
      () => parseWhere(text),
      /Cannot read the condition/,
      JSON.stringify(text)
    );
});
