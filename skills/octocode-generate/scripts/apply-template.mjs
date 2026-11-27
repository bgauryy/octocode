#!/usr/bin/env node
// Layer baseline templates onto a scaffolded project: create missing files, merge package scripts, never overwrite.
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fail, errorMessage } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';
import { stacksIn } from './lib/stacks.mjs';
import { scriptGates } from './verify-project.mjs';

const HELP = `Usage: node <skill-folder>/scripts/apply-template.mjs <target-dir> <template>... [options]
       node <skill-folder>/scripts/apply-template.mjs --list

Layers one or more baseline templates from <skill-folder>/templates onto a project, in the order given.
Without --write it is a dry run: it prints what would happen and changes nothing.
A file that already exists is never overwritten (action skip-exists, or unchanged when identical); a template's
ensureLines only append missing lines (action append, with the lines). A
file whose equivalent already exists (vitest.config.ts beside vite.config.ts, a second Prettier or ESLint
config) is not added (action skip-alternative, with the existing file). Template scripts merge into <target-dir>/package.json: absent names are added, existing ones kept
(npm init's "no test specified" placeholder counts as absent).

Options:
  --list            List templates with their descriptions, scripts and dev dependencies
  --pm <name>       npm, pnpm, yarn or bun: fills {{run}}, {{install}}, {{addDev}} and {{ciSetup}} (default npm);
                    {{gateSteps}} is one CI step per JavaScript gate the merged package.json scripts provide, the
                    same gates verify-project.mjs runs, with a comment for each missing one;
                    {{dependabotUpdates}} lists one update entry per stack found at <target-dir>, plus GitHub Actions
  --name <name>     Project name for {{name}} (default: the target folder name)
  --node <version>  Node.js version for CI {{node}} (default lts/*)
  --java <version>  JDK version for CI {{java}}: the toolchain the Gradle build declares (default 21)
  --write           Apply the changes
  --help            Show this help

Output: one JSON object on stdout.
  ok, write, target, templates, vars
  files     [{template, path, action, existing, lines}]: action create, append, skip-exists, skip-alternative
            or unchanged
  scripts   {added:[name], kept:[{name, existing, template}], noManifest}: noManifest when package.json is absent
  install   dev-dependency install commands the templates need, for check-versions.mjs first
  notes     each template's notes
Effects: with --write, creates directories and files and rewrites package.json scripts; otherwise read-only.
Exit codes: 0 ok; 1 unknown template, unresolved placeholder or write failure; 2 invalid input,
  as {"ok":false,"error":{code,message}}.`;

const PLACEHOLDER_TEST = /no test specified/;
const TEMPLATES = join(dirname(fileURLToPath(import.meta.url)), '..', 'templates');

// Actions are pinned to the commit SHA of the release in the comment (GitHub's hardening guidance); Dependabot updates them.
const SETUP_NODE = 'actions/setup-node@949feb2413d6458794dcd2491c4babbbce0c15c1 # v7.1.0';
const PACKAGE_MANAGERS = {
  npm: { run: 'npm run', install: 'npm ci', addDev: 'npm install --save-dev', dependabot: 'npm',
    setup: node => `- uses: ${SETUP_NODE}\n  with:\n    node-version: ${node}\n    cache: npm\n- run: npm ci` },
  pnpm: { run: 'pnpm run', install: 'pnpm install --frozen-lockfile', addDev: 'pnpm add --save-dev', dependabot: 'npm',
    setup: node => `- uses: pnpm/action-setup@73e46992e05b19abd31f2f3c20e0e7370d762115 # v6.1.1\n- uses: ${SETUP_NODE}\n  with:\n    node-version: ${node}\n    cache: pnpm\n- run: pnpm install --frozen-lockfile` },
  yarn: { run: 'yarn run', install: 'yarn install --immutable', addDev: 'yarn add --dev', dependabot: 'npm',
    setup: node => `- uses: ${SETUP_NODE}\n  with:\n    node-version: ${node}\n- run: npm install --global corepack && corepack enable\n- run: yarn install --immutable` },
  bun: { run: 'bun run', install: 'bun install --frozen-lockfile', addDev: 'bun add --dev', dependabot: 'bun',
    setup: () => '- uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0\n- run: bun install --frozen-lockfile' },
};

// Dependabot package-ecosystem per stack; javascript follows the package manager.
const DEPENDABOT = { deno: 'deno', rust: 'cargo', go: 'gomod', dart: 'pub', dotnet: 'nuget', swift: 'swift', ruby: 'bundler', php: 'composer', elixir: 'mix' };

/** Dependabot update entries for the stacks found in dir, GitHub Actions last. */
export function dependabotUpdates(dir, pm) {
  const ecosystems = stacksIn(dir).map(stack => {
    if (stack === 'javascript') return PACKAGE_MANAGERS[pm].dependabot;
    if (stack === 'python') return existsSync(join(dir, 'uv.lock')) || (existsSync(join(dir, 'pyproject.toml')) && !existsSync(join(dir, 'poetry.lock'))) ? 'uv' : 'pip';
    if (stack === 'jvm') return existsSync(join(dir, 'pom.xml')) ? 'maven' : 'gradle';
    if (stack === 'swift') return existsSync(join(dir, 'Package.swift')) ? 'swift' : null;
    return DEPENDABOT[stack];
  }).filter(Boolean);
  return [...new Set([...ecosystems, 'github-actions'])]
    .map(ecosystem => `- package-ecosystem: "${ecosystem}"\n  directory: "/"\n  schedule:\n    interval: "weekly"`).join('\n');
}

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

