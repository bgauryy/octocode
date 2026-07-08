import path from 'node:path';
import { existsSync, readdirSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import type {
  MarketplaceSkill,
  MarketplaceSource,
} from '../../configs/skills-marketplace.js';
import {
  getSkillTargetDestinations,
  parseUserSkillPlatformList,
} from '../../features/skills.js';
import { c, bold, dim } from '../../utils/colors.js';
import { dirExists, fileExists } from '../../utils/fs.js';
import { HOME } from '../../utils/platform.js';
import {
  formatSkillInstallTargets,
  getAvailableSkills,
  getSkillsSourceDir,
  isSafeSkillName,
  installSkillToDestination,
  resolveModeForTarget,
  resolveSkillDestination,
  type SkillInstallResult,
  type SkillInstallStrategy,
  type SkillInstallTarget,
} from '../../utils/skills.js';
import {
  installMarketplaceSkill,
  readSkillFromGitHub,
  fetchMarketplaceSkills,
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

const OCTOCODE_SKILLS_SOURCE: MarketplaceSource = {
  id: 'github-bgauryy-octocode-main-skills',
  name: 'bgauryy/octocode',
  type: 'github',
  owner: OCTOCODE_SKILLS_GITHUB.owner,
  repo: OCTOCODE_SKILLS_GITHUB.repo,
  branch: OCTOCODE_SKILLS_GITHUB.branch,
  skillsPath: OCTOCODE_SKILLS_GITHUB.skillsPath,
  skillPattern: 'skill-folders',
  description: 'Official Octocode skills',
  url: `https://github.com/${OCTOCODE_SKILLS_GITHUB.owner}/${OCTOCODE_SKILLS_GITHUB.repo}/tree/${OCTOCODE_SKILLS_GITHUB.branch}/${OCTOCODE_SKILLS_GITHUB.skillsPath}`,
};

const KNOWN_OCTOCODE_SKILLS = [
  'octocode',
  'octocode-awareness',
  'octocode-brainstorming',
  'octocode-roast',
  'octocode-research',
  'octocode-rfc-generator',
  'octocode-skills',
  'octocode-stats',
];

const RECOMMENDED_SKILL = 'octocode-research';
const DEFAULT_INSTALL_MODE: SkillInstallStrategy = 'symlink';

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

type SkillInstallRequest = {
  skill: MarketplaceSkill;
  sourceUrl: string;
};

type DestinationInstallResult = {
  target: SkillInstallTarget;
  destPath: string;
  result: SkillInstallResult;
};

type SkillCommandResult = {
  skill: MarketplaceSkill;
  source: string;
  sourcePath: string;
  targets: DestinationInstallResult[];
  installed: number;
  skipped: number;
  failed: number;
  error?: string;
};

function getCanonicalSkillSourceRoot(): string {
  return path.join(paths.home, 'skills');
}

function getSkillSourcePath(skillName: string): string {
  return path.join(getCanonicalSkillSourceRoot(), skillName);
}

function buildGitHubSourceUrl(skill: MarketplaceSkill): string {
  const source = skill.source;
  return `https://github.com/${source.owner}/${source.repo}/tree/${source.branch}/${skill.path}`;
}

function buildOctocodeSkillsSource(branchOverride?: string): MarketplaceSource {
  const branch = branchOverride ?? OCTOCODE_SKILLS_GITHUB.branch;
  return {
    ...OCTOCODE_SKILLS_SOURCE,
    branch,
    id: `github-${OCTOCODE_SKILLS_GITHUB.owner}-${OCTOCODE_SKILLS_GITHUB.repo}-${branch}-${OCTOCODE_SKILLS_GITHUB.skillsPath}`,
    url: `https://github.com/${OCTOCODE_SKILLS_GITHUB.owner}/${OCTOCODE_SKILLS_GITHUB.repo}/tree/${branch}/${OCTOCODE_SKILLS_GITHUB.skillsPath}`,
  };
}

function getBundledSkillsSource(): MarketplaceSource | null {
  try {
    const skillsPath = getSkillsSourceDir();
    return {
      id: 'bundled-octocode-skills',
      name: 'Bundled',
      type: 'local',
      owner: '',
      repo: '',
      branch: '',
      skillsPath,
      skillPattern: 'skill-folders',
      description: 'Bundled Octocode skills',
      url: `file://${skillsPath}`,
    };
  } catch {
    return null;
  }
}

function tryResolveBundledSkillRequest(
  skillName: string
): SkillInstallRequest | null {
  try {
    if (!isSafeSkillName(skillName)) return null;

    const bundledSource = getBundledSkillsSource();
    if (!bundledSource) return null;

    const skillPath = path.join(bundledSource.skillsPath, skillName);
    if (
      !dirExists(skillPath) ||
      !fileExists(path.join(skillPath, 'SKILL.md'))
    ) {
      return null;
    }

    return {
      skill: {
        name: skillName,
        displayName: formatSkillName(skillName),
        description: 'Bundled Octocode skill',
        path: skillName,
        source: bundledSource,
      },
      sourceUrl: `file://${skillPath}`,
    };
  } catch {
    return null;
  }
}

function resolveAllBundledSkillRequests(): SkillInstallRequest[] {
  try {
    const bundledSource = getBundledSkillsSource();
    if (!bundledSource) return [];

    return getAvailableSkills()
      .filter(name =>
        fileExists(path.join(bundledSource.skillsPath, name, 'SKILL.md'))
      )
      .map(name => ({
        skill: {
          name,
          displayName: formatSkillName(name),
          description: 'Bundled Octocode skill',
          path: name,
          source: bundledSource,
        },
        sourceUrl: `file://${path.join(bundledSource.skillsPath, name)}`,
      }));
  } catch {
    return [];
  }
}

function buildGitHubLibrarySource(ref: GithubSkillFolder): MarketplaceSource {
  const sourceId = slugify(
    ['github', ref.owner, ref.repo, ref.branch, ref.skillPath || 'root'].join(
      '-'
    )
  );

  return {
    id: sourceId,
    name: `${ref.owner}/${ref.repo}`,
    type: 'github',
    owner: ref.owner,
    repo: ref.repo,
    branch: ref.branch,
    skillsPath: ref.skillPath,
    skillPattern: 'skill-folders',
    description: `GitHub skills library ${ref.owner}/${ref.repo}/${ref.skillPath}`,
    url: ref.url,
  };
}

function buildLocalSkillSource(sourceRoot: string): MarketplaceSource {
  const resolvedRoot = path.resolve(sourceRoot);
  return {
    id: slugify(['local', resolvedRoot].join('-')) || 'local-skills',
    name: 'Local skills',
    type: 'local',
    owner: '',
    repo: '',
    branch: '',
    skillsPath: resolvedRoot,
    skillPattern: 'skill-folders',
    description: 'Local skill folder',
    url: pathToFileURL(resolvedRoot).href,
  };
}

function stripLocalSkillMd(rawPath: string): string {
  const trimmed = rawPath.trim();
  return path.basename(trimmed).toLowerCase() === 'skill.md'
    ? path.dirname(trimmed)
    : trimmed;
}

function expandLocalPath(rawPath: string): string {
  const trimmed = rawPath.trim();
  if (trimmed === '~') return HOME;
  if (trimmed.startsWith('~/') || trimmed.startsWith('~\\')) {
    return path.join(HOME, trimmed.slice(2));
  }
  return trimmed;
}

function buildLocalSkillRequest(skillDir: string): SkillInstallRequest | null {
  const resolvedSkillDir = path.resolve(stripLocalSkillMd(skillDir));
  if (
    !dirExists(resolvedSkillDir) ||
    !fileExists(path.join(resolvedSkillDir, 'SKILL.md'))
  ) {
    return null;
  }

  const skillName = path.basename(resolvedSkillDir);
  if (!isSafeSkillName(skillName)) {
    return null;
  }

  return {
    skill: {
      name: skillName,
      displayName: formatSkillName(skillName),
      description: 'Local skill folder',
      path: skillName,
      source: buildLocalSkillSource(path.dirname(resolvedSkillDir)),
    },
    sourceUrl: pathToFileURL(resolvedSkillDir).href,
  };
}

function resolveLocalSkillRequests(
  rawPath: string
):
  | { requests: SkillInstallRequest[] }
  | { error: string; status: typeof EXIT.NOT_FOUND | typeof EXIT.USAGE } {
  const resolvedPath = path.resolve(
    stripLocalSkillMd(expandLocalPath(rawPath))
  );
  const directSkill = buildLocalSkillRequest(resolvedPath);
  if (directSkill) {
    return { requests: [directSkill] };
  }

  if (!dirExists(resolvedPath)) {
    return {
      error: `Local skill path not found: ${resolvedPath}`,
      status: EXIT.NOT_FOUND,
    };
  }

  let entries: string[] = [];
  try {
    entries = readdirSync(resolvedPath, { withFileTypes: true })
      .filter(entry => entry.isDirectory() || entry.isSymbolicLink())
      .map(entry => entry.name)
      .sort((a, b) => a.localeCompare(b));
  } catch (error) {
    return {
      error:
        error instanceof Error
          ? error.message
          : `Could not read local skill path: ${resolvedPath}`,
      status: EXIT.NOT_FOUND,
    };
  }

  const requests = entries
    .map(name => buildLocalSkillRequest(path.join(resolvedPath, name)))
    .filter((request): request is SkillInstallRequest => request !== null);

  if (requests.length > 0) {
    return { requests };
  }

  return {
    error: `Local path does not contain a SKILL.md or direct child skill folders: ${resolvedPath}`,
    status: EXIT.NOT_FOUND,
  };
}

function buildKnownOctocodeSkillRequests(
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

async function resolveGitHubSkillRequests(
  ref: GithubSkillFolder,
  namedSkill: string | undefined
): Promise<
  | { requests: SkillInstallRequest[] }
  | { error: string; status: typeof EXIT.NOT_FOUND | typeof EXIT.USAGE }
> {
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

async function resolveOctocodeAllSkillRequests(
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

function countResult(
  results: { installed: number; skipped: number; failed: number },
  result: SkillInstallResult
): void {
  if (result === 'installed') results.installed++;
  else if (result === 'skipped') results.skipped++;
  else results.failed++;
}

function resultSummary(result: {
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

function targetResults(targets: DestinationInstallResult[]): {
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

function skillJsonResult(result: SkillCommandResult): {
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

function printUsageError(message: string, jsonOutput: boolean): void {
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

function short(p: string): string {
  return p.replace(HOME, '~');
}

export const skillCommand: CLICommand = {
  name: 'skill',
  options: [
    { name: 'add', hasValue: true },
    { name: 'path', hasValue: true },
    { name: 'name', hasValue: true },
    { name: 'platform', hasValue: true },
    { name: 'target', hasValue: true },
    { name: 'branch', hasValue: true },
    { name: 'mode', hasValue: true, default: DEFAULT_INSTALL_MODE },
    { name: 'force' },
    { name: 'update' },
    { name: 'dry-run' },
    { name: 'list' },
    { name: 'all' },
    { name: 'install-all' },
    { name: 'all-skills' },
    { name: 'verbose' },
    { name: 'json' },
  ],
  handler: async (args: ParsedArgs) => {
    const jsonOutput = Boolean(args.options['json']);
    const dryRun = Boolean(args.options['dry-run']);
    const verbose = Boolean(args.options['verbose']);
    const listFlag = Boolean(args.options['list']);

    // ── --list: show available Octocode named skills ──────────────────────────
    if (listFlag) {
      const spinner = jsonOutput
        ? null
        : new Spinner('Fetching Octocode skills list...').start();

      let skills: Awaited<ReturnType<typeof fetchMarketplaceSkills>> = [];
      let fetchFailed = false;

      try {
        skills = await fetchMarketplaceSkills(OCTOCODE_SKILLS_SOURCE);
      } catch {
        fetchFailed = true;
      }

      spinner?.stop();

      if (jsonOutput) {
        const payload = fetchFailed
          ? {
              success: false,
              source: OCTOCODE_SKILLS_SOURCE.url,
              skills: KNOWN_OCTOCODE_SKILLS.map(n => ({ name: n })),
              fallback: true,
            }
          : {
              success: true,
              source: OCTOCODE_SKILLS_SOURCE.url,
              skills: skills.map(s => ({
                name: s.name,
                displayName: s.displayName,
                description: s.description,
              })),
            };
        console.log(JSON.stringify(payload));
        return;
      }

      console.log();
      if (fetchFailed) {
        console.log(
          `  ${bold('Octocode skills')}  ${dim('(live list unavailable — showing known names)')}`
        );
        console.log();
        console.log(`  ${KNOWN_OCTOCODE_SKILLS.join('  ')}`);
      } else {
        console.log(
          `  ${bold('Available Octocode skills')}  ${dim('·')}  ${dim(OCTOCODE_SKILLS_SOURCE.url)}`
        );
        console.log();
        const nameWidth = Math.max(...skills.map(s => s.name.length)) + 2;
        for (const s of skills) {
          const star = s.name === RECOMMENDED_SKILL ? c('yellow', '⭐') : '  ';
          console.log(
            `  ${star}  ${s.name.padEnd(nameWidth)}${dim(s.description)}`
          );
        }
      }
      console.log();
      console.log(`  ${dim('Install:')}  octocode skill --name <skill-name>`);
      console.log(`  ${dim('Install all:')}  octocode skill --install-all`);
      console.log(
        `  ${dim('Example:')}  octocode skill --name octocode-research`
      );
      console.log(
        `  ${dim('Example:')}  octocode skill --add owner/repo/skills --platform common`
      );
      console.log();
      return;
    }

    // ── Parse source ──────────────────────────────────────────────────────────
    const rawAdd = args.options['add'];
    const rawPath = args.options['path'];
    const rawName = args.options['name'];
    const namedSkill =
      typeof rawName === 'string' && rawName.trim().length > 0
        ? rawName.trim()
        : undefined;
    const githubFolder =
      typeof rawAdd === 'string' && rawAdd.trim().length > 0
        ? rawAdd.trim()
        : rawAdd === true
          ? undefined
          : args.args[0];
    const localSkillPath =
      typeof rawPath === 'string' && rawPath.trim().length > 0
        ? rawPath.trim()
        : undefined;
    const installAll = Boolean(
      args.options['install-all'] || args.options['all-skills']
    );

    if (!githubFolder && !localSkillPath && !namedSkill && !installAll) {
      printUsageError(
        'Missing GitHub skill path, local --path, Octocode skill name, or --install-all  (try --list to browse)',
        jsonOutput
      );
      return;
    }

    const sourceChoices = [
      Boolean(githubFolder),
      Boolean(localSkillPath),
      Boolean(namedSkill),
      installAll,
    ].filter(Boolean).length;
    if (sourceChoices > 1) {
      printUsageError(
        'Use only one of --add <github-path>, --add --path <local-skill-or-skills-dir>, --name <octocode-skill>, or --install-all',
        jsonOutput
      );
      return;
    }

    // ── Parse mode (copy | symlink | hybrid) ──────────────────────────────────
    const rawMode = args.options['mode'];
    const mode =
      typeof rawMode === 'string' && rawMode.trim().length > 0
        ? rawMode.trim().toLowerCase()
        : DEFAULT_INSTALL_MODE;
    if (mode !== 'copy' && mode !== 'symlink' && mode !== 'hybrid') {
      printUsageError(
        'Invalid --mode value. Use copy, symlink, or hybrid.',
        jsonOutput
      );
      return;
    }

    // ── Parse platform (--all flag is shorthand for --platform all) ───────────
    const allFlag = Boolean(args.options['all']);
    const rawPlatform = allFlag
      ? 'all'
      : typeof args.options['platform'] === 'string' &&
          args.options['platform'].trim().length > 0
        ? args.options['platform']
        : typeof args.options['target'] === 'string' &&
            args.options['target'].trim().length > 0
          ? args.options['target']
          : 'common';

    const parsedPlatforms = parseUserSkillPlatformList(rawPlatform);
    if (parsedPlatforms.error) {
      printUsageError(
        `${parsedPlatforms.error}. Valid platforms: common, cursor, claude, codex, opencode, pi, copilot, gemini, all`,
        jsonOutput
      );
      return;
    }
    const platforms = parsedPlatforms.platforms;
    const targets = parsedPlatforms.targets;

    // ── Resolve source skill(s) ───────────────────────────────────────────────
    const branchOverride =
      typeof args.options['branch'] === 'string' &&
      args.options['branch'].trim().length > 0
        ? args.options['branch'].trim()
        : undefined;

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
          return;
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
          return;
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
        return;
      }
      requests = resolved.requests;
    } else if (githubFolder) {
      const ref = parseGitHubSkillFolder(githubFolder, branchOverride);
      if (!ref) {
        printUsageError(
          'Expected a GitHub path URL or owner/repo/path shorthand',
          jsonOutput
        );
        return;
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
        return;
      }
      requests = resolved.requests;
    }

    if (requests.length === 0) {
      printUsageError('No installable skills were found', jsonOutput);
      return;
    }

    // ── Resolve destinations ──────────────────────────────────────────────────
    const destinations = getSkillTargetDestinations(targets, undefined);
    const sourceRoot = getCanonicalSkillSourceRoot();
    const force =
      Boolean(args.options['force']) || Boolean(args.options['update']);

    // ── --dry-run: preview without installing ─────────────────────────────────
    if (dryRun) {
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
      return;
    }

    // ── Fetch from GitHub ─────────────────────────────────────────────────────
    const spinner = jsonOutput
      ? null
      : new Spinner(`Installing ${requests.length} skill(s)...`).start();

    // ── Install per destination ───────────────────────────────────────────────
    const results: SkillCommandResult[] = [];
    const totals = { installed: 0, skipped: 0, failed: 0 };

    for (const request of requests) {
      const { skill } = request;
      const sourcePath = getSkillSourcePath(skill.name);
      spinner?.update(`Fetching ${skill.name}...`);

      const fetchResult = await installMarketplaceSkill(skill, sourceRoot);
      if (
        !fetchResult.success ||
        !fileExists(path.join(sourcePath, 'SKILL.md'))
      ) {
        const error =
          fetchResult.error ??
          `Fetched folder did not contain a SKILL.md file: ${skill.path}`;
        const failedTargets = destinations.map(dest => ({
          target: dest.target,
          destPath:
            resolveSkillDestination(dest.destDir, skill.name) ??
            path.join(dest.destDir, skill.name),
          result: 'failed' as SkillInstallResult,
        }));

        for (const failedTarget of failedTargets) {
          countResult(totals, failedTarget.result);
        }

        results.push({
          skill,
          source: request.sourceUrl,
          sourcePath,
          targets: failedTargets,
          installed: 0,
          skipped: 0,
          failed: failedTargets.length,
          error,
        });
        continue;
      }

      spinner?.update(`Linking ${skill.name}...`);
      const skillResult: SkillCommandResult = {
        skill,
        source: request.sourceUrl,
        sourcePath,
        targets: [],
        installed: 0,
        skipped: 0,
        failed: 0,
      };

      for (const dest of destinations) {
        const destinationPath = resolveSkillDestination(
          dest.destDir,
          skill.name
        );
        const effectiveMode = resolveModeForTarget(
          mode as SkillInstallStrategy,
          dest.target
        );
        const result = destinationPath
          ? installSkillToDestination({
              sourcePath,
              destinationPath,
              mode: effectiveMode,
              force,
            })
          : 'failed';

        skillResult.targets.push({
          target: dest.target,
          destPath: destinationPath ?? path.join(dest.destDir, skill.name),
          result,
        });
        countResult(skillResult, result);
        countResult(totals, result);
      }

      results.push(skillResult);
    }

    if (totals.failed > 0) {
      spinner?.fail(`Installed ${requests.length} skill(s) with errors`);
      process.exitCode = EXIT.GENERAL;
    } else if (totals.skipped > 0 && totals.installed === 0) {
      spinner?.warn?.(`${requests.length} skill(s) already installed`);
    } else {
      spinner?.succeed(`Installed ${requests.length} skill(s)`);
    }

    // ── JSON output ───────────────────────────────────────────────────────────
    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: totals.failed === 0,
          skills: results.map(skillJsonResult),
          platforms,
          mode,
          summary: resultSummary(totals),
        })
      );
      return;
    }

    // ── Human output ──────────────────────────────────────────────────────────
    console.log();
    console.log(`  ${dim('Mode:')} ${mode}`);
    console.log(`  ${dim('Platforms:')} ${platforms.join(', ')}`);
    console.log();
    for (const result of results) {
      console.log(`  ${bold(result.skill.displayName)}`);
      console.log(`  ${dim('Source:')} ${short(result.sourcePath)}`);
      if (verbose) {
        console.log(`  ${dim(result.source)}`);
      }
      if (result.error) {
        console.log(`  ${c('red', '✗')}  ${dim(result.error)}`);
      }
      for (const r of result.targets) {
        if (r.result === 'installed') {
          console.log(
            `  ${c('green', '✓')}  ${r.target.padEnd(14)} ${short(r.destPath)}`
          );
        } else if (r.result === 'skipped') {
          console.log(
            `  ${c('yellow', '~')}  ${r.target.padEnd(14)} ${short(r.destPath)}  ${dim('(already installed — use --force to overwrite)')}`
          );
        } else {
          console.log(
            `  ${c('red', '✗')}  ${r.target.padEnd(14)} ${short(r.destPath)}  ${dim('(valid targets: ' + formatSkillInstallTargets() + ')')}`
          );
        }
      }
      console.log();
    }
    console.log(
      `  ${dim('Summary:')} ${totals.installed} installed, ${totals.skipped} skipped, ${totals.failed} failed`
    );
    console.log();
  },
};
