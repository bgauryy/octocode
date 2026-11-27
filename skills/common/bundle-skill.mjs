#!/usr/bin/env node
// Bundles a skill's src/*.mjs entries into its scripts/ folder, so the installed skill runs with
// no packages: an entry may import the workspace packages its own package.json declares (such as
// @octocodeai/config, built first). Run from the skill folder through its package scripts:
// `yarn build` writes the bundles; `yarn lint` (--check) fails when one is stale.
import { buildSync } from 'esbuild';
import { chmodSync, existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { isMainModule } from './scripts/lib/entrypoint.mjs';

/** Builds (or with check, compares) every src/*.mjs of the skill at dir; returns the stale targets. */
export function bundleSkill(dir, { check = false } = {}) {
  const stale = [];
  for (const name of readdirSync(join(dir, 'src')).filter(entry => entry.endsWith('.mjs')).toSorted()) {
    const { outputFiles } = buildSync({
      absWorkingDir: dir,
      entryPoints: [join('src', name)],
      bundle: true,
      platform: 'node',
      format: 'esm',
      target: 'node24',
      legalComments: 'none',
      banner: { js: `// Generated from src/${name} by skills/common/bundle-skill.mjs: edit the source, then run \`yarn build\` in this skill.` },
      write: false,
    });
    const target = join(dir, 'scripts', name);
    const content = Buffer.from(outputFiles[0].contents);
    if (existsSync(target) && readFileSync(target).equals(content)) continue;
    stale.push(target);
    if (check) continue;
    writeFileSync(target, content);
    chmodSync(target, 0o755);
  }
  return stale;
}

if (isMainModule(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.some(arg => arg !== '--check')) {
    console.error('Usage: node ../common/bundle-skill.mjs [--check]   (run from a skill folder)');
    process.exit(args.includes('--help') ? 0 : 2);
  }
  const check = args.includes('--check');
  const dir = resolve(process.cwd());
  const stale = bundleSkill(dir, { check });
  for (const file of stale) console.log(`${check ? 'STALE' : 'BUILT'} ${relative(dir, file)}`);
  if (check && stale.length) {
    console.error('Run `yarn build` in this skill to rebuild its bundles.');
    process.exitCode = 1;
  }
}
