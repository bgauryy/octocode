import path from 'node:path';
import { existsSync } from 'node:fs';
import type { MarketplaceSource } from '../../configs/skills-marketplace.js';
import {
  getSkillTargetDestinations,
  parseUserSkillPlatformList,
} from '../../features/skills.js';
import { c, bold, dim } from '../../utils/colors.js';
import { fileExists } from '../../utils/fs.js';
import { HOME } from '../../utils/platform.js';
import {
  formatSkillInstallTargets,
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
  'octocode-engineer',
  'octocode-roast',
  'octocode-brainstorming',
  'octocode-research',
  'octocode-rfc-generator',
  'octocode-loop',
  'octocode-awareness',
  'octocode-skills',
  'octocode-stats',
  'octocode',
];

const RECOMMENDED_SKILL = 'octocode-engineer';

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
): import('../../configs/skills-marketplace.js').MarketplaceSkill | null {
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
      `  ${dim('Usage:')} skill (--add <github-folder> | --name <octocode-skill>) [--platform common|cursor|claude|codex|opencode|pi|all] [--mode copy|symlink|hybrid] [--force|--update] [--dry-run]`
    );
    console.log(`  ${dim('List:  ')} skill --list`);
    console.log(`  ${dim('Example:')} skill --name octocode-engineer`);
    console.log(
      `  ${dim('Example:')} skill --name octocode-engineer --platform all --mode hybrid`
    );
    console.log(
      `  ${dim('Example:')} skill --add https://github.com/owner/repo/tree/main/skills/my-skill --platform cursor`
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
    { name: 'name', hasValue: true },
    { name: 'platform', hasValue: true },
    { name: 'target', hasValue: true },
    { name: 'branch', hasValue: true },
    { name: 'mode', hasValue: true, default: 'copy' },
    { name: 'force' },
    { name: 'update' },
    { name: 'dry-run' },
    { name: 'list' },
    { name: 'all' },
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
      console.log(
        `  ${dim('Example:')}  octocode skill --name octocode-engineer`
      );
      console.log(
        `  ${dim('Example:')}  octocode skill --name octocode-engineer --platform all`
      );
      console.log();
      return;
    }

    // ── Parse source ──────────────────────────────────────────────────────────
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
        'Missing GitHub skill folder or Octocode skill name  (try --list to browse)',
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

    // ── Parse mode (copy | symlink | hybrid) ──────────────────────────────────
    const rawMode = args.options['mode'];
    const mode =
      typeof rawMode === 'string' && rawMode.trim().length > 0
        ? rawMode.trim().toLowerCase()
        : 'copy';
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
        `${parsedPlatforms.error}. Valid platforms: common, cursor, claude, codex, opencode, pi, all`,
        jsonOutput
      );
      return;
    }
    const platforms = parsedPlatforms.platforms;
    const targets = parsedPlatforms.targets;

    // ── Resolve skill ref ─────────────────────────────────────────────────────
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

    // ── Resolve destinations ──────────────────────────────────────────────────
    const destinations = getSkillTargetDestinations(targets, undefined);
    const sourceRoot = path.join(paths.home, 'skill-sources', skill.source.id);
    const sourcePath = path.join(sourceRoot, skill.name);
    const force =
      Boolean(args.options['force']) || Boolean(args.options['update']);

    // ── --dry-run: preview without fetching or installing ─────────────────────
    if (dryRun) {
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            dryRun: true,
            skill: skill.name,
            source: ref.url,
            mode,
            platforms,
            targets: destinations.map(d => {
              const destPath = path.join(d.destDir, skill.name);
              const exists = existsSync(destPath);
              return {
                target: d.target,
                path: destPath,
                action: exists ? (force ? 'overwrite' : 'skip') : 'install',
              };
            }),
          })
        );
        return;
      }

      console.log();
      console.log(`  ${c('cyan', 'DRY RUN')} ${dim('— nothing will change')}`);
      console.log();
      console.log(`  Skill:   ${bold(skill.displayName)}`);
      console.log(`  Source:  ${dim(ref.url)}`);
      console.log(`  Mode:    ${mode}`);
      console.log();
      console.log(`  ${dim('Destinations:')}`);
      for (const dest of destinations) {
        const destPath = path.join(dest.destDir, skill.name);
        const exists = existsSync(destPath);
        if (!exists) {
          console.log(
            `  ${c('green', '+')}  ${dest.target.padEnd(14)} ${short(destPath)}`
          );
        } else if (force) {
          console.log(
            `  ${c('yellow', '↺')}  ${dest.target.padEnd(14)} ${short(destPath)}  ${dim('(overwrite)')}`
          );
        } else {
          console.log(
            `  ${c('yellow', '~')}  ${dest.target.padEnd(14)} ${short(destPath)}  ${dim('(already installed — add --force to overwrite)')}`
          );
        }
      }
      console.log();
      return;
    }

    // ── Fetch from GitHub ─────────────────────────────────────────────────────
    const spinner = jsonOutput
      ? null
      : new Spinner(`Fetching ${skill.name}...`).start();

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

    // ── Install per destination ───────────────────────────────────────────────
    spinner?.update(`Installing ${skill.name}...`);

    type DestResult = {
      target: SkillInstallTarget;
      destPath: string;
      result: SkillInstallResult;
    };
    const installResults: DestResult[] = [];
    let installed = 0,
      skipped = 0,
      failed = 0;

    for (const dest of destinations) {
      const destinationPath = resolveSkillDestination(dest.destDir, skill.name);
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

      installResults.push({
        target: dest.target,
        destPath: destinationPath ?? path.join(dest.destDir, skill.name),
        result,
      });
      if (result === 'installed') installed++;
      else if (result === 'skipped') skipped++;
      else failed++;
    }

    if (failed > 0) {
      spinner?.fail(`Installed ${skill.name} with errors`);
      process.exitCode = EXIT.GENERAL;
    } else if (skipped > 0 && installed === 0) {
      spinner?.warn?.(`${skill.name} already installed`);
    } else {
      spinner?.succeed(`Installed ${skill.name}`);
    }

    // ── JSON output ───────────────────────────────────────────────────────────
    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: failed === 0,
          skill: skill.name,
          source: ref.url,
          cachePath: sourcePath,
          platforms,
          targets: installResults.map(r => ({
            target: r.target,
            path: r.destPath,
            result: r.result,
          })),
          mode,
          installed,
          skipped,
          failed,
        })
      );
      return;
    }

    // ── Human output ──────────────────────────────────────────────────────────
    console.log();
    console.log(`  ${bold(skill.displayName)}`);
    if (verbose) {
      console.log(`  ${dim(ref.url)}`);
      console.log(`  ${dim('Cache:')} ${short(sourcePath)}`);
    }
    console.log();
    for (const r of installResults) {
      if (r.result === 'installed') {
        console.log(
          `  ${c('green', '✓')}  ${r.target.padEnd(14)} ${short(r.destPath)}`
        );
      } else if (r.result === 'skipped') {
        console.log(
          `  ${c('yellow', '~')}  ${r.target.padEnd(14)} ${dim('already installed')}  ${dim('(use --force to overwrite)')}`
        );
      } else {
        console.log(
          `  ${c('red', '✗')}  ${r.target.padEnd(14)} ${dim('failed')}  ${dim('(valid targets: ' + formatSkillInstallTargets() + ')')}`
        );
      }
    }
    console.log();
  },
};
