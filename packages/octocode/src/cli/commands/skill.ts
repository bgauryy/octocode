import path from 'node:path';
import type { MarketplaceSkill } from '../../configs/skills-marketplace.js';
import {
  getSkillTargetDestinations,
  installSkillForTargets,
  parseUserSkillPlatformList,
} from '../../features/skills.js';
import { c, bold, dim } from '../../utils/colors.js';
import { fileExists } from '../../utils/fs.js';
import { HOME } from '../../utils/platform.js';
import {
  formatSkillInstallTargets,
  isSafeSkillName,
  USER_SKILL_PLATFORM_TARGETS,
  type SkillInstallStrategy,
  type SkillInstallTarget,
  type UserSkillPlatform,
} from '../../utils/skills.js';
import {
  installMarketplaceSkill,
  readSkillFromGitHub,
} from '../../utils/skills-fetch.js';
import { Spinner } from '../../utils/spinner.js';
import { paths } from '@octocodeai/octocode-tools-core/paths';
import { EXIT } from '../exit-codes.js';
import type { CLICommand, ParsedArgs } from '../types.js';

type GithubSkillFolder = {
  owner: string;
  repo: string;
  branch: string;
  skillPath: string;
  url: string;
};

const OCTOCODE_SKILLS_GITHUB = {
  owner: 'bgauryy',
  repo: 'octocode',
  branch: 'main',
  skillsPath: 'skills',
} as const;

function stripSkillMd(input: string): string {
  return input
    .replace(/\/SKILL\.md$/i, '')
    .replace(/^SKILL\.md$/i, '')
    .replace(/^\/+|\/+$/g, '');
}

function parseGitHubUrl(
  rawInput: string,
  branchOverride?: string
): GithubSkillFolder | null {
  let url: URL;
  try {
    url = new URL(rawInput);
  } catch {
    return null;
  }

  if (url.hostname !== 'github.com') {
    return null;
  }

  const parts = url.pathname.split('/').filter(Boolean);
  const [owner, rawRepo, kind, ...rest] = parts;
  if (!owner || !rawRepo) {
    return null;
  }

  const repo = rawRepo.replace(/\.git$/i, '');
  if (kind === 'tree' || kind === 'blob') {
    const branch = branchOverride ?? rest[0] ?? 'main';
    const skillPath = stripSkillMd(rest.slice(1).join('/'));
    return {
      owner,
      repo,
      branch,
      skillPath,
      url: rawInput,
    };
  }

  return {
    owner,
    repo,
    branch: branchOverride ?? 'main',
    skillPath: '',
    url: rawInput,
  };
}

function parseGitHubShorthand(
  rawInput: string,
  branchOverride?: string
): GithubSkillFolder | null {
  const cleaned = rawInput.replace(/^github:/i, '').replace(/^\/+|\/+$/g, '');
  const parts = cleaned.split('/').filter(Boolean);
  const [owner, rawRepo, maybeKind, ...rest] = parts;
  if (!owner || !rawRepo) {
    return null;
  }

  const [repoName, inlineBranch] = rawRepo.replace(/\.git$/i, '').split('@');
  const repo = repoName;
  if (!repo) {
    return null;
  }

  if (maybeKind === 'tree' || maybeKind === 'blob') {
    const branch = branchOverride ?? rest[0] ?? inlineBranch ?? 'main';
    return {
      owner,
      repo,
      branch,
      skillPath: stripSkillMd(rest.slice(1).join('/')),
      url: `https://github.com/${owner}/${repo}/tree/${branch}/${stripSkillMd(
        rest.slice(1).join('/')
      )}`,
    };
  }

  const branch = branchOverride ?? inlineBranch ?? 'main';
  const skillPath = stripSkillMd(parts.slice(2).join('/'));
  return {
    owner,
    repo,
    branch,
    skillPath,
    url: `https://github.com/${owner}/${repo}/tree/${branch}/${skillPath}`,
  };
}

function parseGitHubSkillFolder(
  rawInput: string,
  branchOverride?: string
): GithubSkillFolder | null {
  return (
    parseGitHubUrl(rawInput, branchOverride) ??
    parseGitHubShorthand(rawInput, branchOverride)
  );
}

function buildOctocodeSkillFolder(
  skillName: string,
  branchOverride?: string
): GithubSkillFolder | null {
  if (!isSafeSkillName(skillName)) {
    return null;
  }

  const branch = branchOverride ?? OCTOCODE_SKILLS_GITHUB.branch;
  const skillPath = `${OCTOCODE_SKILLS_GITHUB.skillsPath}/${skillName}`;
  return {
    owner: OCTOCODE_SKILLS_GITHUB.owner,
    repo: OCTOCODE_SKILLS_GITHUB.repo,
    branch,
    skillPath,
    url: `https://github.com/${OCTOCODE_SKILLS_GITHUB.owner}/${OCTOCODE_SKILLS_GITHUB.repo}/tree/${branch}/${skillPath}`,
  };
}

