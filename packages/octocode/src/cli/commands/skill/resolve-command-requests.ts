import { Spinner } from '../../../utils/spinner.js';
import { c } from '../../../utils/colors.js';
import {
  resolveAllBundledSkillRequests,
  tryResolveBundledSkillRequest,
} from './bundled-source.js';
import { printUsageError } from './format.js';
import {
  buildMarketplaceSkill,
  buildOctocodeSkillFolder,
  parseGitHubSkillFolder,
} from './github-source.js';
import { resolveLocalSkillRequests } from './local-source.js';
import {
  resolveGitHubSkillRequests,
  resolveOctocodeAllSkillRequests,
} from './resolve.js';
import type { SkillInstallRequest } from './types.js';

export type ResolveCommandRequestsParams = {
  installAll: boolean;
  namedSkill: string | undefined;
  localSkillPath: string | undefined;
  githubFolder: string | undefined;
  branchOverride: string | undefined;
  jsonOutput: boolean;
};

/**
 * Resolves the skill install requests for the `skill` command based on which
 * source flag was used (--install-all, --name, --path, --add). On failure,
 * prints the appropriate error (JSON or human) and sets process.exitCode,
 * returning null so the caller can stop early.
 */
export async function resolveCommandRequests(
  params: ResolveCommandRequestsParams
): Promise<SkillInstallRequest[] | null> {
  const {
    installAll,
    namedSkill,
    localSkillPath,
    githubFolder,
    branchOverride,
    jsonOutput,
  } = params;

  let requests: SkillInstallRequest[] = [];
  if (installAll) {
    // Prefer bundled skills; fall back to GitHub fetch if bundle is unavailable.
    const bundledRequests = resolveAllBundledSkillRequests();
    if (bundledRequests.length > 0) {
      requests = bundledRequests;
    } else {
      const spinner = jsonOutput
        ? null
        : new Spinner('Fetching Octocode skills list...').start();
      requests = await resolveOctocodeAllSkillRequests(branchOverride);
      spinner?.stop();
    }
  } else if (namedSkill) {
    // For official Octocode skills: prefer bundled path (offline, correct version).
    const bundledRequest = tryResolveBundledSkillRequest(namedSkill);
    if (bundledRequest) {
      requests = [bundledRequest];
    } else {
      const ref = buildOctocodeSkillFolder(namedSkill, branchOverride);
      if (!ref) {
        printUsageError('Invalid Octocode skill name', jsonOutput);
        return null;
      }

      const spinner = jsonOutput
        ? null
        : new Spinner(`Resolving ${namedSkill}...`).start();
      const resolved = await resolveGitHubSkillRequests(ref, namedSkill);
      spinner?.stop();
      if ('error' in resolved) {
        if (jsonOutput) {
          const skill = buildMarketplaceSkill(ref);
          console.log(
            JSON.stringify({
              success: false,
              skill: skill?.name,
              source: ref.url,
              error: resolved.error,
            })
          );
        } else {
          console.log();
          console.log(`  ${c('red', '✗')} ${resolved.error}`);
          console.log();
        }
        process.exitCode = resolved.status;
        return null;
      }
      requests = resolved.requests;
    }
  } else if (localSkillPath) {
    const resolved = resolveLocalSkillRequests(localSkillPath);
    if ('error' in resolved) {
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: false,
            source: localSkillPath,
            error: resolved.error,
          })
        );
      } else {
        console.log();
        console.log(`  ${c('red', '✗')} ${resolved.error}`);
        console.log();
      }
      process.exitCode = resolved.status;
      return null;
    }
    requests = resolved.requests;
  } else if (githubFolder) {
    const ref = parseGitHubSkillFolder(githubFolder, branchOverride);
    if (!ref) {
      printUsageError(
        'Expected a GitHub path URL or owner/repo/path shorthand',
        jsonOutput
      );
      return null;
    }

    const spinner = jsonOutput
      ? null
      : new Spinner(`Resolving ${githubFolder}...`).start();
    const resolved = await resolveGitHubSkillRequests(ref, undefined);
    spinner?.stop();
    if ('error' in resolved) {
      if (jsonOutput) {
        const skill = buildMarketplaceSkill(ref);
        console.log(
          JSON.stringify({
            success: false,
            skill: skill?.name,
            source: ref.url,
            error: resolved.error,
          })
        );
      } else {
        console.log();
        console.log(`  ${c('red', '✗')} ${resolved.error}`);
        console.log();
      }
      process.exitCode = resolved.status;
      return null;
    }
    requests = resolved.requests;
  }

  if (requests.length === 0) {
    printUsageError('No installable skills were found', jsonOutput);
    return null;
  }

  return requests;
}
