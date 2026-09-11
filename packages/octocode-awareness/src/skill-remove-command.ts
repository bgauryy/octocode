import { lstatSync, rmSync, unlinkSync } from 'node:fs';
import {
  AWARENESS_SKILL_NAME,
  canonicalSkillPath,
  failSkillCommand,
  isSkillCommandFailure,
  pathExists,
  scopedSkillSelection,
  selectedDestination,
  skillFlag,
  skillOption,
  type SkillCommandArguments,
  type SkillCommandOptions,
  type SkillCommandResult,
} from './skill-command-utils.js';

type RemovalStatus = 'would-remove' | 'removed' | 'missing' | 'failed';

function removePath(path: string): string | undefined {
  try {
    const stat = lstatSync(path);
    if (stat.isDirectory() && !stat.isSymbolicLink())
      rmSync(path, { recursive: true, force: true });
    else unlinkSync(path);
    return undefined;
  } catch (error) {
    return error instanceof Error ? error.message : String(error);
  }
}

function removalOutcome(
  path: string,
  dryRun: boolean
): { status: RemovalStatus; error?: string } {
  if (!pathExists(path)) return { status: 'missing' };
  if (dryRun) return { status: 'would-remove' };
  const error = removePath(path);
  return error ? { status: 'failed', error } : { status: 'removed' };
}

export function runSkillRemove(
  argv: SkillCommandArguments,
  options: SkillCommandOptions
): SkillCommandResult {
  const canonicalSelected = skillFlag(argv, '--canonical');
  const platformValue = skillOption(argv, '--platform');
  if (!canonicalSelected && !platformValue)
    return failSkillCommand('choose exactly one of --platform or --canonical');
  if (canonicalSelected && platformValue)
    return failSkillCommand('use either --canonical or --platform, not both');

  const confirm = skillFlag(argv, '--confirm');
  const explicitDryRun = skillFlag(argv, '--dry-run');
  if (confirm && explicitDryRun)
    return failSkillCommand('use either --confirm or --dry-run, not both');
  const dryRun = !confirm;

  const targets: Array<Record<string, unknown>> = [];
  if (canonicalSelected) {
    if (
      skillFlag(argv, '--global') ||
      skillOption(argv, '--project-dir') !== undefined
    ) {
      return failSkillCommand(
        '--canonical does not accept --global or --project-dir'
      );
    }
    const path = canonicalSkillPath(options);
    targets.push({
      target: 'canonical',
      path,
      ...removalOutcome(path, dryRun),
    });
  } else {
    const selection = scopedSkillSelection(argv, options);
    if (isSkillCommandFailure(selection)) return selection;
    for (const platform of selection.platforms) {
      const path = selectedDestination(selection, platform);
      targets.push({
        target: platform,
        platform,
        scope: selection.scope,
        path,
        ...removalOutcome(path, dryRun),
      });
    }
  }

  const summary = {
    wouldRemove: targets.filter(target => target.status === 'would-remove')
      .length,
    removed: targets.filter(target => target.status === 'removed').length,
    missing: targets.filter(target => target.status === 'missing').length,
    failed: targets.filter(target => target.status === 'failed').length,
  };
  const ok = summary.failed === 0;
  const canonicalRemoved =
    canonicalSelected && targets.some(target => target.status === 'removed');
  return {
    exitCode: ok ? 0 : 1,
    payload: {
      ok,
      action: dryRun ? 'dry-run' : 'remove',
      skill: AWARENESS_SKILL_NAME,
      dryRun,
      confirm,
      canonicalSelected,
      canonicalRemoved,
      preservesCanonical: !canonicalSelected,
      preservesPlatformLinks: canonicalSelected,
      targets,
      summary,
      ...(dryRun
        ? { hint: 'preview only; repeat with --confirm to remove these exact paths' }
        : {}),
    },
  };
}
