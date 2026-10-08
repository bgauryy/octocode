import fs from 'node:fs';
import path from 'node:path';
import { isValidSkillName, listSkills } from '../registry.js';
import { contentFreshness } from '../freshness.js';
import { getPlatformSkillsDir } from '../platforms.js';
import { skillLocations } from '../checker.js';
import { EXIT } from '../../../exit-codes.js';
import {
  getCanonicalSkillsDir,
  parseSkillPlatforms,
  type SkillPlatform,
} from '@octocodeai/octocode-skill-installer';
import { bold, dim } from '../../../../utils/colors.js';
import { reportFailure } from './fail.js';

interface RemoveOptions {
  all: boolean;
  platform: string | null;
  dryRun: boolean;
  /** Also delete real (non-link) directories outside the canonical store. */
  force?: boolean;
  json: boolean;
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
  const canonical = path.join(getCanonicalSkillsDir(), name);
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

/** Every location where the skill is present (a dangling link included). */
function installedTargets(name: string): Target[] {
  const { home, platforms, workspace } = skillLocations(name);
  const seen = new Set<string>();
  return [home, ...platforms, workspace]
    .filter(location => location.status !== 'missing')
    .filter(location => !seen.has(location.path) && seen.add(location.path))
    .map(location => ({ target: location.label, path: location.path }));
}

export function runRemove(skillNames: string[], opts: RemoveOptions): void {
  let names = skillNames;
  if (opts.all) {
    try {
      names = fs
        .readdirSync(getCanonicalSkillsDir(), { withFileTypes: true })
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
    return reportFailure('Specify a skill name or use --all.', opts.json);
  }

  let platforms: SkillPlatform[] | null = null;
  if (opts.platform) {
    const parsed = parseSkillPlatforms(opts.platform);
    if (parsed.error) return reportFailure(parsed.error, opts.json);
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
            path: path.join(getCanonicalSkillsDir(), name),
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
  if (!success) process.exitCode = EXIT.GENERAL;
}