function formatSkillName(name: string): string {
  return name
    .replace(/[-_]/g, ' ')
    .replace(/\b\w/g, char => char.toUpperCase());
}

function slugify(input: string): string {
  return input.replace(/[^a-zA-Z0-9._-]+/g, '-').replace(/^-+|-+$/g, '');
}

function buildMarketplaceSkill(
  ref: GithubSkillFolder
): MarketplaceSkill | null {
  const skillName = path.posix.basename(ref.skillPath || ref.repo);
  if (!isSafeSkillName(skillName)) {
    return null;
  }

  const sourceId = slugify(
    ['github', ref.owner, ref.repo, ref.branch, ref.skillPath || 'root'].join(
      '-'
    )
  );

  return {
    name: skillName,
    displayName: formatSkillName(skillName),
    description: `GitHub skill folder ${ref.owner}/${ref.repo}/${ref.skillPath}`,
    path: ref.skillPath,
    source: {
      id: sourceId,
      name: `${ref.owner}/${ref.repo}`,
      type: 'github',
      owner: ref.owner,
      repo: ref.repo,
      branch: ref.branch,
      skillsPath: ref.skillPath ? path.posix.dirname(ref.skillPath) : '',
      skillPattern: 'skill-folders',
      description: 'GitHub skill folder',
      url: ref.url,
    },
  };
}

function printUsageError(message: string, jsonOutput: boolean): void {
  if (jsonOutput) {
    console.log(JSON.stringify({ success: false, error: message }));
  } else {
    console.log();
    console.log(`  ${c('red', '✗')} ${message}`);
    console.log(
      `  ${dim('Usage:')} skill (--add <github-folder> | --name <octocode-skill>) --platform <common|cursor|claude|codex|all>`
    );
    console.log(
      `  ${dim('Example:')} skill --add https://github.com/owner/repo/tree/main/skills/my-skill --platform cursor`
    );
    console.log(
      `  ${dim('Example:')} skill --name octocode-engineer --platform codex`
    );
    console.log();
  }
  process.exitCode = EXIT.USAGE;
}

function platformTargets(
  platforms: readonly UserSkillPlatform[]
): SkillInstallTarget[] {
  return [
    ...new Set(
      platforms.flatMap(platform => USER_SKILL_PLATFORM_TARGETS[platform])
    ),
  ];
}

