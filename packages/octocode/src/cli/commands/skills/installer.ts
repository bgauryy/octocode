import {
  installBundledSkills,
  type SkillInstallMode,
} from '@octocodeai/octocode-skill-installer';
import type { Platform } from './platforms.js';

export type InstallMode = SkillInstallMode;
export type LinkStatus = 'linked' | 'skipped' | 'failed';

export interface LinkResult {
  target: string;
  destPath: string;
  status: LinkStatus;
  error?: string;
}

export interface SkillInstallOutcome {
  skillName: string;
  homePath: string | null;
  homeStatus: 'installed' | 'skipped' | 'failed' | 'bypassed';
  homeError?: string;
  links: LinkResult[];
}

export interface InstallSkillParams {
  sourcePath: string;
  skillName: string;
  platforms: Platform[];
  workspace: boolean;
  customPath: string | null;
  mode: InstallMode;
  force: boolean;
  dryRun: boolean;
  /** Test/integration override for global platform roots. */
  homeDir?: string;
  /** Test/integration override for the durable canonical store. */
  canonicalSkillsDir?: string;
  /** Project root used by the legacy workspace adapter. */
  projectDir?: string;
}

export function installSkill(params: InstallSkillParams): SkillInstallOutcome {
  const customPath = params.customPath || null;
  const result = installBundledSkills({
    skills: [{ name: params.skillName, sourcePath: params.sourcePath }],
    canonicalSkillsDir: customPath ?? params.canonicalSkillsDir,
    targets: customPath
      ? []
      : [
          ...params.platforms.map(platform => ({
            platform,
            scope: 'global' as const,
            homeDir: params.homeDir,
          })),
          ...(params.workspace
            ? [
                {
                  platform: 'codex' as const,
                  scope: 'project' as const,
                  projectDir: params.projectDir ?? process.cwd(),
                },
              ]
            : []),
        ],
    mode: params.mode,
    force: params.force,
    dryRun: params.dryRun,
  });
  const outcome = result.skills[0]!;
  const homeStatus = customPath
    ? 'bypassed'
    : outcome.canonicalStatus === 'installed'
      ? 'installed'
      : outcome.canonicalStatus === 'unchanged'
        ? 'skipped'
        : 'failed';
  return {
    skillName: outcome.name,
    homePath: customPath ? null : outcome.canonical,
    homeStatus,
    ...(outcome.canonicalError ? { homeError: outcome.canonicalError } : {}),
    links: customPath
      ? [
          {
            target: 'custom',
            destPath: outcome.canonical,
            status:
              outcome.canonicalStatus === 'installed'
                ? 'linked'
                : outcome.canonicalStatus === 'unchanged'
                  ? 'skipped'
                  : 'failed',
            ...(outcome.canonicalError
              ? { error: outcome.canonicalError }
              : {}),
          },
        ]
      : outcome.destinations.map(destination => ({
          target:
            destination.scope === 'project'
              ? 'workspace'
              : destination.platform,
          destPath: destination.destination,
          status:
            destination.status === 'linked' || destination.status === 'copied'
              ? 'linked'
              : destination.status === 'unchanged'
                ? 'skipped'
                : 'failed',
          ...(destination.error ? { error: destination.error } : {}),
        })),
  };
}
