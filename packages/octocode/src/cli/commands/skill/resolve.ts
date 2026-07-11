import type { MarketplaceSkill } from '../../../configs/skills-marketplace.js';
import {
  fetchMarketplaceSkills,
  readSkillFromGitHub,
} from '../../../utils/skills-fetch.js';
import { EXIT } from '../../exit-codes.js';
import {
  buildGitHubLibrarySource,
  buildGitHubSourceUrl,
  buildMarketplaceSkill,
  buildOctocodeSkillFolder,
  buildOctocodeSkillsSource,
} from './github-source.js';
import { KNOWN_OCTOCODE_SKILLS } from './types.js';
import type {
  GithubSkillFolder,
  SkillInstallRequest,
  SkillRequestResolution,
} from './types.js';

export function buildKnownOctocodeSkillRequests(
  branchOverride?: string
): SkillInstallRequest[] {
  return KNOWN_OCTOCODE_SKILLS.map(skillName =>
    buildOctocodeSkillFolder(skillName, branchOverride)
  )
    .map(ref => (ref ? buildMarketplaceSkill(ref) : null))
    .filter((skill): skill is MarketplaceSkill => skill !== null)
    .map(skill => ({
      skill,
      sourceUrl: buildGitHubSourceUrl(skill),
    }));
}

export async function resolveGitHubSkillRequests(
  ref: GithubSkillFolder,
  namedSkill: string | undefined
): Promise<SkillRequestResolution> {
  const skill = buildMarketplaceSkill(ref);
  if (!skill) {
    return {
      error: 'GitHub path does not resolve to a safe skill name',
      status: EXIT.USAGE,
    };
  }

  let readError: string | null = null;
  try {
    await readSkillFromGitHub(ref.owner, ref.repo, ref.skillPath, ref.branch);
  } catch (error) {
    readError = error instanceof Error ? error.message : String(error);
    if (namedSkill && readError.toLowerCase().includes('not found')) {
      readError = `Octocode skill not found: ${namedSkill} (${ref.url})`;
    }
  }

  if (!readError) {
    return {
      requests: [
        {
          skill,
          sourceUrl: ref.url,
        },
      ],
    };
  }

  if (namedSkill) {
    return { error: readError, status: EXIT.NOT_FOUND };
  }

  const librarySource = buildGitHubLibrarySource(ref);
  try {
    const librarySkills = await fetchMarketplaceSkills(librarySource, {
      skipCache: true,
    });
    if (librarySkills.length > 0) {
      return {
        requests: librarySkills
          .sort((a, b) => a.name.localeCompare(b.name))
          .map(librarySkill => ({
            skill: librarySkill,
            sourceUrl: buildGitHubSourceUrl(librarySkill),
          })),
      };
    }
  } catch {
    // Keep the original specific-skill error; it is the most helpful path hint.
  }

  return { error: readError, status: EXIT.NOT_FOUND };
}

export async function resolveOctocodeAllSkillRequests(
  branchOverride?: string
): Promise<SkillInstallRequest[]> {
  const source = buildOctocodeSkillsSource(branchOverride);
  try {
    const skills = await fetchMarketplaceSkills(source, { skipCache: true });
    if (skills.length > 0) {
      return skills
        .sort((a, b) => a.name.localeCompare(b.name))
        .map(skill => ({
          skill,
          sourceUrl: buildGitHubSourceUrl(skill),
        }));
    }
  } catch {
    // Fall through to the embedded names so offline/rate-limited installs still
    // have a deterministic official skill set to try.
  }

  return buildKnownOctocodeSkillRequests(branchOverride);
}
