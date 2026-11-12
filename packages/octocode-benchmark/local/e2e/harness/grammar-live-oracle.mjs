import fs from 'node:fs';
import path from 'node:path';
import { errorPage, semanticPagesComplete } from './grammar-live-readiness.mjs';

const rowData = pages =>
  pages.flatMap(page => page.json?.results ?? []).map(row => row.data ?? {});
// The fix a reader needs, not the sentence around it.
const indexWarning = warning => /compile_commands\.json/u.test(warning);

/** Compare source-derived positions and file identity, retaining duplicates as failures. */
export function semanticLocationEvidence(locations, ranges, file, base) {
  const observed = locations
    .map(row => [
      path.resolve(base, row.path),
      row.line,
      row.column,
    ])
    .toSorted((a, b) => a[0].localeCompare(b[0]) || a[1] - b[1] || a[2] - b[2]);
  const expected = ranges
    .map(row => [file, row.start.line + 1, row.start.character + 1])
    .toSorted((a, b) => a[1] - b[1] || a[2] - b[2]);
  return {
    ok: JSON.stringify(observed) === JSON.stringify(expected),
    expected,
    observed,
    locations,
  };
}

export function selectedGrammarCases(capabilities, selected) {
  const cases = capabilities.flatMap(capability =>
    capability.extensions.map(ext => ({ capability, ext }))
  );
  for (const selection of selected)
    if (
      !cases.some(
        row => row.ext === selection || row.capability.language === selection
      )
    ) {
      throw new Error(`Unknown grammar selection: ${selection}`);
    }
  return cases.filter(
    row =>
      !selected.size ||
      selected.has(row.ext) ||
      selected.has(row.capability.language)
  );
}

const coverageProblems = (row, state) => {
  const { coverage } = row;
  return [
    ...(coverage?.state === state ? [] : [`coverage.state ${coverage?.state}`]),
    ...(coverage?.textOnlyFiles === 0
      ? []
      : [`coverage.textOnlyFiles ${coverage?.textOnlyFiles}`]),
  ];
};
const compilationOf = row => row.lsp?.analysisContext?.compilation;
const warnsOfIndex = row => !!row.warnings?.some(indexWarning);

function unindexedRowProblems(row, { reason, configuration }) {
  const problems = coverageProblems(row, 'partial');
  const reasons = JSON.stringify(row.partialReasons);
  if (row.isPartial !== true) problems.push('isPartial is not true');
  if (reasons !== JSON.stringify([reason]))
    problems.push(`partialReasons ${reasons}, expected ["${reason}"]`);
  if (compilationOf(row)?.configuration !== configuration)
    problems.push(`compilation ${JSON.stringify(compilationOf(row))}`);
  if (!warnsOfIndex(row)) problems.push('the clangd index warning is missing');
  return problems;
}

/**
 * A C-family `references` row with no finished index: text chose the files, so it must say
 * it is partial on every page, why, and how to fix it. `reason` follows the configuration
 * clangd reads. Returns what is wrong; empty means the row is honest.
 */
export function unindexedReferencesProblems(pages, expected) {
  const rows = rowData(pages);
  return [
    ...(rows.length ? [] : ['no rows']),
    ...(pages.some(errorPage) ? ['an error page'] : []),
    ...rows.flatMap(row => unindexedRowProblems(row, expected)),
  ];
}

function indexedRowProblems(row, database) {
  const problems = coverageProblems(row, 'complete');
  const compilation = compilationOf(row);
  if (
    compilation?.configuration !== 'found' ||
    !compilation.sources?.includes(database)
  )
    problems.push(
      `compilation ${JSON.stringify(compilation)} does not name ${database}`
    );
  if (warnsOfIndex(row))
    problems.push(
      'the row still warns that the index does not cover the project'
    );
  return problems;
}

/** A C-family `references` row answered from a finished index of the project's database. */
export function indexedReferencesProblems(pages, { database }) {
  return [
    ...(semanticPagesComplete(pages) ? [] : ['a page is an error or partial']),
    ...rowData(pages).flatMap(row => indexedRowProblems(row, database)),
  ];
}

/**
 * What clangd did on disk: read the mirror under the Octocode home with a background index,
 * wrote a shard for this file there, and left the project alone.
 */
export function finishedIndexProblems({ status, home, cwd, file }) {
  const args = status.server?.args ?? [];
  const mirror = args
    .find(arg => arg.startsWith('--compile-commands-dir='))
    ?.split('=')[1];
  if (
    !args.includes('--background-index') ||
    !mirror?.startsWith(
      `${path.join(home, 'lsp-workspaces/clangd')}${path.sep}`
    )
  )
    return [
      `clangd was not started on a mirror with a background index: ${JSON.stringify(args)}`,
    ];
  const index = path.join(mirror, '.cache/clangd/index');
  const shards = fs.existsSync(index) ? fs.readdirSync(index) : [];
  const problems = shards.some(
    shard =>
      shard.startsWith(`${path.basename(file)}.`) && shard.endsWith('.idx')
  )
    ? []
    : [
        `no index shard for ${path.basename(file)} in ${index}: ${JSON.stringify(shards)}`,
      ];
  if (fs.existsSync(path.join(cwd, '.cache')))
    problems.push('clangd wrote its index into the project');
  return problems;
}
