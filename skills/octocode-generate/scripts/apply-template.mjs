#!/usr/bin/env node
// Layer baseline templates onto a scaffolded project: create missing files, merge package scripts, never overwrite.
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fail, errorMessage } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

const HELP = `Usage: node <skill-folder>/scripts/apply-template.mjs <target-dir> <template>... [options]
       node <skill-folder>/scripts/apply-template.mjs --list

Layers one or more baseline templates from <skill-folder>/templates onto a project, in the order given.
Without --write it is a dry run: it prints what would happen and changes nothing.
A file that already exists is never overwritten (action skip-exists, or unchanged when identical).
Template scripts merge into <target-dir>/package.json: absent names are added, existing ones kept
(npm init's "no test specified" placeholder counts as absent).

Options:
  --list            List templates with their descriptions, scripts and dev dependencies
  --pm <name>       npm, pnpm, yarn or bun: fills {{run}}, {{install}}, {{addDev}} and {{ciSetup}} (default npm)
  --name <name>     Project name for {{name}} (default: the target folder name)
  --node <version>  Node.js version for CI {{node}} (default lts/*)
  --write           Apply the changes
  --help            Show this help

Output: one JSON object on stdout.
  ok, write, target, templates, vars
  files     [{template, path, action}]: action create, skip-exists or unchanged
  scripts   {added:[name], kept:[{name, existing, template}], noManifest}: noManifest when package.json is absent
  install   dev-dependency install commands the templates need, for check-versions.mjs first
  notes     each template's notes
Effects: with --write, creates directories and files and rewrites package.json scripts; otherwise read-only.
Exit codes: 0 ok; 1 unknown template, unresolved placeholder or write failure; 2 invalid input,
  as {"ok":false,"error":{code,message}}.`;

const PLACEHOLDER_TEST = /no test specified/;
const TEMPLATES = join(dirname(fileURLToPath(import.meta.url)), '..', 'templates');

const PACKAGE_MANAGERS = {
  npm: { run: 'npm run', install: 'npm ci', addDev: 'npm install --save-dev',
    setup: node => `- uses: actions/setup-node@v7\n  with:\n    node-version: ${node}\n    cache: npm\n- run: npm ci` },
  pnpm: { run: 'pnpm run', install: 'pnpm install --frozen-lockfile', addDev: 'pnpm add --save-dev',
    setup: node => `- uses: pnpm/action-setup@v6\n- uses: actions/setup-node@v7\n  with:\n    node-version: ${node}\n    cache: pnpm\n- run: pnpm install --frozen-lockfile` },
  yarn: { run: 'yarn run', install: 'yarn install --immutable', addDev: 'yarn add --dev',
    setup: node => `- uses: actions/setup-node@v7\n  with:\n    node-version: ${node}\n- run: npm install --global corepack && corepack enable\n- run: yarn install --immutable` },
  bun: { run: 'bun run', install: 'bun install --frozen-lockfile', addDev: 'bun add --dev',
    setup: () => '- uses: oven-sh/setup-bun@v2\n- run: bun install --frozen-lockfile' },
};

/** Template names with their manifests. */
export function listTemplates(root = TEMPLATES) {
  return readdirSync(root, { withFileTypes: true }).filter(entry => entry.isDirectory()).map(entry => ({
    name: entry.name, ...JSON.parse(readFileSync(join(root, entry.name, 'template.json'), 'utf8')),
  })).toSorted((a, b) => a.name.localeCompare(b.name));
}

function templateFiles(dir, base = dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return templateFiles(path, base);
    return entry.name === 'template.json' ? [] : [relative(base, path)];
  });
}

/** Installed path for a template path: dot-<name> segments become .<name>; a .tmpl suffix is dropped. */
export function outputPath(templatePath) {
  return templatePath.split(sep).map(part => part.startsWith('dot-') ? `.${part.slice(4)}` : part).join(sep).replace(/\.tmpl$/, '');
}

/** Replace {{var}}; a placeholder alone on its line indents every line of a multi-line value. Throws on an unknown var. */
export function render(text, vars, where) {
  return text.replace(/^([ \t]*)\{\{(\w+)\}\}[ \t]*$|\{\{(\w+)\}\}/gm, (match, indent, lineVar, inlineVar) => {
    const name = lineVar ?? inlineVar;
    if (!(name in vars)) throw new Error(`${where}: unknown placeholder {{${name}}}`);
    const value = String(vars[name]);
    return lineVar ? value.split('\n').map(line => indent + line).join('\n') : value;
  });
}

