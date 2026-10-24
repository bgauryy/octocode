// The tool-health mock's in-memory world: active and archived repositories (octo/demo's files
// are the tool-health fixture), its commits, pull requests, issues and npm
// packages, plus the lookups every mock endpoint shares.
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

export const sha = text => createHash('sha1').update(text).digest('hex');
export const blobSha = text => sha(`blob ${Buffer.byteLength(text)}\0${text}`);
export const DATE = i => new Date(Date.UTC(2025, 0, 1 + i, 12)).toISOString().replace('.000', '');
export const USER = { login: 'octo-dev', id: 1, type: 'User', html_url: 'https://github.com/octo-dev' };

/** The lines a file adds: its lines without the empty one after a final newline. */
export function addedLines(text) {
  const lines = text.split('\n');
  return lines.at(-1) === '' ? lines.slice(0, -1) : lines;
}

/** Every file under `dir` (relative path → UTF-8 text), sorted. */
function readTree(dir) {
  const out = {};
  const walk = rel => {
    for (const entry of fs.readdirSync(path.join(dir, rel), { withFileTypes: true }).toSorted((a, b) => a.name.localeCompare(b.name))) {
      const child = rel ? `${rel}/${entry.name}` : entry.name;
      if (entry.isDirectory()) walk(child);
      else out[child] = fs.readFileSync(path.join(dir, child), 'utf8');
    }
  };
  walk('');
  return out;
}

/** A file larger than one default read window, so reads continue. */
const LARGE_GUIDE = Array.from({ length: 1500 }, (_, i) => `Line ${i + 1}: the greeter guide repeats this sentence to fill a long document.`).join('\n') + '\n';

/** The model: repositories, commits, pull requests, issues, npm packages. */
export function buildModel(fixtureDir) {
  const files = readTree(fixtureDir);
  const paths = Object.keys(files);
  // Three commits on `main`: each adds a slice of the files, newest first in listings.
  const commits = [];
  const slices = [paths.slice(0, 2), paths.slice(2, 4), paths.slice(4)];
  slices.forEach((added, i) => {
    const parent = commits.at(-1);
    const message = `Add ${added.join(', ')}` + (i === 2 ? '\n\nPreserve multiline commit rationale as separate evidence.' : '');
    commits.push({ oid: sha(`commit ${i} ${message}`), message, date: DATE(i), parent: parent?.oid, added, tree: sha(`tree ${i}`) });
  });
  const head = commits.at(-1);
  const repos = [
    { owner: 'octo', name: 'demo', languages: { TypeScript: 1200, Markdown: 300 }, description: 'Tool-health demo greeter in TypeScript', language: 'TypeScript', stars: 42, files },
    { owner: 'octo', name: 'demo-cli', description: 'Command line wrapper for the demo greeter', language: 'TypeScript', stars: 7, files: { 'README.md': '# demo-cli\n\nA greeter CLI.\n' } },
    { owner: 'octo', name: 'demo-docs', description: 'Documentation site for the demo greeter', language: 'Markdown', stars: 3, files: { 'README.md': '# demo-docs\n\nGreeter docs.\n', 'guide.txt': LARGE_GUIDE } },
  ];
  repos.push({ owner: 'octo', name: 'retired-greeter', description: 'Archived greeter sample', language: 'Rust', stars: 99, archived: true, files: { 'README.md': '# retired\n' } });
  const pulls = [
    { number: 1, title: 'Add greeter module', body: 'Introduces greet() and the Greeter class.', state: 'closed', merged: true, base: commits[0], head: commits[1], date: DATE(1) },
    { number: 3, title: 'Add greeter docs', body: 'Documents the greeter usage.', state: 'open', merged: false, base: commits[1], head: commits[2], date: DATE(2) },
  ];
  const issues = [
    { number: 2, title: 'Greeter should support a prefix', body: 'The greeter needs a configurable prefix.', state: 'open', date: DATE(1), comments: [{ id: 21, body: 'Agreed, the Greeter class takes one now.', date: DATE(2) }] },
  ];
  for (let i = 0; i < 51; i++) issues.push({ number: 100 + i, title: `Paged history issue ${i}`, body: 'History paging evidence.', state: 'open', date: DATE(i), comments: [] });
  const packages = [
    { name: 'tiny-health', version: '1.2.3', versions: ['1.0.0', '1.2.3'], description: 'Tiny health-check helpers', dependencies: { 'tiny-greet': '^2.0.0' }, keywords: ['health', 'check'] },
    { name: 'tiny-greet', version: '2.1.0', versions: ['2.0.0', '2.1.0'], description: 'Greeting helpers for health dashboards', dependencies: {}, keywords: ['greet', 'health'] },
    { name: 'tiny-pulse', version: '0.4.0', versions: ['0.4.0'], description: 'Pulse monitor for health checks', dependencies: {}, keywords: ['health', 'pulse'] },
  ];
  return { files, commits, branchCommits: [], head, repos, pulls, issues, packages, githubResponders: new Map() };
}

