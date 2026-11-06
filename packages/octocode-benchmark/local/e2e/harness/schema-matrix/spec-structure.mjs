// structureSearch: listings judged against the known tree and fresh file metadata.
import fs from 'node:fs';
import path from 'node:path';
import { structureFiles } from '../responses.mjs';
import { isTestFile, visibleFiles } from './world-local.mjs';

const TEST_DIRS = new Set(['tests']);
const PERMISSION_FILES = ['open.txt', 'locked.txt', 'frozen.txt', 'tool.sh'];
const ACCESS = { executable: fs.constants.X_OK, readable: fs.constants.R_OK, writable: fs.constants.W_OK };
const dirsOf = file => { const parts = file.split('/').slice(0, -1); return parts.map((_, index) => parts.slice(0, index + 1).join('/')); };
const lineCount = text => { const lines = text.split('\n'); return lines.at(-1) === '' ? lines.length - 1 : lines.length; };

function rootOf(assignment) {
  return Object.hasOwn(assignment, 'access') ? 'perm' : assignment.root ?? 'plain';
}

/** Entries of the tree under a root, with the attributes a listing reports. */
function entries(ctx, root, query) {
  const base = ctx.world[root];
  const files = root === 'perm' ? PERMISSION_FILES : visibleFiles(root, query);
  const allowed = files.filter(file => {
    const mode = query.testCode ?? 'include';
    if (mode === 'exclude' && isTestFile(file)) return false;
    if (mode === 'only' && !isTestFile(file)) return false;
    return true;
  });
  const ancestors = [...new Set(allowed.flatMap(dirsOf))];
  // A test-only listing keeps test directories; the paths view still names any other parent as a group header.
  const dirs = (query.testCode ?? 'include') === 'only' && query.view !== 'paths' ? ancestors.filter(dir => dir.split('/').some(part => TEST_DIRS.has(part))) : ancestors;
  const rows = [
    ...allowed.map(file => {
      const stat = fs.statSync(path.join(base, file));
      return { path: file, type: 'file', size: stat.size, modifiedMs: stat.mtimeMs, full: path.join(base, file), depth: file.split('/').length };
    }),
    ...dirs.map(dir => ({ path: dir, type: 'directory', modifiedMs: fs.statSync(path.join(base, dir)).mtimeMs, full: path.join(base, dir), depth: dir.split('/').length })),
  ];
  return rows;
}

/** Emptiness is content bytes for files and physical entries for directories. */
export function isEmptyEntry(row) {
  return row.type === 'file' ? row.size === 0 : row.type === 'directory' && fs.readdirSync(row.full).length === 0;
}

function selected(ctx, testCase) {
  const { query, assignment } = testCase;
  const root = rootOf(assignment);
  let rows = entries(ctx, root, query);
  const operation = query.operation ?? 'tree';
  if (operation === 'tree') rows = rows.filter(row => row.depth <= query.maxDepth);
  if (query.entryType === 'file') rows = rows.filter(row => row.type === 'file');
  if (query.entryType === 'directory') rows = rows.filter(row => row.type === 'directory');
  if (query.access) rows = rows.filter(row => { try { fs.accessSync(row.full, ACCESS[query.access]); return true; } catch { return false; } });
  if (query.empty !== undefined) rows = rows.filter(row => isEmptyEntry(row) === query.empty);
  return { root, rows };
}

function truth(ctx, testCase) {
  const { rows } = selected(ctx, testCase);
  return rows.map(row => [row.path, row.type]);
}

const rel = (ctx, root, value) => path.relative(fs.realpathSync(ctx.world[root]), path.resolve(fs.realpathSync(ctx.cwd), value)).split(path.sep).join('/');
const rowsOf = pages => pages.flatMap(page => page.json?.results ?? []).flatMap(row => structureFiles(row.data?.files ?? []));
/** The paths view names a directory once, as the header of its group, instead of a row of its own. */
const headers = pages => pages.flatMap(page => page.json?.results ?? []).flatMap(row => (row.data?.files ?? []).map(group => group.dir));

