import { existsSync } from 'node:fs';
import path from 'node:path';
import type { SkillTargetDestination } from '../../../features/skills.js';
import { bold, c, dim } from '../../../utils/colors.js';
import { getSkillSourcePath } from './bundled-source.js';
import { short } from './format.js';
import type { SkillInstallRequest } from './types.js';

export type DryRunParams = {
  requests: SkillInstallRequest[];
  destinations: SkillTargetDestination[];
  force: boolean;
  jsonOutput: boolean;
  mode: string;
  platforms: string[];
  verbose: boolean;
};

export function runDryRun(params: DryRunParams): void {
  const {
    requests,
    destinations,
    force,
    jsonOutput,
    mode,
    platforms,
    verbose,
  } = params;

  const dryRunSkills = requests.map(request => ({
    name: request.skill.name,
    displayName: request.skill.displayName,
    source: request.sourceUrl,
    sourcePath: getSkillSourcePath(request.skill.name),
    targets: destinations.map(d => {
      const destPath = path.join(d.destDir, request.skill.name);
      const exists = existsSync(destPath);
      return {
        target: d.target,
        path: destPath,
        action: exists ? (force ? 'overwrite' : 'skip') : 'install',
      };
    }),
  }));

  if (jsonOutput) {
    console.log(
      JSON.stringify({
        dryRun: true,
        skills: dryRunSkills,
        mode,
        platforms,
      })
    );
    return;
  }

  console.log();
  console.log(`  ${c('cyan', 'DRY RUN')} ${dim('— nothing will change')}`);
  console.log();
  console.log(`  Mode:   ${mode}`);
  console.log(`  Skills: ${dryRunSkills.length}`);
  for (const skillPreview of dryRunSkills) {
    console.log();
    console.log(`  ${bold(skillPreview.displayName)}`);
    console.log(`  ${dim('Source:')} ${short(skillPreview.sourcePath)}`);
    if (verbose) {
      console.log(`  ${dim(skillPreview.source)}`);
    }
    for (const target of skillPreview.targets) {
      if (target.action === 'install') {
        console.log(
          `  ${c('green', '+')}  ${target.target.padEnd(14)} ${short(target.path)}`
        );
      } else if (target.action === 'overwrite') {
        console.log(
          `  ${c('yellow', '↺')}  ${target.target.padEnd(14)} ${short(target.path)}  ${dim('(overwrite)')}`
        );
      } else {
        console.log(
          `  ${c('yellow', '~')}  ${target.target.padEnd(14)} ${short(target.path)}  ${dim('(already installed — add --force to overwrite)')}`
        );
      }
    }
  }
  console.log();
}