/** Plan (and with write, apply) templates onto target. */
export function applyTemplates({ target, templates, pm = 'npm', name, node = 'lts/*', write = false, root = TEMPLATES }) {
  const manager = PACKAGE_MANAGERS[pm];
  if (!manager) throw Object.assign(new Error(`--pm must be one of ${Object.keys(PACKAGE_MANAGERS).join(', ')}`), { code: 'usage' });
  const known = new Map(listTemplates(root).map(template => [template.name, template]));
  const unknown = templates.filter(template => !known.has(template));
  if (unknown.length) throw Object.assign(new Error(`unknown template ${unknown.join(', ')}; available: ${[...known.keys()].join(', ')}`), { code: 'unknown-template' });
  const dir = resolve(target);
  const vars = { name: name ?? basename(dir), pm, node, run: manager.run, install: manager.install, addDev: manager.addDev, ciSetup: manager.setup(node) };
  const report = { ok: true, write, target: dir, templates, vars, files: [], scripts: { added: [], kept: [], noManifest: false }, install: [], notes: [] };
  const pending = new Map();
  for (const template of templates) {
    const manifest = known.get(template);
    for (const file of templateFiles(join(root, template))) {
      const path = outputPath(file);
      const content = render(readFileSync(join(root, template, file), 'utf8'), vars, `${template}/${file}`);
      const destination = join(dir, path);
      const current = pending.has(path) ? pending.get(path) : existsSync(destination) ? readFileSync(destination, 'utf8') : null;
      const action = current === null ? 'create' : current === content ? 'unchanged' : 'skip-exists';
      if (action === 'create') pending.set(path, content);
      report.files.push({ template, path, action });
    }
    if (manifest.devDependencies?.length) report.install.push(`${manager.addDev} ${manifest.devDependencies.join(' ')}`);
    report.install.push(...manifest.install ?? []);
    if (manifest.notes) report.notes.push(`${template}: ${manifest.notes}`);
  }
  const pkgPath = join(dir, 'package.json');
  const wanted = templates.flatMap(template => Object.entries(known.get(template).scripts ?? {}).map(([key, value]) => [key, render(value, vars, `${template}/template.json`), template]));
  let pkg = null;
  if (wanted.length) {
    pkg = existsSync(pkgPath) ? JSON.parse(readFileSync(pkgPath, 'utf8')) : null;
    if (!pkg) report.scripts.noManifest = true;
    else {
      pkg.scripts ??= {};
      for (const [key, value, template] of wanted) {
        // npm init writes a failing placeholder test script: it is not a test suite, so a template replaces it.
        if (pkg.scripts[key] === undefined || (key === 'test' && PLACEHOLDER_TEST.test(pkg.scripts[key]))) { pkg.scripts[key] = value; report.scripts.added.push(key); }
        else if (pkg.scripts[key] !== value && !report.scripts.added.includes(key)) report.scripts.kept.push({ name: key, existing: pkg.scripts[key], template });
      }
    }
  }
  if (write) {
    for (const [path, content] of pending) {
      mkdirSync(dirname(join(dir, path)), { recursive: true });
      writeFileSync(join(dir, path), content, { flag: 'wx' });
    }
    if (pkg && report.scripts.added.length) writeFileSync(pkgPath, `${JSON.stringify(pkg, null, 2)}\n`);
  }
  return report;
}

function parseArgs(argv) {
  const options = { templates: [], write: false };
  const positional = [];
  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];
    if (arg === '--write') options.write = true;
    else if (['--pm', '--name', '--node'].includes(arg)) {
      const value = argv[++index];
      if (!value) throw new Error(`${arg} needs a value`);
      options[arg.slice(2)] = value;
    } else if (arg.startsWith('--')) throw new Error(`unknown option ${arg}`);
    else positional.push(arg);
  }
  if (positional.length < 2) throw new Error('give a target directory and at least one template (--list shows them)');
  [options.target, ...options.templates] = positional;
  return options;
}

if (isMainModule(import.meta.url)) {
  const argv = process.argv.slice(2);
  if (argv.includes('--help') || argv.includes('-h')) { console.log(HELP); process.exit(0); }
  if (argv.includes('--list')) { console.log(JSON.stringify(listTemplates(), null, 2)); process.exit(0); }
  let options;
  try { options = parseArgs(argv); } catch (error) { fail('usage', errorMessage(error), 2); }
  try {
    console.log(JSON.stringify(applyTemplates(options), null, 2));
  } catch (error) {
    fail(error.code ?? 'failed', errorMessage(error), error.code === 'usage' ? 2 : 1);
  }
}
