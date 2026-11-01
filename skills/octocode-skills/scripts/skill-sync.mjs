#!/usr/bin/env node
// skill-sync — plan/symlink a local skill folder into vendor skill dirs.
// Default is dry-run. Use --approve when the user's request covers the planned writes.
// Usage:
//   node skill-sync.mjs <skill-dir> [--platforms top|all|claude,cursor,...]
//   node skill-sync.mjs <skill-dir> --platforms top --approve
//   node skill-sync.mjs <skill-dir> --platforms top --approve --force
//   node skill-sync.mjs --list-vendors
// Exit: 0 ok, 1 usage/validation/write failure.

import {
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
} from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { PRIMARY, printVendors, resolvePlatformIds } from './lib/vendors.mjs';
import {
  applyRow,
  assertDestinationWithin,
  assertSafeSkillName,
  destinationPath,
  inspectDest,
  isWin,
  linksToSource,
  planRows,
  readFrontmatterName,
} from './lib/link-plan.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));

function usage() {
  return `skill-sync <skill-dir> [options]

Symlink a local skill folder into vendor skill directories.
Default is dry-run (plan only). Use --approve when the request covers the plan.

Options:
  --platforms <list>   top | all | comma ids (default: top)
                       top = claude,cursor,codex
  --project-root <dir> also plan/write project-scope destinations under <dir>
  --name <skill-name>  override destination folder name (default: source folder)
  --approve            apply the authorized symlink plan
  --force              replace existing destination (only with --approve)
  --list-vendors       print vendor map and exit
  --self-test          run path-safety regression checks and exit
  --json               machine-readable plan/result
  --help, -h           this help

Examples:
  node scripts/skill-sync.mjs ../octocode-skills
  node scripts/skill-sync.mjs ../octocode-skills --platforms top --approve
  node scripts/skill-sync.mjs . --platforms claude,cursor --approve --force
`;
}

const FLAGS = new Map([['--help', 'help'], ['-h', 'help'], ['--approve', 'approve'], ['--force', 'force'], ['--list-vendors', 'listVendors'], ['--self-test', 'selfTest'], ['--json', 'json']]);
const VALUES = new Map([['--platforms', 'platforms'], ['--project-root', 'projectRoot'], ['--name', 'name']]);

function parseArgs(argv) {
  const out = {
    skillDir: null,
    platforms: 'top',
    projectRoot: null,
    name: null,
    approve: false,
    force: false,
    listVendors: false,
    selfTest: false,
    json: false,
    help: false,
  };
  const rest = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (FLAGS.has(a)) out[FLAGS.get(a)] = true;
    else if (VALUES.has(a)) {
      const value = argv[i + 1];
      if (!value || value.startsWith('-')) {
        console.error(`${a} requires a value.`);
        process.exit(1);
      }
      out[VALUES.get(a)] = value;
      i++;
    }
    else if (a.startsWith('-')) {
      console.error(`Unknown flag: ${a}`);
      process.exit(1);
    } else rest.push(a);
  }
  out.skillDir = rest[0] || null;
  if (rest.length > 1) { console.error('Expected one skill directory.'); process.exit(1); }
  return out;
}

const expectThrow = (failures, label, fn) => {
  try {
    fn();
    failures.push(`${label}: expected rejection`);
  } catch {
    // Expected.
  }
};

function nameAndPathChecks(failures) {
  for (const name of ['', '.', '..', '../escape', 'nested/escape', 'nested\\escape', '\0']) {
    expectThrow(failures, `unsafe name ${JSON.stringify(name)}`, () => assertSafeSkillName(name));
  }
  for (const name of ['safe-skill', 'safe_skill', 'safe.skill', 'Skill2']) {
    try {
      if (assertSafeSkillName(name) !== name) failures.push(`valid name changed: ${name}`);
    } catch (err) {
      failures.push(`valid name rejected (${name}): ${err.message}`);
    }
  }

  const root = resolve(HERE, '.self-test-vendor', 'skills');
  const safe = destinationPath(root, 'safe-skill');
  if (safe !== join(root, 'safe-skill')) failures.push('safe destination resolved incorrectly');
  expectThrow(failures, 'parent destination', () => assertDestinationWithin(root, resolve(root, '..')));
  expectThrow(failures, 'root destination', () => assertDestinationWithin(root, root));

  if (JSON.stringify(resolvePlatformIds('all')) !== JSON.stringify(PRIMARY)) {
    failures.push('all must expand to each canonical platform exactly once');
  }
  const repeated = resolvePlatformIds('codex,claude,codex');
  if (JSON.stringify(repeated) !== JSON.stringify(['codex', 'claude'])) {
    failures.push('platform ids must de-duplicate');
  }
}