export const skillCommand: CLICommand = {
  name: 'skill',
  options: [
    { name: 'add', hasValue: true },
    { name: 'name', hasValue: true },
    { name: 'platform', hasValue: true },
    { name: 'target', hasValue: true },
    { name: 'branch', hasValue: true },
    { name: 'mode', hasValue: true, default: 'copy' },
    { name: 'force' },
    { name: 'json' },
  ],
  handler: async (args: ParsedArgs) => {
    const jsonOutput = Boolean(args.options['json']);
    const rawAdd = args.options['add'];
    const rawName = args.options['name'];
    const namedSkill =
      typeof rawName === 'string' && rawName.trim().length > 0
        ? rawName.trim()
        : undefined;
    const githubFolder =
      typeof rawAdd === 'string' && rawAdd.trim().length > 0
        ? rawAdd.trim()
        : args.args[0];

    if (!githubFolder && !namedSkill) {
      printUsageError(
        'Missing GitHub skill folder or Octocode skill name',
        jsonOutput
      );
      return;
    }

    if (githubFolder && namedSkill) {
      printUsageError(
        'Use either --add <github-folder> or --name <octocode-skill>, not both',
        jsonOutput
      );
      return;
    }

    const rawMode = args.options['mode'];
    const mode =
      typeof rawMode === 'string' && rawMode.trim().length > 0
        ? rawMode.trim().toLowerCase()
        : 'copy';
    if (mode !== 'copy' && mode !== 'symlink') {
      printUsageError('Invalid --mode value. Use copy or symlink.', jsonOutput);
      return;
    }

    const branchOverride =
      typeof args.options['branch'] === 'string' &&
      args.options['branch'].trim().length > 0
        ? args.options['branch'].trim()
        : undefined;
    const ref = namedSkill
      ? buildOctocodeSkillFolder(namedSkill, branchOverride)
      : parseGitHubSkillFolder(githubFolder, branchOverride);
    if (!ref) {
      printUsageError(
        namedSkill
          ? 'Invalid Octocode skill name'
          : 'Expected a GitHub folder URL or owner/repo/path shorthand',
        jsonOutput
      );
      return;
    }

    const skill = buildMarketplaceSkill(ref);
    if (!skill) {
      printUsageError(
        'GitHub folder does not resolve to a safe skill name',
        jsonOutput
      );
      return;
    }

    const rawPlatform = args.options['platform'] ?? args.options['target'];
    if (typeof rawPlatform !== 'string' || rawPlatform.trim().length === 0) {
      printUsageError(
        'Missing required option: --platform <common|cursor|claude|codex|all>',
        jsonOutput
      );
      return;
    }

    const parsedPlatforms = parseUserSkillPlatformList(rawPlatform);
    if (parsedPlatforms.error) {
      printUsageError(
        `${parsedPlatforms.error}. Valid platforms: common, cursor, claude, codex, all`,
        jsonOutput
      );
      return;
    }
    const platforms = parsedPlatforms.platforms;

    const targets = platformTargets(platforms);
    const destinations = getSkillTargetDestinations(targets, undefined);
    const sourceRoot = path.join(paths.home, 'skill-sources', skill.source.id);
    const sourcePath = path.join(sourceRoot, skill.name);
    const force = Boolean(args.options['force']);

    const spinner = jsonOutput
      ? null
      : new Spinner(`Fetching ${skill.name} from GitHub...`).start();

    let readError: string | null = null;
    try {
      await readSkillFromGitHub(ref.owner, ref.repo, ref.skillPath, ref.branch);
    } catch (error) {
      readError = error instanceof Error ? error.message : String(error);
      if (namedSkill && readError.toLowerCase().includes('not found')) {
        readError = `Octocode skill not found: ${namedSkill} (${ref.url})`;
      }
    }

    if (readError) {
      spinner?.fail(`Could not fetch ${skill.name}`);
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: false,
            skill: skill.name,
            source: ref.url,
            error: readError,
          })
        );
      } else {
        console.log();
        console.log(`  ${c('red', '✗')} ${readError}`);
        console.log();
      }
      process.exitCode = EXIT.NOT_FOUND;
      return;
    }

    const fetchResult = await installMarketplaceSkill(skill, sourceRoot);
    if (
      !fetchResult.success ||
      !fileExists(path.join(sourcePath, 'SKILL.md'))
    ) {
      const error =
        fetchResult.error ??
        `Fetched folder did not contain a SKILL.md file: ${ref.skillPath}`;
      spinner?.fail(`Could not fetch ${skill.name}`);
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: false,
            skill: skill.name,
            source: ref.url,
            error,
          })
        );
      } else {
        console.log();
        console.log(`  ${c('red', '✗')} ${error}`);
        console.log();
      }
      process.exitCode = EXIT.GENERAL;
      return;
    }

    spinner?.update(`Installing ${skill.name}...`);
    const summary = installSkillForTargets({
      skillName: skill.name,
      sourceDir: sourceRoot,
      destinations,
      strategy: mode as SkillInstallStrategy,
      force,
    });

    if (summary.failed === 0) {
      spinner?.succeed(`Installed ${skill.name}`);
    } else {
      spinner?.fail(`Installed ${skill.name} with errors`);
      process.exitCode = EXIT.GENERAL;
    }

    const installedPaths = destinations.map(destination => ({
      target: destination.target,
      path: path.join(destination.destDir, skill.name),
    }));

    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: summary.failed === 0,
          skill: skill.name,
          source: ref.url,
          cachePath: sourcePath,
          platforms,
          targets: installedPaths,
          mode,
          installed: summary.installed,
          skipped: summary.skipped,
          failed: summary.failed,
        })
      );
      return;
    }

    console.log();
    console.log(`  ${bold(skill.displayName)}`);
    console.log(`  ${dim(ref.url)}`);
    console.log(`  ${dim('Source cache:')} ${sourcePath.replace(HOME, '~')}`);
    console.log();
    for (const destination of installedPaths) {
      console.log(
        `  ${c('cyan', '•')} ${destination.target}: ${destination.path.replace(
          HOME,
          '~'
        )}`
      );
    }
    if (summary.skipped > 0) {
      console.log();
      console.log(
        `  ${c('yellow', 'WARN')} Skipped ${summary.skipped} existing target(s). Use --force to overwrite.`
      );
    }
    if (summary.failed > 0) {
      console.log();
      console.log(
        `  ${c('red', '✗')} Failed ${summary.failed} target(s). Valid low-level targets: ${formatSkillInstallTargets()}`
      );
    }
    console.log();
  },
};
