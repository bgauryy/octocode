import path from 'node:path';
import type { SkillTargetDestination } from '../../../features/skills.js';
import { bold, c, dim } from '../../../utils/colors.js';
import { fileExists } from '../../../utils/fs.js';
import {
  formatSkillInstallTargets,
  installSkillToDestination,
  resolveModeForTarget,
  resolveSkillDestination,
  type SkillInstallResult,
  type SkillInstallStrategy,
} from '../../../utils/skills.js';
import { installMarketplaceSkill } from '../../../utils/skills-fetch.js';
import { Spinner } from '../../../utils/spinner.js';
import { EXIT } from '../../exit-codes.js';
import { getSkillSourcePath } from './bundled-source.js';
import {
  countResult,
  resultSummary,
  short,
  skillJsonResult,
} from './format.js';
import type { SkillCommandResult, SkillInstallRequest } from './types.js';

export type RunInstallParams = {
  requests: SkillInstallRequest[];
  destinations: SkillTargetDestination[];
  sourceRoot: string;
  force: boolean;
  mode: SkillInstallStrategy;
  platforms: string[];
  jsonOutput: boolean;
  verbose: boolean;
};

export async function runInstall(params: RunInstallParams): Promise<void> {
  const {
    requests,
    destinations,
    sourceRoot,
    force,
    mode,
    platforms,
    jsonOutput,
    verbose,
  } = params;

  // ── Fetch from GitHub ─────────────────────────────────────────────────────
  const spinner = jsonOutput
    ? null
    : new Spinner(`Installing ${requests.length} skill(s)...`).start();

  // ── Install per destination ───────────────────────────────────────────────
  const results: SkillCommandResult[] = [];
  const totals = { installed: 0, skipped: 0, failed: 0 };

  for (const request of requests) {
    const { skill } = request;
    const sourcePath = getSkillSourcePath(skill.name);
    spinner?.update(`Fetching ${skill.name}...`);

    const fetchResult = await installMarketplaceSkill(skill, sourceRoot);
    if (
      !fetchResult.success ||
      !fileExists(path.join(sourcePath, 'SKILL.md'))
    ) {
      const error =
        fetchResult.error ??
        `Fetched folder did not contain a SKILL.md file: ${skill.path}`;
      const failedTargets = destinations.map(dest => ({
        target: dest.target,
        destPath:
          resolveSkillDestination(dest.destDir, skill.name) ??
          path.join(dest.destDir, skill.name),
        result: 'failed' as SkillInstallResult,
      }));

      for (const failedTarget of failedTargets) {
        countResult(totals, failedTarget.result);
      }

      results.push({
        skill,
        source: request.sourceUrl,
        sourcePath,
        targets: failedTargets,
        installed: 0,
        skipped: 0,
        failed: failedTargets.length,
        error,
      });
      continue;
    }

    spinner?.update(`Linking ${skill.name}...`);
    const skillResult: SkillCommandResult = {
      skill,
      source: request.sourceUrl,
      sourcePath,
      targets: [],
      installed: 0,
      skipped: 0,
      failed: 0,
    };

    for (const dest of destinations) {
      const destinationPath = resolveSkillDestination(dest.destDir, skill.name);
      const effectiveMode = resolveModeForTarget(mode, dest.target);
      const result = destinationPath
        ? installSkillToDestination({
            sourcePath,
            destinationPath,
            mode: effectiveMode,
            force,
          })
        : 'failed';

      skillResult.targets.push({
        target: dest.target,
        destPath: destinationPath ?? path.join(dest.destDir, skill.name),
        result,
      });
      countResult(skillResult, result);
      countResult(totals, result);
    }

    results.push(skillResult);
  }

  if (totals.failed > 0) {
    spinner?.fail(`Installed ${requests.length} skill(s) with errors`);
    process.exitCode = EXIT.GENERAL;
  } else if (totals.skipped > 0 && totals.installed === 0) {
    spinner?.warn?.(`${requests.length} skill(s) already installed`);
  } else {
    spinner?.succeed(`Installed ${requests.length} skill(s)`);
  }

  // ── JSON output ───────────────────────────────────────────────────────────
  if (jsonOutput) {
    console.log(
      JSON.stringify({
        success: totals.failed === 0,
        skills: results.map(skillJsonResult),
        platforms,
        mode,
        summary: resultSummary(totals),
      })
    );
    return;
  }

  // ── Human output ──────────────────────────────────────────────────────────
  console.log();
  console.log(`  ${dim('Mode:')} ${mode}`);
  console.log(`  ${dim('Platforms:')} ${platforms.join(', ')}`);
  console.log();
  for (const result of results) {
    console.log(`  ${bold(result.skill.displayName)}`);
    console.log(`  ${dim('Source:')} ${short(result.sourcePath)}`);
    if (verbose) {
      console.log(`  ${dim(result.source)}`);
    }
    if (result.error) {
      console.log(`  ${c('red', '✗')}  ${dim(result.error)}`);
    }
    for (const r of result.targets) {
      if (r.result === 'installed') {
        console.log(
          `  ${c('green', '✓')}  ${r.target.padEnd(14)} ${short(r.destPath)}`
        );
      } else if (r.result === 'skipped') {
        console.log(
          `  ${c('yellow', '~')}  ${r.target.padEnd(14)} ${short(r.destPath)}  ${dim('(already installed — use --force to overwrite)')}`
        );
      } else {
        console.log(
          `  ${c('red', '✗')}  ${r.target.padEnd(14)} ${short(r.destPath)}  ${dim('(valid targets: ' + formatSkillInstallTargets() + ')')}`
        );
      }
    }
    console.log();
  }
  console.log(
    `  ${dim('Summary:')} ${totals.installed} installed, ${totals.skipped} skipped, ${totals.failed} failed`
  );
  console.log();
}