function symlinkChecks(failures, temp) {
  const dangling = join(temp, 'dangling');
  symlinkSync(join(temp, 'absent'), dangling, isWin ? 'junction' : 'dir');
  if (inspectDest(dangling).state !== 'symlink') failures.push('dangling symlink must remain a conflict');
  const result = applyRow({ destDir: temp, destPath: dangling, action: 'conflict-symlink', reason: 'dangling link' }, temp, { force: false });
  if (result.result !== 'blocked' || !lstatSync(dangling).isSymbolicLink()) failures.push('unapproved conflict was changed');
  const nested = join(temp, 'parent', 'source');
  mkdirSync(nested, { recursive: true });
  const protectedResult = applyRow({ destDir: temp, destPath: join(temp, 'parent'), action: 'conflict-dir', reason: 'contains source' }, nested, { force: true });
  if (protectedResult.result !== 'blocked' || !existsSync(nested)) failures.push('source-containing destination was not protected');
  const realSource = join(temp, 'real-skill');
  mkdirSync(realSource);
  const aliasSource = join(temp, 'alias-skill');
  symlinkSync(realSource, aliasSource, isWin ? 'junction' : 'dir');
  const vendorLink = join(temp, 'vendor-link');
  symlinkSync(realSource, vendorLink, isWin ? 'junction' : 'dir');
  if (!linksToSource(inspectDest(vendorLink).resolved, aliasSource)) failures.push('a link to the source reached through a symlinked folder must count as linked');
}

function runSelfTest() {
  const failures = [];
  nameAndPathChecks(failures);
  const source = resolve(HERE, '..');
  const cases = [
    { args: ['--platforms'], error: '--platforms requires a value.' },
    { args: ['--platforms', '--approve'], error: '--platforms requires a value.' },
    { args: ['--platforms', ','], error: 'Select at least one platform.' },
    { args: ['--name', ''], error: '--name requires a value.' },
  ];
  for (const { args, error } of cases) {
    const result = spawnSync(process.execPath, [join(HERE, 'skill-sync.mjs'), source, ...args, '--json'], { encoding: 'utf8' });
    if (result.status !== 1 || !result.stderr.includes(error)) failures.push(`invalid option did not report ${error}: ${JSON.stringify(args)}`);
  }
  const valid = spawnSync(process.execPath, [join(HERE, 'skill-sync.mjs'), source, '--platforms', 'codex', '--json'], { encoding: 'utf8' });
  try {
    const plan = JSON.parse(valid.stdout);
    if (valid.status !== 0 || plan.mode !== 'dry-run' || plan.platforms?.join() !== 'codex' || plan.rows?.length !== 1) failures.push('valid dry-run control failed');
  } catch { failures.push('valid dry-run control did not return JSON'); }
  const temp = mkdtempSync(join(tmpdir(), 'skill-sync-'));
  try {
    symlinkChecks(failures, temp);
  } finally { rmSync(temp, { recursive: true, force: true }); }
  return { ok: failures.length === 0, failures };
}

// --help, --list-vendors, and --self-test answer and exit before any skill is read.
function runInfoModes(args) {
  if (args.help) {
    console.log(usage());
    process.exit(0);
  }
  if (args.listVendors) {
    printVendors(args.json);
    process.exit(0);
  }
  if (args.selfTest) {
    const result = runSelfTest();
    console.log(JSON.stringify(result, null, 2));
    process.exit(result.ok ? 0 : 1);
  }
}