/**
 * Add an open pull request `number` to octo/demo whose head is a branch commit
 * off main adding `fileCount` small modules plus one `hugeLines`-line file: more
 * changed files than one files page and a patch larger than one patch window.
 * Its head commit (branch `feature-<number>`) resolves through commits,
 * contents and trees.
 */
export function addBulkPullRequest(model, { number = 4, fileCount = 350, hugeLines = 3000 } = {}) {
  const files = {};
  for (let i = 0; i < fileCount; i++) files[`pkg/mod${String(i).padStart(2, '0')}.ts`] = `// healthMarker bulk-${i}\nexport const mod${i} = ${i};\n`;
  files['pkg/huge.ts'] = Array.from({ length: hugeLines }, (_, i) => `export const huge${i} = ${i}; // healthMarker huge line ${i + 1}`).join('\n') + '\n';
  const parent = model.head;
  const message = `Add ${fileCount + 1} bulk modules`;
  // `parent` reads through: the git mock rewrites main's commit ids after configureModel.
  const head = { oid: sha(`branch ${number} ${message}`), message, date: DATE(4), get parent() { return parent.oid; }, added: Object.keys(files).toSorted(), tree: sha(`branch tree ${number}`), files, branch: `feature-${number}` };
  model.branchCommits.push(head);
  const reviews = Array.from({ length: 105 }, (_, i) => ({ id: 4000 + i, body: `Review evidence ${i}`, state: i % 2 ? 'APPROVED' : 'COMMENTED', date: DATE(4) }));
  const reviewComments = Array.from({ length: 105 }, (_, i) => ({ id: 5000 + i, body: `Inline review evidence ${i}`, date: DATE(4), path: 'pkg/mod00.ts', line: 2, side: 'RIGHT' }));
  model.pulls.push({ number, title: 'Bulk greeter modules', body: 'Adds the bulk modules and the huge table.', state: 'open', merged: false, base: parent, head, date: DATE(4), reviews, reviewComments });
  return model;
}

/** A file's text as `commit` added it: the commit's own files (a branch commit) or the main tree. */
export const textOf = (model, commit, p) => commit.files?.[p] ?? model.files[p];

export const findRepo = (model, owner, name) => model.repos.find(r => r.owner === owner && r.name === name);

/** A commit by ref: HEAD/main (or none) is the head; else a SHA, SHA prefix or branch name. */
export function commitByRef(model, ref) {
  if (!ref || ref === 'HEAD' || ref === 'main') return model.head;
  return [...model.commits, ...model.branchCommits].find(c => c.oid === ref || c.oid.startsWith(ref) || c.branch === ref);
}

/** Files visible at a commit: those added by it and its ancestors (non-demo repos have one snapshot). */
export function filesAt(model, repo, commit) {
  if (repo.files !== model.files) return repo.files;
  // A branch commit (a pull request head off main) sees its parent's tree plus its own files.
  if (commit.files) return { ...filesAt(model, repo, commitByRef(model, commit.parent)), ...commit.files };
  const visible = {};
  for (const c of model.commits) {
    for (const p of c.added) visible[p] = model.files[p];
    if (c === commit) break;
  }
  return visible;
}
