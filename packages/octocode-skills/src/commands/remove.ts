/**
 * `octocode-skills remove <name> [options]`
 *
 * Uninstall a skill — removes the canonical home copy and any platform
 * symlinks (or copies) that point to it.
 *
 * ─── BEHAVIOUR ────────────────────────────────────────────────────────────────
 *   Without --platform: removes home AND all platform locations found on disk.
 *   With --platform:    removes only the named platform link (home kept).
 *   --all:              removes every installed skill in ~/.octocode/skills/.
 *   --dry-run:          preview without deleting anything.
 */

import fs from 'node:fs';
import path from 'node:path';
import { listSkills, getSkill } from '../registry.js';
import { getSkillsHome } from '../home.js';
import { getPlatformSkillsDir, parsePlatforms } from '../platforms.js';
import type { Platform } from '../platforms.js';
import { bold, dim, green, yellow, red, cyan } from '../utils/colors.js';
import { shortPath } from '../utils/paths.js';

export interface RemoveOptions {
  all: boolean;
  platform: string | null;
  dryRun: boolean;
  json: boolean;
}

type RemoveTarget = { location: string; path: string };
type RemoveResult = { target: string; path: string; status: 'removed' | 'skipped' | 'failed'; error?: string };

// ─── Helpers ──────────────────────────────────────────────────────────────────


function removeEntry(entryPath: string): { ok: boolean; error?: string } {
  try {
    const stat = fs.lstatSync(entryPath);
    if (stat.isDirectory() && !stat.isSymbolicLink()) {
      fs.rmSync(entryPath, { recursive: true, force: true });
    } else {
      fs.unlinkSync(entryPath);
    }
    return { ok: true };
  } catch (err) {
    return { ok: false, error: err instanceof Error ? err.message : String(err) };
  }
}

function existsOnDisk(p: string): boolean {
  try { fs.lstatSync(p); return true; } catch { return false; }
}

/** Find all platform locations where a skill is currently installed. */
function detectPlatformLocations(skillName: string): RemoveTarget[] {
  const all: Platform[] = ['pi', 'cursor', 'claude', 'claude-desktop', 'codex', 'opencode', 'copilot', 'gemini', 'common'];
  const found: RemoveTarget[] = [];
  const seen = new Set<string>();

  for (const platform of all) {
    const dir = getPlatformSkillsDir(platform);
    const p = path.join(dir, skillName);
    if (!seen.has(p) && existsOnDisk(p)) {
      seen.add(p);
      found.push({ location: platform, path: p });
    }
  }

  // Also check workspace
  const wsPath = path.join(process.cwd(), '.agents', 'skills', skillName);
  if (existsOnDisk(wsPath)) {
    found.push({ location: 'workspace', path: wsPath });
  }

  return found;
}

// ─── Main ─────────────────────────────────────────────────────────────────────

