import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { buildToolFlow, PRESENTATION, ROUTE_HEADING, routeTable, TOOL_FLOW, withRouteTable } from '../tool-flow.mjs';

const routes = [
  { needs: ['localFetch'], text: '- known file → localFetch\n' },
  { needs: [], text: '- command output, git state → shell (§5)\n' },
  { needs: ['a', 'b'], text: '- x | y → a → b\n' },
];

test('renders one row per served route with the tools it needs listed', () => {
  assert.deepEqual(routeTable(routes), [
    '| Route (server instructions §1) | Served when listed |',
    '|---|---|',
    '| known file → localFetch | `localFetch` |',
    '| command output, git state → shell (§5) | always |',
    '| x \\| y → a → b | `a`, `b` |',
  ]);
});

test('replaces only the first table under the route heading, idempotently', () => {
  const guide = ['intro', '', ROUTE_HEADING, '', 'Prose.', '', '| old | table |', '|---|---|', '| a | b |', '', 'After.', '', '## Shape', '', '| keep | me |'].join('\n');
  const once = withRouteTable(guide, routes);
  assert.equal(withRouteTable(once, routes), once);
  assert.match(once, /Prose\.\n\n\| Route \(server instructions §1\)/);
  assert.match(once, /\| x \\\| y → a → b \| `a`, `b` \|\n\nAfter\./);
  assert.match(once, /## Shape\n\n\| keep \| me \|$/);
  assert.throws(() => withRouteTable('no heading', routes), /no "## Route/);
  assert.throws(() => withRouteTable(`${ROUTE_HEADING}\n\n## Next\n\n| t |`, routes), /No table/);
});

test('the checked-in guide lists exactly the routes core serves', () => {
  const served = JSON.parse(readFileSync(PRESENTATION, 'utf8')).instructions.routes;
  const guide = readFileSync(TOOL_FLOW, 'utf8');
  for (const row of routeTable(served).slice(2)) assert.ok(guide.includes(`${row}\n`), row);
  assert.equal(buildToolFlow({ check: true }).stale, false);
});
