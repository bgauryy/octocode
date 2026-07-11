import { c, dim } from '../../../utils/colors.js';
import { HOME } from '../../../utils/platform.js';
import type {
  SkillInstallResult,
  SkillInstallTarget,
} from '../../../utils/skills.js';
import { EXIT } from '../../exit-codes.js';
import type { DestinationInstallResult, SkillCommandResult } from './types.js';

export function countResult(
  results: { installed: number; skipped: number; failed: number },
  result: SkillInstallResult
): void {
  if (result === 'installed') results.installed++;
  else if (result === 'skipped') results.skipped++;
  else results.failed++;
}

export function resultSummary(result: {
  installed: number;
  skipped: number;
  failed: number;
}): { installed: number; skipped: number; failed: number } {
  return {
    installed: result.installed,
    skipped: result.skipped,
    failed: result.failed,
  };
}

export function targetResults(targets: DestinationInstallResult[]): {
  target: SkillInstallTarget;
  path: string;
  result: SkillInstallResult;
}[] {
  return targets.map(target => ({
    target: target.target,
    path: target.destPath,
    result: target.result,
  }));
}

export function skillJsonResult(result: SkillCommandResult): {
  name: string;
  displayName: string;
  source: string;
  sourcePath: string;
  error?: string;
  targets: ReturnType<typeof targetResults>;
  summary: ReturnType<typeof resultSummary>;
} {
  return {
    name: result.skill.name,
    displayName: result.skill.displayName,
    source: result.source,
    sourcePath: result.sourcePath,
    error: result.error,
    targets: targetResults(result.targets),
    summary: resultSummary(result),
  };
}

export function printUsageError(message: string, jsonOutput: boolean): void {
  if (jsonOutput) {
    console.log(JSON.stringify({ success: false, error: message }));
  } else {
    console.log();
    console.log(`  ${c('red', '✗')} ${message}`);
    console.log(
      `  ${dim('Usage:')} skill (--add <github-path> | --add --path <local-skill-or-skills-dir> | --name <octocode-skill> | --install-all) [--platform common|cursor|claude|codex|opencode|pi|copilot|gemini|all] [--mode symlink|copy|hybrid] [--force|--update] [--dry-run]`
    );
    console.log(`  ${dim('List:  ')} skill --list`);
    console.log(`  ${dim('Example:')} skill --name octocode-research`);
    console.log(`  ${dim('Example:')} skill --install-all --platform common`);
    console.log(
      `  ${dim('Example:')} skill --add owner/repo/skills --platform cursor`
    );
    console.log(
      `  ${dim('Example:')} skill --add --path /path/to/skills --platform common`
    );
    console.log();
  }
  process.exitCode = EXIT.USAGE;
}

export function short(p: string): string {
  return p.replace(HOME, '~');
}
