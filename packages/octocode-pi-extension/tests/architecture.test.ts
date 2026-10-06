import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sourceRoot = path.join(packageRoot, 'src');

const walk = (dir: string): string[] => fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => (entry.isDirectory() ? walk(path.join(dir, entry.name)) : entry.name.endsWith('.ts') ? [path.join(dir, entry.name)] : []));
const files = walk(sourceRoot).sort();
const domainOf = (file: string): string => {
  const [first] = path.relative(sourceRoot, file).split(path.sep);
  return first!.endsWith('.ts') ? '(root)' : first!;
};

function importsOf(file: string): string[] {
  const text = fs.readFileSync(file, 'utf8');
  // `from '…'`, dynamic `import('…')`, side-effect `import '…'` and `require('…')` all create a dependency.
  return [...text.matchAll(/(?:from\s+|import\s*\(\s*|import\s+|require\(\s*)(['"])(\.{1,2}\/[^'"]+)\1/g)].flatMap((match) => {
    const target = path.resolve(path.dirname(file), match[2]!.replace(/\.js$/, '.ts'));
    return files.includes(target) ? [target] : [];
  });
}

/**
 * The layering: each domain may depend only on the domains listed. `shared` is the leaf; the root modules
 * (`index.ts` wires everything, `prompt.ts` and `skills.ts` sit beside it) may use any domain.
 */
const ALLOWED: Record<string, string[]> = {
  shared: [],
  api: ['shared'],
  ask: ['shared'],
  files: ['shared'],
  team: ['shared', 'agentdb'],
  hooks: ['shared'],
  mcp: ['shared'],
  web: ['shared'],
  browser: ['shared', 'web'],
  compaction: ['shared', 'files'],
  ui: ['shared'],
  subagents: ['shared', 'mcp', 'team'],
  agentdb: ['shared'],
  sessions: ['shared', 'agentdb'],
  backlog: ['shared', 'agentdb'],
  memory: ['shared', 'agentdb'],
};

describe('source layout', () => {
  it('keeps src/ a few root modules plus one folder per domain', () => {
    const rootFiles = fs.readdirSync(sourceRoot, { withFileTypes: true }).filter((entry) => entry.isFile()).map((entry) => entry.name).sort();
    expect(rootFiles).toEqual(['index.ts', 'prompt.ts', 'skills.ts', 'turn.ts']);
    const folders = fs.readdirSync(sourceRoot, { withFileTypes: true }).filter((entry) => entry.isDirectory()).map((entry) => entry.name).sort();
    expect(folders).toEqual(Object.keys(ALLOWED).sort());
    // shared/ is the leaf every domain uses, so it holds one more small helper than a feature folder may.
    for (const folder of folders) expect(walk(path.join(sourceRoot, folder)).length, `${folder}/ is a god folder`).toBeLessThanOrEqual(folder === 'shared' ? 13 : 12);
  });

  it('lets each domain depend only on the domains below it', () => {
    const violations = files.flatMap((file) => {
      const domain = domainOf(file);
      if (domain === '(root)') return [];
      return importsOf(file)
        .filter((target) => domainOf(target) !== domain && !(ALLOWED[domain] ?? []).includes(domainOf(target)))
        .map((target) => `${path.relative(sourceRoot, file)} -> ${path.relative(sourceRoot, target)}`);
    });
    expect(violations).toEqual([]);
  });

  it('never imports the root modules from a domain, and nothing imports index.ts', () => {
    const upward = files.filter((file) => domainOf(file) !== '(root)').flatMap((file) => importsOf(file).filter((target) => domainOf(target) === '(root)').map((target) => `${path.relative(sourceRoot, file)} -> ${path.relative(sourceRoot, target)}`));
    expect(upward).toEqual([]);
    expect(files.filter((file) => importsOf(file).includes(path.join(sourceRoot, 'index.ts'))).map((file) => path.relative(sourceRoot, file))).toEqual([]);
  });

  it('has no import cycles between files', () => {
    const graph = new Map(files.map((file) => [file, importsOf(file)]));
    const cycles: string[] = [];
    const done = new Set<string>();
    const visit = (file: string, trail: string[]): void => {
      if (trail.includes(file)) return void cycles.push([...trail.slice(trail.indexOf(file)), file].map((f) => path.relative(sourceRoot, f)).join(' -> '));
      if (done.has(file)) return;
      for (const next of graph.get(file) ?? []) visit(next, [...trail, file]);
      done.add(file);
    };
    for (const file of files) visit(file, []);
    expect(cycles).toEqual([]);
  });

  it('keeps one owner per shared helper', () => {
    // Each helper has one owner (src/shared unless one domain alone uses it); a copy elsewhere drifts (as OCTOCODE_REVIEW=true once did from OCTOCODE_HOOKS=true).
    const owners: Array<[RegExp, string]> = [
      [/process\.kill\(/, 'shared/process.ts'],
      [/instanceof Error \? [^:]+: String\(/, 'shared/util.ts'],
      [/createHash\(['"]sha256['"]\)/, 'shared/atomic.ts'],
      [/env\[[^\]]+\](?:\?\.trim\(\))? === '1'/, 'shared/env.ts'],
      [/signal\.reason instanceof Error \?/, 'browser/cdp.ts'],
    ];
    const copies = files.flatMap((file) => {
      const relative = path.relative(sourceRoot, file).split(path.sep).join('/');
      const text = fs.readFileSync(file, 'utf8');
      return owners.filter(([pattern, owner]) => relative !== owner && pattern.test(text)).map(([pattern]) => `${relative}: ${pattern.source}`);
    });
    expect(copies).toEqual([]);
  });

  it('names the .octocode folder only in shared/home.ts', () => {
    // Every path under `~/.octocode` or `<repo root>/.octocode` goes through src/shared/home.ts, so one module decides
    // where things live. Prose that mentions `.octocode/tmp/` (prompts, tool descriptions) is fine; path segments are not.
    const owners = new Set(['shared/home.ts']);
    const segment = /(['"`])\.octocode\1|homedir\(\)\s*,\s*(['"`])\.octocode/;
    const hits = files.map((file) => path.relative(sourceRoot, file).split(path.sep).join('/')).filter((relative) => !owners.has(relative) && segment.test(fs.readFileSync(path.join(sourceRoot, relative), 'utf8')));
    expect(hits).toEqual([]);
  });

  it('imports only declared packages, Pi only as a peer, and never @octocodeai/config', () => {
    const manifest = JSON.parse(fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8')) as Record<string, Record<string, string> | undefined>;
    const runtime = new Set([...Object.keys(manifest['dependencies'] ?? {}), ...Object.keys(manifest['peerDependencies'] ?? {})]);
    const packageOf = (specifier: string) => (specifier.startsWith('@') ? specifier.split('/').slice(0, 2).join('/') : specifier.split('/')[0]!);
    const bare = files.flatMap((file) =>
      [...fs.readFileSync(file, 'utf8').matchAll(/(?:from\s+|import\s*\(\s*|import\s+|require\(\s*)(['"])([^.'"][^'"]*)\1/g)]
        .map((match) => match[2]!)
        .filter((specifier) => !specifier.startsWith('node:'))
        .map((specifier) => ({ file: path.relative(sourceRoot, file), name: packageOf(specifier) })),
    );
    expect(bare.filter(({ name }) => !runtime.has(name)).map(({ file, name }) => `${file}: ${name}`)).toEqual([]);
    expect(bare.filter(({ name }) => name === '@octocodeai/config').map(({ file }) => file)).toEqual([]);
    // Pi provides its own packages at runtime; bundling a second copy would split its module state.
    const piPackages = [...new Set(bare.map(({ name }) => name).filter((name) => name.startsWith('@earendil-works/pi-')))];
    expect(piPackages.length).toBeGreaterThan(0);
    for (const name of piPackages) {
      expect(manifest['peerDependencies']?.[name], `${name} is a peer dependency`).toBeDefined();
      expect(manifest['dependencies']?.[name], `${name} is not a runtime dependency`).toBeUndefined();
    }
  });

  it('is the only package in the repository that imports Pi', () => {
    const packagesDir = path.resolve(packageRoot, '..');
    const others = fs.readdirSync(packagesDir, { withFileTypes: true }).filter((entry) => entry.isDirectory() && path.join(packagesDir, entry.name) !== packageRoot);
    const offenders = others.flatMap((entry) => {
      const src = path.join(packagesDir, entry.name, 'src');
      if (!fs.existsSync(src)) return [];
      return walk(src).filter((file) => /['"]@earendil-works\/pi-/.test(fs.readFileSync(file, 'utf8'))).map((file) => path.relative(packagesDir, file));
    });
    expect(offenders).toEqual([]);
  });

  it('locates its package root at any depth', async () => {
    const { packageRoot: found } = await import('../src/shared/package.js');
    expect(found()).toBe(packageRoot);
    expect(found(new URL('../src/subagents/process.ts', import.meta.url).href)).toBe(packageRoot);
  });
});