export function runRemove(skillNames: string[], opts: RemoveOptions): void {
  // ── Resolve skill list ────────────────────────────────────────────────────

  let names: string[];

  if (opts.all) {
    const home = getSkillsHome();
    try {
      names = fs.readdirSync(home, { withFileTypes: true })
        .filter((e) => e.isDirectory() || e.isSymbolicLink())
        .map((e) => e.name);
    } catch {
      names = listSkills().map((s) => s.folder);
    }
    if (names.length === 0) {
      const msg = 'No installed skills found in ~/.octocode/skills/';
      if (opts.json) console.log(JSON.stringify({ success: true, removed: 0, skills: [], message: msg }));
      else { console.log(); console.log(`  ${dim(msg)}`); console.log(); }
      return;
    }
  } else {
    if (skillNames.length === 0) {
      if (opts.json) {
        console.log(JSON.stringify({ success: false, error: 'Specify a skill name or use --all.' }));
      } else {
        console.log();
        console.log(`  ${red('✗')}  No skill specified.`);
        console.log();
        console.log(`  ${dim('Usage:')}  octocode-skills remove <name>`);
        console.log(`           octocode-skills remove --all`);
        console.log(`  ${dim('Browse:')} octocode-skills list`);
        console.log();
      }
      process.exitCode = 1;
      return;
    }

    // Validate all names exist as bundled skills (warn but don't block for
    // skills that may have been installed externally / manually).
    names = skillNames;
    const notBundled = skillNames.filter((n) => !getSkill(n));
    if (notBundled.length > 0 && !opts.json) {
      console.log();
      console.log(`  ${yellow('⚠')}  Not in bundled registry (removing anyway if found on disk): ${notBundled.join(', ')}`);
    }
  }

  // ── Resolve platforms filter ──────────────────────────────────────────────

  let platformFilter: Platform[] | null = null;
  if (opts.platform) {
    const parsed = parsePlatforms(opts.platform);
    if (parsed.error) {
      if (opts.json) console.log(JSON.stringify({ success: false, error: parsed.error }));
      else console.log(`\n  ${red('✗')}  ${parsed.error}\n`);
      process.exitCode = 1;
      return;
    }
    platformFilter = parsed.platforms;
  }

  // ── Header ────────────────────────────────────────────────────────────────

  if (!opts.json) {
    console.log();
    const header = opts.dryRun
      ? `${cyan('Dry-run preview')}${dim(' — no files deleted')}`
      : platformFilter
        ? dim(`removing platform links only (home kept)`)
        : dim(`removing home + all platform links`);
    console.log(`  ${bold('Removing')} ${dim(`${names.length} skill(s)  ·  ${header}`)}`);
    console.log();
  }

  // ── Remove per skill ──────────────────────────────────────────────────────

  type SkillRemoveRecord = { name: string; results: RemoveResult[]; nothingFound: boolean };
  const records: SkillRemoveRecord[] = [];
  let totalRemoved = 0, totalSkipped = 0, totalFailed = 0;

  for (const skillName of names) {
    const results: RemoveResult[] = [];

    if (platformFilter) {
      // Remove only the specified platform links
      for (const platform of platformFilter) {
        const p = path.join(getPlatformSkillsDir(platform), skillName);
        if (!existsOnDisk(p)) {
          results.push({ target: platform, path: p, status: 'skipped' });
        } else if (opts.dryRun) {
          results.push({ target: platform, path: p, status: 'removed' });
        } else {
          const { ok, error } = removeEntry(p);
          results.push({ target: platform, path: p, status: ok ? 'removed' : 'failed', ...(error ? { error } : {}) });
        }
      }
    } else {
      // Remove home first, then all detected platform locations
      const homePath = path.join(getSkillsHome(), skillName);
      if (existsOnDisk(homePath)) {
        if (opts.dryRun) {
          results.push({ target: 'home', path: homePath, status: 'removed' });
        } else {
          const { ok, error } = removeEntry(homePath);
          results.push({ target: 'home', path: homePath, status: ok ? 'removed' : 'failed', ...(error ? { error } : {}) });
        }
      }

      // Detect and remove platform/workspace locations
      const detected = detectPlatformLocations(skillName);
      for (const loc of detected) {
        if (opts.dryRun) {
          results.push({ target: loc.location, path: loc.path, status: 'removed' });
        } else {
          const { ok, error } = removeEntry(loc.path);
          results.push({ target: loc.location, path: loc.path, status: ok ? 'removed' : 'failed', ...(error ? { error } : {}) });
        }
      }
    }

    const nothingFound = results.length === 0;
    if (nothingFound) results.push({ target: 'home', path: path.join(getSkillsHome(), skillName), status: 'skipped' });

    for (const r of results) {
      if (r.status === 'removed') totalRemoved++;
      else if (r.status === 'skipped') totalSkipped++;
      else totalFailed++;
    }

    records.push({ name: skillName, results, nothingFound });
  }

  const success = totalFailed === 0;

  // ── JSON output ───────────────────────────────────────────────────────────

  if (opts.json) {
    console.log(JSON.stringify({
      success,
      dryRun: opts.dryRun,
      skills: records.map((r) => ({
        name: r.name,
        nothingFound: r.nothingFound,
        targets: r.results.map((t) => ({ target: t.target, path: t.path, status: t.status, ...(t.error ? { error: t.error } : {}) })),
      })),
      summary: { removed: totalRemoved, skipped: totalSkipped, failed: totalFailed },
    }));
    if (!success) process.exitCode = 1;
    return;
  }

  // ── Human output ──────────────────────────────────────────────────────────

  for (const record of records) {
    const anyRemoved = record.results.some((r) => r.status === 'removed');
    const anyFailed  = record.results.some((r) => r.status === 'failed');
    const icon = anyFailed ? red('✗') : anyRemoved ? green('✓') : yellow('~');

    console.log(`  ${icon}  ${bold(record.name)}${record.nothingFound ? `  ${dim('(not installed — nothing to remove)')}` : ''}`);

    if (!record.nothingFound) {
      for (const r of record.results) {
        const rIcon = r.status === 'removed' ? green('✓') : r.status === 'failed' ? red('✗') : yellow('~');
        const note = r.status === 'removed'
          ? (opts.dryRun ? dim('  (would remove)') : dim('  removed'))
          : r.status === 'skipped'
            ? dim('  (not installed)')
            : red(`  ${r.error ?? 'failed'}`);
        console.log(`     ${rIcon}  ${r.target.padEnd(14)} ${dim(shortPath(r.path))}${note}`);
      }
    }

    console.log();
  }

  // ── Summary ───────────────────────────────────────────────────────────────

  const parts = [
    totalRemoved > 0 ? green(`${totalRemoved} removed`) : null,
    totalSkipped > 0 ? yellow(`${totalSkipped} not installed`) : null,
    totalFailed  > 0 ? red(`${totalFailed} failed`)   : null,
  ].filter(Boolean).join('  ·  ');

  console.log(`  ${dim('─'.repeat(60))}`);
  console.log(`  ${parts}`);
  console.log();

  if (!success) process.exitCode = 1;
}
