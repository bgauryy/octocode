import fs from 'node:fs';
import path from 'node:path';
import { statusClass } from '../driver.mjs';
import { canon } from '../calls.mjs';

export const fixturePath = suffix => `advanced-local/${suffix}`;
export const sourceText = (ctx, suffix) => fs.readFileSync(path.join(ctx.cwd, fixturePath(suffix)), 'utf8');
export const sourcePairs = text => text.split('\n').flatMap((line, index, lines) => index === lines.length - 1 && line === '' ? [] : [[index + 1, line]]);
const compare = (a, b) => canon(a).localeCompare(canon(b));

/** Compare source-derived values, preserving multiplicity so duplicate pages fail. */
export function equal(checks, label, actual, expected) {
  orderedEqual(checks, label, actual.toSorted(compare), expected.toSorted(compare));
}

export function orderedEqual(checks, label, actual, expected) {
  checks.push([label, canon(actual) === canon(expected), JSON.stringify({ expected, actual })]);
}

/** Reuse the shared schema-driven continuation walker and actual process transport. */
export async function scenario(ctx, tool, surface, { label, query, checks, responseChars = 20000, maxPages }) {
  const report = await ctx.explore(tool, { queries: [query], responseChars }, surface, maxPages === undefined ? {} : { maxPages });
  const status = statusClass(report.root.json);
  const errors = [report.root.json, ...(report.root.json?.results ?? []).map(row => row.data)]
    .filter(data => data?.errorCode || data?.error)
    .map(data => ({ errorCode: data.errorCode, error: data.error }));
  checks.push([`${label}.${surface}.complete`, report.failures.length === 0 && ['ok', 'empty', 'outOfRange'].includes(status), JSON.stringify({ errors, status, failures: report.failures })]);
  const responses = report.responses.filter(entry => entry.tool === tool);
  return {
    data: responses.flatMap(entry => entry.result.json?.results ?? []).map(row => row.data),
    responses,
    root: report.root.json,
    pages: responses.length,
  };
}

export const files = data => data.flatMap(row => row.files ?? []);
export const matches = data => files(data).flatMap(file => (file.matches ?? []).map(match => ({ path: file.path, ...match })));
export const relative = (ctx, value) => path.relative(fs.realpathSync(path.join(ctx.cwd, fixturePath(''))), fs.realpathSync(path.resolve(ctx.cwd, value))).split(path.sep).join('/');