function observed(ctx, testCase, pages) {
  const root = rootOf(testCase.assignment);
  const rows = rowsOf(pages).map(row => [rel(ctx, root, row.path), row.type === 'directory' ? 'directory' : 'file']);
  if (testCase.query.view !== 'paths' || testCase.query.entryType === 'file') return rows;
  const named = new Set(rows.map(([file]) => file));
  const implied = new Set(headers(pages).map(dir => rel(ctx, root, dir)).filter(dir => dir && !named.has(dir)));
  return [...rows, ...[...implied].map(dir => [dir, 'directory'])];
}

const KEY = {
  path: row => row.path,
  name: row => path.basename(row.path),
  modifiedAt: (row, stat) => stat.mtimeMs,
  sizeBytes: (row, stat) => (row.type === 'directory' ? 0 : stat.size),
  lineCount: (row, stat, full) => (row.type === 'directory' ? 0 : lineCount(fs.readFileSync(full, 'utf8'))),
};

/** A listed row's attributes against the file on disk. */
function attributeProblem(ctx, root, row, query) {
  const full = path.join(ctx.world[root], row.path);
  const stat = fs.statSync(full);
  if (row.size !== undefined && row.type !== 'directory' && row.size !== stat.size) return `${row.path} size ${row.size}, on disk ${stat.size}`;
  if (row.modifiedMs !== undefined && Math.abs(row.modifiedMs - stat.mtimeMs) > 1) return `${row.path} modified ${row.modifiedMs}, on disk ${stat.mtimeMs}`;
  if (row.totalLines !== undefined && root !== 'perm' && row.totalLines !== lineCount(fs.readFileSync(full, 'utf8'))) return `${row.path} totalLines ${row.totalLines}`;
  const details = (query.view ?? 'details') === 'details';
  return details === (row.modifiedMs !== undefined) ? null : `${row.path}: view ${query.view ?? 'details'} ${details ? 'lacks' : 'carries'} metadata`;
}

function orderProblem(ctx, root, listed, sort) {
  if (!sort || !KEY[sort.by]) return null;
  const files = listed.filter(row => row.type !== 'directory');
  const value = row => { const full = path.join(ctx.world[root], row.path); return KEY[sort.by](row, fs.statSync(full), full); };
  for (let index = 1; index < files.length; index += 1) {
    const [a, b] = [value(files[index - 1]), value(files[index])];
    if (sort.order === 'asc' ? a > b : a < b) return `${sort.by} ${sort.order}: ${files[index - 1].path} before ${files[index].path}`;
  }
  return null;
}

/** Sort keys and attributes, read from the tree independently of the listing. */
function check(ctx, testCase, pages) {
  const root = rootOf(testCase.assignment);
  const listed = rowsOf(pages).map(row => ({ ...row, path: rel(ctx, root, row.path) }));
  for (const row of listed) {
    const problem = attributeProblem(ctx, root, row, testCase.query);
    if (problem) return problem;
  }
  return orderProblem(ctx, root, listed, testCase.query.sort);
}

export const structureSearch = {
  tool: 'structureSearch',
  pseudo: [{ key: 'root', values: ['plain', 'git'] }, { key: 'page', values: ['whole', 'tiny'] }],
  base: (row, assignment) => ({
    path: `matrix-world/${rootOf({ ...assignment, ...row })}`,
    ...((row.operation ?? 'tree') === 'tree' && { maxDepth: 2 }),
    ...(assignment.page === 'tiny' && { limit: 1 }),
  }),
  normalize: testCase => {
    testCase.assignment.root ??= 'plain';
    if (testCase.assignment.page === 'tiny') testCase.responseChars = 300;
  },
  truth,
  observed,
  check,
  indistinct: {
    sort: 'order, judged against file metadata; the selected set must not change',
    view: 'selects the same entries; `check` judges the metadata each view carries',
    operation: 'one value per form; the form itself owns depth and filters',
  },

};
