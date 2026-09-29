import fs from 'node:fs';
import path from 'node:path';
import { isValidSkillName, listSkills } from '../registry.js';
import { contentFreshness } from '../freshness.js';
import { getSkillsHome } from '../home.js';
import {
  ALL_PLATFORMS,
  getPlatformSkillsDir,
  parsePlatforms,
} from '../platforms.js';
import type { Platform } from '../platforms.js';
import { bold, c, dim } from '../../../../utils/colors.js';

export interface RemoveOptions {
  all: boolean;
  platform: string | null;
  dryRun: boolean;
  /** Also delete real (non-link) directories outside the canonical store. */
  force?: boolean;
  json: boolean;
  jsonErrors?: boolean;
}

type Target = { target: string; path: string };
type Result = Target & {
  status: 'removed' | 'skipped' | 'failed';
  error?: string;
};

function exists(entry: string): boolean {
  try {
    fs.lstatSync(entry);
    return true;
  } catch {
    return false;
  }
}

/**
 * True when deleting `entry` could destroy user data: a real directory or file
 * (not a link) outside the canonical Octocode store whose bytes match neither
 * the canonical copy nor the bundled skill. Links are unlinked only, the
 * canonical store copy is Octocode-owned, and an identical `--mode copy`
 * install is Octocode-owned too; a diverged or user-edited copy is not.
 */
function isUserOwnedContent(
  entry: string,
  target: string,
  name: string
): boolean {
  if (target === 'home') return false;
  try {
    if (fs.lstatSync(entry).isSymbolicLink()) return false;
  } catch {
    return false;
  }
  const canonical = path.join(getSkillsHome(), name);
  if (contentFreshness(canonical, entry) === 'fresh') return false;
  const bundled = listSkills().find(skill => skill.name === name);
  return !(bundled && contentFreshness(bundled.dir, entry) === 'fresh');
}

function remove(entry: string): string | undefined {
  try {
    const stat = fs.lstatSync(entry);
    if (stat.isDirectory() && !stat.isSymbolicLink()) {
      fs.rmSync(entry, { recursive: true, force: true });
    } else {
      fs.unlinkSync(entry);
    }
    return undefined;
  } catch (error) {
    return error instanceof Error ? error.message : String(error);
  }
}

function installedTargets(name: string): Target[] {
  const targets: Target[] = [
    { target: 'home', path: path.join(getSkillsHome(), name) },
  ];
  for (const platform of ALL_PLATFORMS) {
    targets.push({
      target: platform,
      path: path.join(getPlatformSkillsDir(platform), name),
    });
  }
  targets.push({
    target: 'workspace',
    path: path.join(process.cwd(), '.agents', 'skills', name),
  });
  const seen = new Set<string>();
  return targets.filter(
    target =>
      !seen.has(target.path) && seen.add(target.path) && exists(target.path)
  );
}

function fail(message: string, json: boolean, jsonErrors = false): void {
  if (jsonErrors)
    console.log(
      JSON.stringify({ kind: 'octocode.toolError', version: 1, error: message })
    );
  else if (json)
    console.log(JSON.stringify({ success: false, error: message }));
  else console.error(`\n  ${c('red', '✗')} ${message}\n`);
  process.exitCode = 1;
}

export function runRemove(skillNames: string[], opts: RemoveOptions): void {
  let names = skillNames;
  if (opts.all) {
    try {
      names = fs
        .readdirSync(getSkillsHome(), { withFileTypes: true })
        .filter(entry => entry.isDirectory() || entry.isSymbolicLink())
        .map(entry => entry.name);
    } catch {
      names = listSkills().map(skill => skill.folder);
    }
  }
  if (names.length === 0) {
    if (opts.all) {
      if (opts.json)
        console.log(JSON.stringify({ success: true, removed: 0, skills: [] }));
      else console.log('\n  No installed skills found.\n');
      return;
    }
    return fail(
      'Specify a skill name or use --all.',
      opts.json,
      opts.jsonErrors
    );
  }

  let platforms: Platform[] | null = null;
  if (opts.platform) {
    const parsed = parsePlatforms(opts.platform);
    if (parsed.error) return fail(parsed.error, opts.json, opts.jsonErrors);
    platforms = parsed.platforms;
  }

  const records = names.map(name => {
    if (!isValidSkillName(name)) {
      return {
        name,
        nothingFound: false,
        targets: [
          {
            target: 'home',
            path: '',
            status: 'failed' as const,
            error: `Invalid skill name: "${name}".`,
          },
        ],
      };
    }
    const targets = platforms
      ? platforms.map(platform => ({
          target: platform,
          path: path.join(getPlatformSkillsDir(platform), name),
        }))
      : installedTargets(name);
    if (targets.length === 0) {
      return {
        name,
        nothingFound: true,
        targets: [
          {
            target: 'home',
            path: path.join(getSkillsHome(), name),
            status: 'skipped' as const,
          },
        ],
      };
    }
    const results: Result[] = targets.map(target => {
      if (!exists(target.path)) return { ...target, status: 'skipped' };
      if (!opts.force && isUserOwnedContent(target.path, target.target, name)) {
        return {
          ...target,
          status: 'failed',
          error: `Refusing to delete ${target.path}: it is a real directory that differs from the Octocode copy (it may hold your edits). Remove it manually or pass --force.`,
        };
      }
      if (opts.dryRun) return { ...target, status: 'removed' };
      const error = remove(target.path);
      return error
        ? { ...target, status: 'failed', error }
        : { ...target, status: 'removed' };
    });
    return { name, nothingFound: false, targets: results };
  });

  const flat = records.flatMap(record => record.targets);
  const summary = {
    removed: flat.filter(result => result.status === 'removed').length,
    skipped: flat.filter(result => result.status === 'skipped').length,
    failed: flat.filter(result => result.status === 'failed').length,
  };
  const success = summary.failed === 0;
  if (opts.json) {
    console.log(
      JSON.stringify({ success, dryRun: opts.dryRun, skills: records, summary })
    );
  } else {
    console.log(
      `\n  ${bold(opts.dryRun ? 'Remove preview' : 'Removed skills')}`
    );
    for (const record of records) {
      console.log(`  ${record.name}`);
      for (const result of record.targets) {
        console.log(`    ${result.status}: ${dim(result.path)}`);
      }
    }
    console.log(
      `  ${summary.removed} removed; ${summary.skipped} skipped; ${summary.failed} failed\n`
    );
  }
  if (!success) process.exitCode = 1;
}
