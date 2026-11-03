import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { contract } from '../contract.mjs';
import { structureFiles } from '../responses.mjs';
import { writeResults } from '../mcp-client.mjs';

/** Check all pages, including failures after a successful first page. */
export function inspectStructureWindow(responses, expected, cwd = process.cwd()) {
  const envelopes = responses.map(response => response.result.json);
  const rows = envelopes.flatMap(envelope => envelope?.results ?? []);
  const data = rows.map(row => row.data).filter(Boolean);
  const paths = data.flatMap(row => structureFiles(row.files)).map(file => path.resolve(cwd, file.path));
  const actual = new Set(paths);
  const wanted = new Set(expected);
  const errors = envelopes.filter(envelope => !envelope || envelope.error || envelope.errorCode).length
    + rows.filter(row => row.status === 'error' || row.data?.error || row.data?.errorCode).length;
  const windowLeads = data.filter(row => row.partialReasons?.includes('walkWindow') && row.next?.nextPage).length;
  const wrongKeys = data.flatMap(row => Object.keys(row.next ?? {}).filter(key => key !== 'nextPage'));
  const missing = expected.filter(file => !actual.has(file));
  const extra = paths.filter(file => !wanted.has(file));
  return { errors, windowLeads, wrongKeys, missing, extra, duplicates: paths.length - actual.size, actual: paths.length, expected: expected.length };
}

/** Cross the runtime's internal scan window, independently of public row/response pages. */
export async function advancedStructureWindow(ctx) {
  const walkWindow = contract.tools.find(tool => tool.name === 'structureSearch').semanticPolicy.walkWindow;
  assert.ok(Number.isSafeInteger(walkWindow) && walkWindow > 0, 'contract must declare the structure walk window');
  const directory = fs.mkdtempSync(path.join(ctx.cwd, 'structure-window-'));
  const names = Array.from({ length: walkWindow + 1 }, (_, index) => `entry-${String(index).padStart(String(walkWindow).length, '0')}-λ.txt`);
  const checks = [];
  const observations = [];
  try {
    for (const name of names) fs.writeFileSync(path.join(directory, name), '');
    const expected = names.map(name => path.join(fs.realpathSync(directory), name));
    for (const surface of ctx.surfaces) {
      const report = await ctx.explore('structureSearch', {
        queries: [{ path: directory, operation: 'files', view: 'paths', entryType: 'file', limit: walkWindow }],
        responseChars: contract.limits.defaultResponseChars,
      }, surface);
      const observed = inspectStructureWindow(report.responses, expected, fs.realpathSync(ctx.cwd));
      const errorRows = report.responses.flatMap(response => response.result.json?.results ?? [])
        .filter(row => row.data?.errorCode).map(row => ({ code: row.data.errorCode, error: row.data.error }));
      observations.push({ surface, ...observed, errorRows, failures: report.failures });
      const detail = JSON.stringify({ actual: observed.actual, expected: observed.expected, duplicates: observed.duplicates,
        missing: observed.missing.length, extra: observed.extra.length, windowLeads: observed.windowLeads, wrongKeys: observed.wrongKeys, failures: report.failures });
      checks.push([`structure.scan-window.${surface}.accepted`, report.failures.length === 0 && observed.errors === 0, detail]);
      checks.push([`structure.scan-window.${surface}.canonical`, observed.windowLeads > 0 && observed.wrongKeys.length === 0, detail]);
      checks.push([`structure.scan-window.${surface}.exactOnce`, observed.actual === expected.length && observed.duplicates === 0
        && observed.missing.length === 0 && observed.extra.length === 0, detail]);
    }
  } finally {
    writeResults('structure-window', { walkWindow, observations });
    fs.rmSync(directory, { recursive: true, force: true });
  }
  return checks;
}