// The source folder and the install name, or an exit with the reason.
function resolveSkill(args) {
  if (!args.skillDir) {
    console.error('Missing <skill-dir>.\n');
    console.error(usage());
    process.exit(1);
  }
  if (args.force && !args.approve) {
    console.error('--force requires --approve to apply replacements.');
    process.exit(1);
  }
  const sourcePath = resolve(args.skillDir);
  const skillMd = join(sourcePath, 'SKILL.md');
  if (!existsSync(skillMd)) {
    console.error(`Not a skill folder (missing SKILL.md): ${sourcePath}`);
    process.exit(1);
  }
  const fmName = readFrontmatterName(skillMd);
  let skillName;
  try {
    skillName = assertSafeSkillName(args.name === null ? basename(sourcePath) : args.name);
  } catch (err) {
    console.error(err.message || String(err));
    process.exit(1);
  }
  return { sourcePath, fmName, skillName };
}

function printDryRun(args, plan) {
  const { sourcePath, skillName, frontmatterName: fmName, platforms: platformIds, rows } = plan;
  if (args.json) {
    console.log(JSON.stringify({ ok: true, ...plan, hint: 'Re-run with --approve when the request covers this plan' }, null, 2));
    return;
  }
  console.log('DRY-RUN — no writes. Use --approve to apply the authorized plan.\n');
  console.log(`Source:  ${sourcePath}`);
  console.log(`Name:    ${skillName}${plan.nameMismatch ? ` (frontmatter name: ${fmName})` : ''}`);
  console.log(`Targets: ${platformIds.join(', ')}`);
  if (plan.projectRoot) console.log(`Project: ${plan.projectRoot}`);
  console.log('');
  for (const r of rows) {
    console.log(`${r.action.padEnd(18)} ${r.platform}/${r.scope}`);
    console.log(`  → ${r.destPath}`);
    console.log(`  ${r.reason}`);
  }
  console.log('\nTo apply when authorized:');
  console.log(`  node ${join(HERE, 'skill-sync.mjs')} ${args.skillDir} --platforms ${args.platforms}${args.projectRoot ? ` --project-root ${args.projectRoot}` : ''}${args.name ? ` --name ${args.name}` : ''} --approve`);
}

// Applies every row; returns how many were blocked or failed.
function applyPlan(args, { sourcePath, skillName, rows }) {
  const results = [];
  for (const row of rows) {
    try {
      results.push(applyRow(row, sourcePath, { force: args.force }));
    } catch (err) {
      results.push({ ...row, result: 'error', detail: err.message || String(err) });
    }
  }
  const failed = results.filter((r) => r.result === 'error' || r.result === 'blocked').length;

  if (args.json) {
    console.log(JSON.stringify({ ok: failed === 0, mode: 'apply', sourcePath, skillName, results }, null, 2));
  } else {
    console.log(`APPLY — symlink sync for ${skillName}\n`);
    for (const r of results) {
      console.log(`${String(r.result).padEnd(10)} ${r.platform}/${r.scope}`);
      console.log(`  → ${r.destPath}`);
      console.log(`  ${r.detail}`);
    }
    const linked = results.filter((r) => r.result === 'linked' || r.result === 'replaced').length;
    const skipped = results.filter((r) => r.result === 'skipped').length;
    console.log(`\nlinked/replaced=${linked} skipped=${skipped} blocked/error=${failed}`);
  }
  return failed;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  runInfoModes(args);
  const { sourcePath, fmName, skillName } = resolveSkill(args);
  const platformIds = resolvePlatformIds(args.platforms);
  let rows;
  try {
    rows = planRows(sourcePath, skillName, platformIds, args.projectRoot);
  } catch (error) {
    console.error(`Cannot plan destinations: ${error.message}`);
    process.exit(1);
  }

  const plan = {
    mode: args.approve ? 'apply' : 'dry-run',
    sourcePath,
    skillName,
    frontmatterName: fmName,
    nameMismatch: Boolean(fmName && fmName !== skillName),
    platforms: platformIds,
    projectRoot: args.projectRoot ? resolve(args.projectRoot) : null,
    rows,
  };

  if (!args.approve) {
    printDryRun(args, plan);
    process.exit(0);
  }
  process.exit(applyPlan(args, plan) ? 1 : 0);
}

main();
