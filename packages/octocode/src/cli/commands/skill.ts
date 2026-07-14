import { join } from 'node:path';

import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';

import type { CLICommand, ParsedArgs } from '../types.js';
import { EXIT } from '../exit-codes.js';
import { getBool, getString } from '../options.js';
import { c, bold, dim } from '../../utils/colors.js';
import {
  NAMED_SKILL_OWNER,
  NAMED_SKILL_REPO,
  NAMED_SKILL_BASE_PATH,
  type InstallMode,
  type SkillResult,
  type SkillInstallEntry,
  canonicalSkillsRoot,
  platformDirs,
  runList,
  fetchAndInstallSkill,
  printSkillResult,
  parseAddPath,
  skillNameFromPath,
} from './skill-helpers.js';

// ─── Command ──────────────────────────────────────────────────────────────────

export const skillCommand: CLICommand = {
  name: 'skill',
  options: [
    { name: 'add', hasValue: true },
    { name: 'name', hasValue: true },
    { name: 'list' },
    { name: 'platform', hasValue: true },
    { name: 'target', hasValue: true }, // alias of --platform
    { name: 'all' }, // shorthand for --platform all
    { name: 'mode', hasValue: true, default: 'symlink' },
    { name: 'force' },
    { name: 'update' }, // alias of --force
    { name: 'dry-run' },
    { name: 'verbose' },
    { name: 'branch', hasValue: true },
    { name: 'json' },
    { name: 'install-all' },
    { name: 'all-skills' }, // alias of --install-all
  ],
  handler: async (args: ParsedArgs) => {
    const addPath = getString(args.options, 'add');
    const skillName = getString(args.options, 'name');
    const list = getBool(args.options, 'list');
    const rawPlatform =
      getString(args.options, 'platform') ||
      getString(args.options, 'target') ||
      (getBool(args.options, 'all') ? 'all' : 'common');
    const mode = (getString(args.options, 'mode') || 'symlink') as InstallMode;
    const force =
      getBool(args.options, 'force') || getBool(args.options, 'update');
    const dryRun = getBool(args.options, 'dry-run');
    const verbose = getBool(args.options, 'verbose');
    const branch = getString(args.options, 'branch');
    const jsonOutput = getBool(args.options, 'json');
    const installAll =
      getBool(args.options, 'install-all') ||
      getBool(args.options, 'all-skills');

    // ── list ──────────────────────────────────────────────────────────────
    if (list) {
      const code = await runList(jsonOutput);
      if (code !== EXIT.OK) process.exitCode = code;
      return;
    }

    // ── validate mode ──────────────────────────────────────────────────────
    if (mode !== 'symlink' && mode !== 'copy' && mode !== 'hybrid') {
      const msg = `Invalid --mode "${mode}". Must be: symlink, copy, or hybrid.`;
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: msg }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${msg}\n`);
      }
      process.exitCode = EXIT.USAGE;
      return;
    }

    // ── collect skill jobs ─────────────────────────────────────────────────
    type SkillJob = {
      name: string;
      owner: string;
      repo: string;
      path: string;
      branch?: string;
    };

    const jobs: SkillJob[] = [];

    if (addPath) {
      const ref = parseAddPath(addPath, branch ?? undefined);
      if (!ref) {
        const msg = `Cannot parse GitHub path: "${addPath}". Use owner/repo/path or a github.com URL.`;
        if (jsonOutput) {
          console.log(JSON.stringify({ success: false, error: msg }));
        } else {
          console.error(`\n  ${c('red', '✗')} ${msg}\n`);
        }
        process.exitCode = EXIT.USAGE;
        return;
      }
      // If path is a library (no SKILL.md), we'd need to enumerate — for now
      // install as a single skill folder.
      jobs.push({
        name: skillNameFromPath(ref.path),
        owner: ref.owner,
        repo: ref.repo,
        path: ref.path,
        branch: ref.branch,
      });
    } else if (skillName) {
      jobs.push({
        name: skillName,
        owner: NAMED_SKILL_OWNER,
        repo: NAMED_SKILL_REPO,
        path: `${NAMED_SKILL_BASE_PATH}/${skillName}`,
        branch: branch ?? undefined,
      });
    } else if (installAll) {
      // Fetch skills list from GitHub, then install each
      try {
        const listResult = await executeDirectTool('ghViewRepoStructure', {
          queries: [
            {
              owner: NAMED_SKILL_OWNER,
              repo: NAMED_SKILL_REPO,
              path: NAMED_SKILL_BASE_PATH,
              maxDepth: 1,
              directoriesOnly: true,
              reasoning: 'List all official Octocode skills to install',
            },
          ],
        });
        const sc = listResult.structuredContent as {
          results?: Array<{ data?: { folders?: string[] } }>;
        };
        const allSkills = sc?.results?.[0]?.data?.folders ?? [];
        for (const s of allSkills) {
          jobs.push({
            name: s,
            owner: NAMED_SKILL_OWNER,
            repo: NAMED_SKILL_REPO,
            path: `${NAMED_SKILL_BASE_PATH}/${s}`,
            branch: branch ?? undefined,
          });
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        if (jsonOutput) {
          console.log(JSON.stringify({ success: false, error: message }));
        } else {
          console.error(
            `\n  ${c('red', '✗')} Failed to fetch skill list: ${message}\n`
          );
        }
        process.exitCode = EXIT.GENERAL;
        return;
      }
    } else {
      const msg =
        'Provide one of: --add <github-path>, --name <skill-name>, --install-all, or --list.';
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: msg }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${msg}\n`);
        console.error(
          `  ${dim('Examples:')}\n` +
            `    skill --list\n` +
            `    skill --name octocode-research\n` +
            `    skill --name octocode-research --platform all\n` +
            `    skill --add owner/repo/skills/my-skill\n`
        );
      }
      process.exitCode = EXIT.USAGE;
      return;
    }

    if (jobs.length === 0) {
      const msg = 'No skills to install.';
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: msg }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${msg}\n`);
      }
      process.exitCode = EXIT.GENERAL;
      return;
    }

    // ── dry-run header ─────────────────────────────────────────────────────
    if (dryRun && !jsonOutput) {
      console.log(
        `\n  ${c('yellow', bold('Dry run'))} — no files will be written\n`
      );
    }

    // ── install each skill ─────────────────────────────────────────────────
    const skillsRoot = canonicalSkillsRoot();
    const allResults: SkillResult[] = [];
    let anyError = false;

    for (const job of jobs) {
      const canonicalDir = join(skillsRoot, job.name);
      const destinations = platformDirs(rawPlatform, job.name);

      const entry: SkillInstallEntry = {
        name: job.name,
        canonicalDir,
        destinations,
        mode,
        force,
        dryRun,
        verbose,
      };

      const r = await fetchAndInstallSkill(
        entry,
        job.owner,
        job.repo,
        job.path,
        job.branch
      );

      allResults.push(r);
      printSkillResult(r, verbose, jsonOutput);

      if (r.destinations.some(d => !d.success)) anyError = true;
    }

    // ── summary ────────────────────────────────────────────────────────────
    const successCount = allResults.filter(
      r => !r.destinations.some(d => !d.success)
    ).length;
    const summary = {
      total: allResults.length,
      succeeded: successCount,
      failed: allResults.length - successCount,
    };

    if (jsonOutput) {
      console.log(
        JSON.stringify(
          {
            skills: allResults.map(r => ({
              name: r.name,
              canonicalDir: r.canonicalDir,
              mode: r.mode,
              platforms: r.destinations.map(d => ({
                platform: d.platform,
                dir: d.dir,
                action: d.action,
                success: d.success,
                ...(d.error ? { error: d.error } : {}),
              })),
            })),
            platforms: rawPlatform,
            mode,
            summary,
          },
          null,
          2
        )
      );
    } else {
      if (!dryRun) {
        const msg =
          summary.failed === 0
            ? c('green', `${summary.succeeded} skill(s) installed`)
            : c(
                'red',
                `${summary.succeeded}/${summary.total} skill(s) installed, ${summary.failed} failed`
              );
        console.log(`\n  ${msg}\n`);
      }
    }

    if (anyError) {
      process.exitCode = EXIT.GENERAL;
    }
  },
};