/** CI steps for the JavaScript gates of the scripts package.json will hold; a missing gate becomes a comment. */
function gateSteps(dir, pm, scripts) {
  if (!scripts) return '# no package.json: add the gate steps by hand';
  return scriptGates(dir, pm, scripts).filter(gate => gate.status !== 'not-applicable').map(gate => gate.command
    ? `- name: ${gate.gate}\n  run: ${gate.command.join(' ')}` : `# ${gate.gate} gate missing: ${gate.hint}`).join('\n');
}

/** Plan (and with write, apply) templates onto target. */
export function applyTemplates({ target, templates, pm = 'npm', name, node = 'lts/*', java = '21', write = false, root = TEMPLATES }) {
  const manager = PACKAGE_MANAGERS[pm];
  if (!manager) throw Object.assign(new Error(`--pm must be one of ${Object.keys(PACKAGE_MANAGERS).join(', ')}`), { code: 'usage' });
  const known = new Map(listTemplates(root).map(template => [template.name, template]));
  const unknown = templates.filter(template => !known.has(template));
  if (unknown.length) throw Object.assign(new Error(`unknown template ${unknown.join(', ')}; available: ${[...known.keys()].join(', ')}`), { code: 'unknown-template' });
  const dir = resolve(target);
  const vars = { name: name ?? basename(dir), pm, node, java, run: manager.run, install: manager.install, addDev: manager.addDev, ciSetup: manager.setup(node),
    dependabotUpdates: dependabotUpdates(dir, pm) };
  const report = { ok: true, write, target: dir, templates, vars, files: [], scripts: { added: [], kept: [], noManifest: false }, install: [], notes: [] };
  const pkgPath = join(dir, 'package.json');
  const wanted = templates.flatMap(template => Object.entries(known.get(template).scripts ?? {}).map(([key, value]) => [key, render(value, vars, `${template}/template.json`), template]));
  const pkg = existsSync(pkgPath) ? JSON.parse(readFileSync(pkgPath, 'utf8')) : null;
  if (wanted.length) {
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
  vars.gateSteps = gateSteps(dir, pm, pkg?.scripts ?? null);

  const pending = new Map();
  for (const template of templates) {
    const manifest = known.get(template);
    for (const file of templateFiles(join(root, template))) {
      const path = outputPath(file);
      const content = render(readFileSync(join(root, template, file), 'utf8'), vars, `${template}/${file}`);
      const destination = join(dir, path);
      const current = pending.has(path) ? pending.get(path) : existsSync(destination) ? readFileSync(destination, 'utf8') : null;
      // An equivalent config under another name (vite.config.ts for Vitest, .prettierrc for Prettier) already configures the tool.
      const existing = current === null ? (manifest.alternatives?.[path] ?? []).find(other => existsSync(join(dir, other))) : undefined;
      const action = existing ? 'skip-alternative' : current === null ? 'create' : current === content ? 'unchanged' : 'skip-exists';
      if (action === 'create') pending.set(path, content);
      report.files.push({ template, path, action, ...(existing ? { existing } : {}) });
    }
    if (manifest.devDependencies?.length) report.install.push(`${manager.addDev} ${manifest.devDependencies.join(' ')}`);
    report.install.push(...manifest.install ?? []);
    if (manifest.notes) report.notes.push(`${template}: ${manifest.notes}`);
  }
  // ensureLines appends missing lines (.octocode/ and .env in .gitignore) to a file the scaffold or another template wrote.
  const appends = new Map();
  for (const template of templates) {
    for (const [path, lines] of Object.entries(known.get(template).ensureLines ?? {})) {
      const onDisk = existsSync(join(dir, path)) ? readFileSync(join(dir, path), 'utf8') : null;
      const current = pending.get(path) ?? appends.get(path) ?? onDisk ?? '';
      const present = new Set(current.split('\n').map(line => line.trim()));
      const missing = lines.filter(line => !present.has(line));
      if (!missing.length) { report.files.push({ template, path, action: 'unchanged' }); continue; }
      const content = `${current}${current && !current.endsWith('\n') ? '\n' : ''}${missing.join('\n')}\n`;
      if (onDisk === null) pending.set(path, content); else appends.set(path, content);
      report.files.push({ template, path, action: onDisk === null && !current ? 'create' : 'append', lines: missing });
    }
  }
  if (write) {
    for (const [path, content] of pending) {
      mkdirSync(dirname(join(dir, path)), { recursive: true });
      writeFileSync(join(dir, path), content, { flag: 'wx' });
    }
    for (const [path, content] of appends) writeFileSync(join(dir, path), content);
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
    else if (['--pm', '--name', '--node', '--java'].includes(arg)) {
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
