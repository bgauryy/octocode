/**
 * Env param registry — which environment variables each skill needs.
 *
 * Semantics:
 *   required     — skill is fully broken without this
 *   recommended  — skill degrades significantly; works with reduced capability
 *   optional     — nice to have, graceful without it
 *
 * group semantics — when group is set, AT LEAST ONE env var in the group must
 * be present to satisfy the group requirement. The group itself carries the
 * overall `required` level.
 */

// ─── Types ────────────────────────────────────────────────────────────────────

export type EnvRequirement = 'required' | 'recommended' | 'optional';

export interface EnvParam {
  /** Environment variable name, e.g. "TAVILY_API_KEY" */
  key: string;
  /** Short human-readable description */
  description: string;
  /** Importance level */
  required: EnvRequirement;
  /**
   * Group tag — when set, AT LEAST ONE var in the group must be present.
   * e.g. "web-search" means tavily OR serper OR exa is enough.
   */
  group?: string;
  /** Where to get the key */
  link?: string;
}

export type EnvStatus = 'set' | 'missing';

export interface EnvParamStatus {
  param: EnvParam;
  status: EnvStatus;
}

export interface SkillEnvStatus {
  skillName: string;
  params: EnvParamStatus[];
  /**
   * Overall readiness:
   *   ready         — all required groups satisfied, all standalone required params set
   *   needs-config  — one or more required params/groups missing
   *   partial       — required ok, but recommended params missing
   *   ok            — no env params needed at all
   */
  readiness: 'ok' | 'ready' | 'partial' | 'needs-config';
}

// ─── Static registry ──────────────────────────────────────────────────────────

const WEB_SEARCH_PARAMS: EnvParam[] = [
  {
    key: 'TAVILY_API_KEY',
    description: 'Tavily — curated/deep web research',
    required: 'recommended',
    group: 'web-search',
    link: 'https://app.tavily.com/',
  },
  {
    key: 'SERPER_API_KEY',
    description: 'Serper — broad Google SERP results',
    required: 'recommended',
    group: 'web-search',
    link: 'https://serper.dev/',
  },
  {
    key: 'EXA_API_KEY',
    description: 'Exa — neural/AI-native search, category filters',
    required: 'recommended',
    group: 'web-search',
    link: 'https://dashboard.exa.ai/',
  },
];

const GITHUB_TOKEN_PARAMS: EnvParam[] = [
  {
    key: 'GH_TOKEN',
    description: 'GitHub token — code search, file reads, repo/PR discovery',
    required: 'recommended',
    group: 'github-token',
    link: 'https://github.com/settings/tokens',
  },
  {
    key: 'GITHUB_TOKEN',
    description: 'GitHub token (alternate name — either GH_TOKEN or GITHUB_TOKEN)',
    required: 'recommended',
    group: 'github-token',
    link: 'https://github.com/settings/tokens',
  },
];

/**
 * Canonical env param requirements per skill.
 * Skills not listed here need no env params.
 */
export const SKILL_ENV_PARAMS: Record<string, EnvParam[]> = {
  'octocode-brainstorming': WEB_SEARCH_PARAMS,
  'octocode-research': GITHUB_TOKEN_PARAMS,
  'octocode-rfc-generator': GITHUB_TOKEN_PARAMS,
  'octocode-roast': GITHUB_TOKEN_PARAMS,
  // awareness, eval, prompt-optimizer, skills, subagent: no special env params
};

// ─── Runtime status check ─────────────────────────────────────────────────────

/** Check whether a single env var is set in the current process env. */
export function isEnvSet(key: string): boolean {
  const val = process.env[key];
  return typeof val === 'string' && val.trim().length > 0;
}

/** Get env status for all params of a skill. */
export function getSkillEnvStatus(skillName: string): SkillEnvStatus {
  const params = SKILL_ENV_PARAMS[skillName] ?? [];

  if (params.length === 0) {
    return { skillName, params: [], readiness: 'ok' };
  }

  const paramStatuses: EnvParamStatus[] = params.map((p) => ({
    param: p,
    status: isEnvSet(p.key) ? 'set' : 'missing',
  }));

  // Evaluate group satisfication
  const groups = new Map<string, { level: EnvRequirement; anySatisfied: boolean }>();
  const standaloneUnsatisfied: EnvRequirement[] = [];

  for (const ps of paramStatuses) {
    const { group, required } = ps.param;
    if (group) {
      const existing = groups.get(group);
      if (existing) {
        if (ps.status === 'set') existing.anySatisfied = true;
      } else {
        groups.set(group, { level: required, anySatisfied: ps.status === 'set' });
      }
    } else {
      if (ps.status === 'missing') standaloneUnsatisfied.push(required);
    }
  }

  // Determine readiness
  let hasRequiredMissing = false;
  let hasRecommendedMissing = false;

  for (const [, g] of groups) {
    if (!g.anySatisfied) {
      if (g.level === 'required') hasRequiredMissing = true;
      else if (g.level === 'recommended') hasRecommendedMissing = true;
    }
  }
  for (const level of standaloneUnsatisfied) {
    if (level === 'required') hasRequiredMissing = true;
    else if (level === 'recommended') hasRecommendedMissing = true;
  }

  const readiness = hasRequiredMissing
    ? 'needs-config'
    : hasRecommendedMissing
      ? 'partial'
      : 'ready';

  return { skillName, params: paramStatuses, readiness };
}

/** Get env status for a list of skills. */
export function getSkillsEnvStatus(skillNames: string[]): SkillEnvStatus[] {
  return skillNames.map(getSkillEnvStatus);
}

// ─── Display helpers ──────────────────────────────────────────────────────────

/** Human-readable group label, e.g. "web-search" → "web search (at least one)" */
export function groupLabel(group: string): string {
  const labels: Record<string, string> = {
    'web-search': 'web search (at least one of three)',
    'github-token': 'GitHub token (GH_TOKEN or GITHUB_TOKEN)',
  };
  return labels[group] ?? group;
}

/** True when the group that contains this param is satisfied by ANY other set param in the list. */
export function isGroupSatisfied(ps: EnvParamStatus, all: EnvParamStatus[]): boolean {
  const { group } = ps.param;
  if (!group) return ps.status === 'set';
  return all.some((other) => other.param.group === group && other.status === 'set');
}

/** Compact summary of what's missing, for inline display. */
export function missingHint(envStatus: SkillEnvStatus): string {
  if (envStatus.readiness === 'ok' || envStatus.readiness === 'ready') return '';

  const unsatisfiedGroups = new Set<string>();
  const standaloneKeys: string[] = [];

  for (const ps of envStatus.params) {
    if (ps.status === 'set') continue;
    if (ps.param.group) {
      const groupSatisfied = isGroupSatisfied(ps, envStatus.params);
      if (!groupSatisfied) unsatisfiedGroups.add(ps.param.group);
    } else {
      standaloneKeys.push(ps.param.key);
    }
  }

  const parts: string[] = [
    ...[...unsatisfiedGroups].map(groupLabel),
    ...standaloneKeys,
  ];

  if (parts.length === 0) return '';
  const verb = envStatus.readiness === 'needs-config' ? 'missing' : 'recommended';
  return `${verb}: ${parts.join(', ')}`;
}
