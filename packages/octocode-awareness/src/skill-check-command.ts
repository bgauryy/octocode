import {
  AWARENESS_SKILL_NAME,
  failSkillCommand,
  inspectCanonicalSkill,
  inspectSkillDestination,
  isSkillCommandFailure,
  scopedSkillSelection,
  selectedDestination,
  skillFlag,
  skillOption,
  type SkillCommandArguments,
  type SkillCommandOptions,
  type SkillCommandResult,
} from './skill-command-utils.js';

export function runSkillCheck(
  argv: SkillCommandArguments,
  options: SkillCommandOptions
): SkillCommandResult {
  const platformValue = skillOption(argv, '--platform');
  if (
    !platformValue &&
    (skillFlag(argv, '--global') || skillOption(argv, '--project-dir'))
  ) {
    return failSkillCommand(
      '--platform is required when selecting --global or --project-dir'
    );
  }

  const canonical = inspectCanonicalSkill(options);
  const destinations: Array<Record<string, unknown>> = [];
  if (platformValue) {
    const selection = scopedSkillSelection(argv, options);
    if (isSkillCommandFailure(selection)) return selection;
    for (const platform of selection.platforms) {
      const destination = selectedDestination(selection, platform);
      destinations.push({
        platform,
        scope: selection.scope,
        path: destination,
        status: inspectSkillDestination(destination, canonical.canonical),
      });
    }
  }

  const statuses = [canonical.status, ...destinations.map(item => item.status)];
  const summary = {
    healthy: statuses.filter(
      status =>
        status === 'installed' || status === 'linked' || status === 'copied'
    ).length,
    missing: statuses.filter(status => status === 'missing').length,
    broken: statuses.filter(status => status === 'broken').length,
    drifted: statuses.filter(status => status === 'drifted').length,
    outdated: statuses.filter(status => status === 'outdated').length,
    invalid: statuses.filter(status => status === 'invalid').length,
    total: statuses.length,
  };
  const ok =
    canonical.status === 'installed' &&
    destinations.every(item => item.status === 'linked' || item.status === 'copied');

  return {
    exitCode: ok ? 0 : 1,
    payload: {
      ok,
      action: 'check',
      skill: {
        name: AWARENESS_SKILL_NAME,
        source: canonical.source,
        canonical: canonical.canonical,
        canonicalStatus: canonical.status,
        destinations,
      },
      summary,
      ...(canonical.status === 'outdated'
        ? { hint: 'run skill install with --upgrade using the same platform and scope' }
        : {}),
    },
  };
}
