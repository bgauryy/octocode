import fs from 'node:fs';
import path from 'node:path';
import { orderedEqual, scenario } from './advanced-local/helpers.mjs';

const file = 'graph-impact.rs';
const names = Array.from({ length: 125 }, (_, index) => `case_${String(index).padStart(3, '0')}`);
const expected = names.map(name => `${file}#tests.${name}`);

export function prepareGraphImpact(cwd) {
  const tests = names.map(name => `#[test] fn ${name}() { impact_target(); }`).join('\n');
  fs.writeFileSync(path.join(cwd, file), `pub fn impact_target() {}\n#[cfg(test)] mod tests { use super::*;\n${tests}\n}\n`);
}

export async function checkGraphImpact(ctx, graph, checks) {
  const observed = {};
  for (const surface of ctx.surfaces) {
    const result = await scenario(ctx, 'graphQuery', surface, {
      label: 'graph.impact.large-summary', checks, responseChars: 600,
      query: { operation: 'impact', graph, target: `${file}#impact_target`, limit: 1 },
    });
    const tests = result.data.flatMap(data => data.summary?.testFunctions ?? []);
    const rows = result.data.flatMap(data => data.rows ?? []);
    orderedEqual(checks, `graph.impact.test-functions-exact.${surface}`, tests, expected);
    orderedEqual(checks, `graph.impact.files-exact.${surface}`, rows.map(row => row.id), [file]);
    orderedEqual(checks, `graph.impact.tests-to-run-exact.${surface}`, result.data.flatMap(data => data.summary?.testsToRun ?? []), [file]);
    checks.push([`graph.impact.shared-response-pages.${surface}`, result.pages > 1
      && result.responses.some(entry => entry.result.json?.responsePagination?.hasMore === true)
      && result.data.every(data => Object.keys(data.next ?? {}).every(key => key === 'nextPage')),
    JSON.stringify({ pages: result.pages, tests: tests.length, rows: rows.length })]);
    observed[surface] = { tests, rows: rows.map(row => row.id) };
  }
  orderedEqual(checks, 'graph.impact.large-summary.parity', [observed.cli], [observed.mcp]);
}
