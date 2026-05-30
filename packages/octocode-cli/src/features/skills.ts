import path from 'node:path';
import { dirExists, listSubdirectories, removeDirectory } from '../utils/fs.js';
import {
  getSkillsDirForTarget,
  installSkillToDestination,
  isSafeSkillName,
  normalizeSkillTarget,
  resolveModeForTarget,
  resolveSkillDestination,
  type SkillInstallResult,
  type SkillInstallStrategy,
  type SkillInstallTarget,
} from '../utils/skills.js';

export interface SkillTargetDestination {
  target: SkillInstallTarget;
  destDir: string;
}

export interface SkillInstallSummary {
  installed: number;
  skipped: number;
  failed: number;
  targetCount: number;
}

export interface SkillRemoveFailure {
  target: SkillInstallTarget;
  path?: string;
  reason: 'invalid-skill-name' | 'remove-failed';
}

export interface SkillRemoveSummary {
  removed: number;
  missing: number;
  failed: number;
  targetCount: number;
  failures: SkillRemoveFailure[];
}

export function parseSkillTargetList(rawTargets: string): {
  targets: SkillInstallTarget[];
  error?: string;
} {
  const parsed = rawTargets
    .split(',')
    .map(target => normalizeSkillTarget(target))
    .filter((target): target is SkillInstallTarget => target !== null);
  const targets = [...new Set(parsed)];

  if (targets.length === 0) {
    return { targets, error: 'No valid targets provided' };
  }

  return { targets };
}

export function getSkillTargetDestinations(
  targets: readonly SkillInstallTarget[],
  defaultDestDir: string
): SkillTargetDestination[] {
  return targets.map(target => ({
    target,
    destDir: getSkillsDirForTarget(target, defaultDestDir),
  }));
}

export function getAvailableSkillNames(sourceDir: string): string[] {
  return listSubdirectories(sourceDir).filter(isSafeSkillName);
}

function recordInstallResult(
  summary: SkillInstallSummary,
  result: SkillInstallResult
): void {
  if (result === 'installed') {
    summary.installed++;
  } else if (result === 'skipped') {
    summary.skipped++;
  } else {
    summary.failed++;
  }
}

export function installSkillForTargets({
  skillName,
  sourceDir,
  destinations,
  strategy,
  force,
}: {
  skillName: string;
  sourceDir: string;
  destinations: readonly SkillTargetDestination[];
  strategy: SkillInstallStrategy;
  force: boolean;
}): SkillInstallSummary {
  const sourcePath = path.join(sourceDir, skillName);
  const summary: SkillInstallSummary = {
    installed: 0,
    skipped: 0,
    failed: 0,
    targetCount: destinations.length,
  };

  for (const destination of destinations) {
    const destinationPath = resolveSkillDestination(
      destination.destDir,
      skillName
    );
    const result = destinationPath
      ? installSkillToDestination({
          sourcePath,
          destinationPath,
          mode: resolveModeForTarget(strategy, destination.target),
          force,
        })
      : 'failed';

    recordInstallResult(summary, result);
  }

  return summary;
}

export function installAllSkillsForTargets({
  skillNames,
  sourceDir,
  destinations,
  strategy,
  force,
}: {
  skillNames: readonly string[];
  sourceDir: string;
  destinations: readonly SkillTargetDestination[];
  strategy: SkillInstallStrategy;
  force: boolean;
}): SkillInstallSummary {
  const summary: SkillInstallSummary = {
    installed: 0,
    skipped: 0,
    failed: 0,
    targetCount: destinations.length,
  };

  for (const skillName of skillNames) {
    const result = installSkillForTargets({
      skillName,
      sourceDir,
      destinations,
      strategy,
      force,
    });
    summary.installed += result.installed;
    summary.skipped += result.skipped;
    summary.failed += result.failed;
  }

  return summary;
}

export function removeSkillFromTargets({
  skillName,
  destinations,
}: {
  skillName: string;
  destinations: readonly SkillTargetDestination[];
}): SkillRemoveSummary {
  const summary: SkillRemoveSummary = {
    removed: 0,
    missing: 0,
    failed: 0,
    targetCount: destinations.length,
    failures: [],
  };

  for (const destination of destinations) {
    const skillPath = resolveSkillDestination(destination.destDir, skillName);

    if (!skillPath) {
      summary.failed++;
      summary.failures.push({
        target: destination.target,
        reason: 'invalid-skill-name',
      });
      continue;
    }

    if (!dirExists(skillPath)) {
      summary.missing++;
      continue;
    }

    if (removeDirectory(skillPath)) {
      summary.removed++;
    } else {
      summary.failed++;
      summary.failures.push({
        target: destination.target,
        path: skillPath,
        reason: 'remove-failed',
      });
    }
  }

  return summary;
}
