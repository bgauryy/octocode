import type { CLICommand, ParsedArgs } from '../types.js';
import { c, bold, dim } from '../../utils/colors.js';
import { dirExists } from '../../utils/fs.js';
import {
  CLAUDE_SKILL_INSTALL_TARGETS,
  DEFAULT_SKILL_INSTALL_TARGETS,
  SKILL_INSTALL_TARGETS,
  formatSkillInstallTargets,
  getSkillsSourceDir,
  getSkillsDestDir,
  getSkillsDirForTarget,
  getSkillMetadata,
  isSafeSkillName,
} from '../../utils/skills.js';
import { listSubdirectories } from '../../utils/fs.js';
import {
  getAvailableSkillNames,
  getSkillTargetDestinations,
  installAllSkillsForTargets,
  installSkillForTargets,
  parseSkillTargetList,
  removeSkillFromTargets,
} from '../../features/skills.js';
import {
  fetchSkillsShSearch,
  readSkillFromGitHub,
  type SkillsShResult,
} from '../../utils/skills-fetch.js';
import { readFileContent, fileExists } from '../../utils/fs.js';
import { parseSkillFrontmatter } from '../../utils/parsers/frontmatter.js';
import { HOME } from '../../utils/platform.js';
import { resolve as resolvePath, basename as pathBasename } from 'node:path';
import { loadInquirer, select, checkbox } from '../../utils/prompts.js';
import { Spinner } from '../../utils/spinner.js';
import path from 'node:path';
import {
  type SkillInstallMode,
  type SkillInstallStrategy,
  type SkillInstallTarget,
} from './shared.js';

type SkillReadTarget =
  | { type: 'local'; skillDir: string }
  | {
      type: 'github';
      owner: string;
      repo: string;
      skillPath: string;
      branch: string;
    };

function parseSkillReadTarget(input: string): SkillReadTarget | null {
  const expanded = input.startsWith('~/')
    ? input.replace('~/', `${HOME}/`)
    : input;

  if (
    expanded.startsWith('/') ||
    expanded.startsWith('./') ||
    expanded.startsWith('../')
  ) {
    const abs = resolvePath(expanded);
    const skillDir = abs.endsWith('SKILL.md')
      ? abs.slice(0, -'/SKILL.md'.length)
      : abs;
    return { type: 'local', skillDir };
  }

  if (expanded.startsWith('https://github.com/')) {
    const m = expanded.match(
      /github\.com\/([^/]+)\/([^/]+)\/(tree|blob)\/([^/]+)\/(.+)/
    );
    if (m) {
      const skillPath = m[5].replace(/\/SKILL\.md$/, '');
      return {
        type: 'github',
        owner: m[1],
        repo: m[2],
        branch: m[4],
        skillPath,
      };
    }
    const base = expanded.match(/github\.com\/([^/]+)\/([^/]+)\/?$/);
    if (base) {
      return {
        type: 'github',
        owner: base[1],
        repo: base[2],
        skillPath: '',
        branch: 'main',
      };
    }
    return null;
  }

  const parts = expanded.split('/');
  if (parts.length >= 2) {
    const owner = parts[0];
    const repo = parts[1];
    const skillPath = parts
      .slice(2)
      .join('/')
      .replace(/\/SKILL\.md$/, '')
      .replace(/\/SKILL\.md$/, '');
    return { type: 'github', owner, repo, skillPath, branch: 'main' };
  }

  return null;
}

async function promptInstallTargets(): Promise<SkillInstallTarget[]> {
  await loadInquirer();
  const targetPreset = await select<
    'claude-only' | 'all' | 'custom' | 'cancel'
  >({
    message: 'Install skills to which platforms?',
    choices: [
      {
        name: '- Claude locations (claude-code + claude-desktop)',
        value: 'claude-only',
      },
      { name: '- All supported platforms', value: 'all' },
      { name: '- Custom selection', value: 'custom' },
      { name: `${dim('- Cancel')}`, value: 'cancel' },
    ],
    loop: false,
  });
  if (targetPreset === 'cancel') return [];
  if (targetPreset === 'claude-only') {
    return [...CLAUDE_SKILL_INSTALL_TARGETS];
  }
  if (targetPreset === 'all') return [...SKILL_INSTALL_TARGETS];
  return await checkbox<SkillInstallTarget>({
    message: 'Select target platforms',
    choices: SKILL_INSTALL_TARGETS.map(target => ({
      name: `- ${target}`,
      value: target,
      checked: CLAUDE_SKILL_INSTALL_TARGETS.includes(target),
    })),
    required: true,
    loop: false,
  });
}

async function promptInstallStrategy(): Promise<SkillInstallStrategy | null> {
  await loadInquirer();
  const selected = await select<SkillInstallStrategy | 'cancel'>({
    message: 'How should skills be installed?',
    choices: [
      {
        name: '- Hybrid (copy for Claude targets, symlink for others)',
        value: 'hybrid',
      },
      { name: '- Full copies everywhere', value: 'copy' },
      { name: '- Symlinks everywhere', value: 'symlink' },
      { name: `${dim('- Cancel')}`, value: 'cancel' },
    ],
    loop: false,
  });
  return selected === 'cancel' ? null : selected;
}

const READ_TRUNCATE_CHARS = 3000;

export const skillsCommand: CLICommand = {
  name: 'skills',
  aliases: ['sk'],
  description: 'Search, install, and manage Octocode skills across AI clients',
  usage:
    'octocode skills [search|read|install|remove|list|sync] [--skill <name>] [--targets <list>] [--mode <copy|symlink>] [--json]',
  options: [
    { name: 'force', short: 'f', description: 'Overwrite existing skills' },
    {
      name: 'skill',
      short: 'k',
      description: 'Skill folder name (install/remove from bundled)',
      hasValue: true,
    },
    {
      name: 'local',
      description:
        'Path to a local skill folder — for read, install, or remove (e.g. ./my-skill or /abs/path)',
      hasValue: true,
    },
    {
      name: 'targets',
      short: 't',
      description: `Comma-separated targets: ${formatSkillInstallTargets()}`,
      hasValue: true,
    },
    {
      name: 'mode',
      short: 'm',
      description: 'Install mode: copy (default) or symlink',
      hasValue: true,
      default: 'copy',
    },
    {
      name: 'limit',
      short: 'l',
      description: 'Max results for search (default: 20)',
      hasValue: true,
    },
    {
      name: 'full',
      description: 'Show full SKILL.md without truncation (read only)',
    },
    {
      name: 'direct',
      description:
        'Search skills.sh directly and show results (human mode — skips agent protocol)',
    },
    {
      name: 'target',
      description: `Filter list to one target: ${formatSkillInstallTargets()} (list only)`,
      hasValue: true,
    },
    {
      name: 'install',
      short: 'i',
      description: 'Install the top search result (use with search --direct)',
    },
    {
      name: 'dry-run',
      short: 'n',
      description:
        'Show what would be installed without writing anything (install only)',
    },
    {
      name: 'json',
      short: 'j',
      description: 'Output as JSON',
    },
  ],
  handler: async (args: ParsedArgs) => {
    const subcommand = args.args[0] || 'list';
    const force = Boolean(args.options['force'] || args.options['f']);
    const jsonOutput = Boolean(args.options['json'] || args.options['j']);
    const fullOutput = Boolean(args.options['full']);
    const dryRun = Boolean(args.options['dry-run'] || args.options['n']);
    const installTopResult = Boolean(
      args.options['install'] || args.options['i']
    );
    const rawTargetFilter = args.options['target'];
    const targetFilter =
      typeof rawTargetFilter === 'string' && rawTargetFilter.length > 0
        ? rawTargetFilter.trim().toLowerCase()
        : undefined;
    const rawSkill = args.options['skill'] ?? args.options['k'];
    const specificSkill =
      typeof rawSkill === 'string' && rawSkill.length > 0
        ? rawSkill
        : undefined;
    const rawLocalPath = args.options['local'];
    const localPath =
      typeof rawLocalPath === 'string' && rawLocalPath.length > 0
        ? rawLocalPath
        : undefined;
    const rawTargets = args.options['targets'] ?? args.options['t'];
    const rawMode =
      subcommand === 'remove'
        ? undefined
        : (args.options['mode'] ?? args.options['m']);

    let installMode: SkillInstallMode = 'copy';
    if (typeof rawMode === 'string' && rawMode.trim().length > 0) {
      const normalizedMode = rawMode.trim().toLowerCase();
      if (normalizedMode !== 'copy' && normalizedMode !== 'symlink') {
        console.log();
        console.log(
          `  ${c('red', 'X')} Invalid --mode value: ${c('yellow', rawMode)}`
        );
        console.log(`  ${dim('Allowed values:')} copy, symlink`);
        console.log(
          `  ${dim('Example:')} octocode skills install --mode symlink`
        );
        console.log();
        process.exitCode = 1;
        return;
      }
      installMode = normalizedMode;
    }
    const hasExplicitTargets =
      typeof rawTargets === 'string' && rawTargets.trim().length > 0;
    const hasExplicitMode = typeof rawMode === 'string' && rawMode.length > 0;

    const srcDir = getSkillsSourceDir();
    const destDir = getSkillsDestDir();

    let selectedTargets: SkillInstallTarget[] = [
      ...DEFAULT_SKILL_INSTALL_TARGETS,
    ];
    if (typeof rawTargets === 'string' && rawTargets.trim().length > 0) {
      const parsed = parseSkillTargetList(rawTargets);
      selectedTargets = parsed.targets;
      if (parsed.error) {
        console.log();
        console.log(`  ${c('red', 'X')} ${parsed.error}`);
        console.log(
          `  ${dim('Valid targets:')} ${formatSkillInstallTargets()}`
        );
        console.log();
        process.exitCode = 1;
        return;
      }
    }
    let installStrategy: SkillInstallStrategy = installMode;

    if (
      subcommand === 'install' &&
      !localPath &&
      !jsonOutput &&
      process.stdout.isTTY &&
      (!hasExplicitTargets || !hasExplicitMode)
    ) {
      const promptedTargets = await promptInstallTargets();
      if (promptedTargets.length === 0) {
        console.log();
        console.log(`  ${c('yellow', 'WARN')} Skills install cancelled`);
        console.log();
        return;
      }
      selectedTargets = promptedTargets;
      const promptedStrategy = await promptInstallStrategy();
      if (!promptedStrategy) {
        console.log();
        console.log(`  ${c('yellow', 'WARN')} Skills install cancelled`);
        console.log();
        return;
      }
      installStrategy = promptedStrategy;
    }

    const targetDestinations = getSkillTargetDestinations(
      selectedTargets,
      destDir
    );

    const needsBundledSource =
      subcommand === 'install' && !localPath && !specificSkill;
    if (needsBundledSource && !dirExists(srcDir)) {
      console.log();
      console.log(`  ${c('red', '✗')} Skills directory not found`);
      console.log(`  ${dim('Expected:')} ${srcDir}`);
      console.log();
      process.exitCode = 1;
      return;
    }

    const availableSkills = getAvailableSkillNames(srcDir);

    if (subcommand === 'read') {
      const rawInput = args.args[1] ?? localPath;

      if (!rawInput) {
        console.log();
        console.log(`  ${c('red', '✗')} Missing path`);
        console.log(
          `  ${dim('Usage:')} octocode skills read <path-to-SKILL.md>`
        );
        console.log(`  ${dim('Local:')}  skills read ./my-skill/SKILL.md`);
        console.log(
          `  ${dim('GitHub:')} skills read owner/repo/skills/my-skill`
        );
        console.log(
          `  ${dim('GitHub:')} skills read https://github.com/owner/repo/tree/main/skills/my-skill`
        );
        console.log();
        process.exitCode = 1;
        return;
      }

      const target = parseSkillReadTarget(rawInput);
      if (!target) {
        console.log();
        console.log(
          `  ${c('red', '✗')} Cannot parse path: ${c('yellow', rawInput)}`
        );
        console.log(
          `  ${dim('Expected:')} /local/path, ./relative, owner/repo/path, or https://github.com/...`
        );
        console.log();
        process.exitCode = 1;
        return;
      }

      const spinner = jsonOutput
        ? null
        : new Spinner('Reading SKILL.md...').start();

      let content: string | null = null;
      let readError: string | null = null;
      let resolvedSource = '';

      try {
        if (target.type === 'local') {
          const skillMdPath = target.skillDir.endsWith('SKILL.md')
            ? target.skillDir
            : path.join(target.skillDir, 'SKILL.md');
          if (!fileExists(skillMdPath)) {
            throw new Error(`SKILL.md not found at ${skillMdPath}`);
          }
          content = readFileContent(skillMdPath) ?? null;
          resolvedSource = skillMdPath;
        } else {
          content = await readSkillFromGitHub(
            target.owner,
            target.repo,
            target.skillPath,
            target.branch
          );
          resolvedSource = `https://github.com/${target.owner}/${target.repo}/tree/${target.branch}/${target.skillPath}`;
        }
      } catch (err) {
        readError = err instanceof Error ? err.message : String(err);
      }

      spinner?.stop();

      if (readError || !content) {
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              success: false,
              error: readError ?? 'Empty content',
            })
          );
          process.exitCode = 1;
          return;
        }
        console.log();
        console.log(`  ${c('red', '✗')} ${readError ?? 'Empty content'}`);
        if (
          readError?.includes('not found') ||
          readError?.includes('tried main')
        ) {
          console.log(
            `  ${dim('This skill may have been moved or removed from the registry.')}`
          );
          const skillName =
            (rawInput as string)
              .replace(/\/SKILL\.md$/i, '')
              .split('/')
              .at(-1) ?? '';
          if (skillName) {
            console.log(
              `  ${dim('Search for it:')} octocode skills search "${skillName}" --direct`
            );
          }
        }
        console.log();
        process.exitCode = 1;
        return;
      }

      const meta = parseSkillFrontmatter(content);
      const skillName =
        meta?.name ??
        (target.type === 'local'
          ? pathBasename(target.skillDir)
          : (target.skillPath.split('/').pop() ?? target.repo));

      const truncated = !fullOutput && content.length > READ_TRUNCATE_CHARS;
      const displayContent = truncated
        ? content.slice(0, READ_TRUNCATE_CHARS)
        : content;

      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: true,
            name: skillName,
            description: meta?.description ?? null,
            source: resolvedSource,
            content: displayContent,
            truncated,
            totalChars: content.length,
          })
        );
        return;
      }

      console.log();
      console.log(`  ${bold(skillName)}`);
      if (meta?.description) {
        console.log(`  ${dim(meta.description)}`);
      }
      console.log(`  ${dim(resolvedSource)}`);
      console.log();
      console.log(
        `  ${c('cyan', bold('── SKILL.md ─────────────────────────────────'))}`
      );
      console.log();
      console.log(displayContent);
      if (truncated) {
        console.log();
        console.log(
          `  ${dim(`... (${content.length - READ_TRUNCATE_CHARS} more chars — use --full to show all)`)}`
        );
      }
      console.log();
      return;
    }

    if (subcommand === 'search') {
      const query =
        args.args[1] ||
        (args.options['query'] as string) ||
        (args.options['q'] as string);
      const isHumanTTY = process.stdout.isTTY === true;
      const directMode = Boolean(args.options['direct']) || isHumanTTY;
      const rawLimit = args.options['limit'] ?? args.options['l'];
      const limit =
        typeof rawLimit === 'string' && /^\d+$/.test(rawLimit)
          ? Math.max(1, Math.min(100, parseInt(rawLimit, 10)))
          : 20;

      if (!query || typeof query !== 'string') {
        console.log();
        console.log(`  ${c('red', '✗')} Missing search query`);
        console.log(
          `  ${dim('Usage:')} octocode skills search <query> [--direct] [--json] [--limit N]`
        );
        console.log(
          `  ${dim('Example:')} octocode skills search "code review" --direct`
        );
        console.log();
        process.exitCode = 1;
        return;
      }

      const SEARCH_SKILL_URL =
        'https://github.com/bgauryy/octocode/blob/main/skills/octocode-search-skill/SKILL.md';
      const SEARCH_SKILL_RAW =
        'https://raw.githubusercontent.com/bgauryy/octocode/main/skills/octocode-search-skill/SKILL.md';
      const SEARCH_SKILL_REFS =
        'https://github.com/bgauryy/octocode/tree/main/skills/octocode-search-skill/references';

      if (directMode) {
        const spinner = jsonOutput
          ? null
          : new Spinner(`Searching skills.sh for "${query}"...`).start();

        let webData: Awaited<ReturnType<typeof fetchSkillsShSearch>> | null =
          null;
        let webError: string | null = null;
        try {
          webData = await fetchSkillsShSearch(query, limit);
        } catch (err) {
          webError = String(err);
        }

        spinner?.stop();

        if (jsonOutput) {
          console.log(
            JSON.stringify({
              query,
              source: 'skills.sh',
              count: webData?.count ?? 0,
              results: (webData?.results ?? []).map((r: SkillsShResult) => ({
                name: r.name,
                skillId: r.skillId,
                repo: r.source,
                installs: r.installs,
                url: `https://github.com/${r.source}`,
                readCmd: `octocode skills read ${r.source}/${r.skillId}/SKILL.md`,
                installCmd: `octocode skills install --local ${r.source}/${r.skillId}`,
              })),
              error: webError ?? undefined,
            })
          );
          return;
        }

        console.log();
        console.log(`  ${bold(`Skill Search: "${query}"`)}`);

        if (!webData || webData.results.length === 0) {
          console.log();
          console.log(
            webError
              ? `  ${c('yellow', 'WARN')} skills.sh unavailable: ${webError}`
              : `  ${dim(`No results for "${query}" — try broader terms`)}`
          );
          console.log();
          return;
        }

        const byRepo = new Map<string, SkillsShResult[]>();
        for (const r of webData.results) {
          const bucket = byRepo.get(r.source) ?? [];
          bucket.push(r);
          byRepo.set(r.source, bucket);
        }
        const sortedRepos = [...byRepo.entries()].sort(
          (a, b) =>
            b[1].reduce((s, r) => s + r.installs, 0) -
            a[1].reduce((s, r) => s + r.installs, 0)
        );

        console.log(
          `  ${dim(`${webData.count} results from skills.sh — grouped by repo`)}`
        );

        for (const [repo, skills] of sortedRepos) {
          const repoTotal = skills.reduce((s, r) => s + r.installs, 0);
          console.log();
          console.log(
            `  ${c('cyan', bold(repo))}  ${dim(`(${repoTotal.toLocaleString()} total installs)`)}`
          );
          for (const r of skills.sort((a, b) => b.installs - a.installs)) {
            const installsStr =
              r.installs > 0
                ? dim(` · ${r.installs.toLocaleString()} installs`)
                : '';
            console.log(`    ${c('green', '•')} ${bold(r.name)}${installsStr}`);
            console.log(
              `      ${dim('read:')} octocode skills read ${r.source}/${r.skillId}/SKILL.md`
            );
          }
        }

        if (installTopResult && webData.results.length > 0) {
          const topAll = webData.results.sort(
            (a, b) => b.installs - a.installs
          );
          const top = topAll[0];
          const skillPath = `${top.source}/${top.skillId}`;
          console.log();
          console.log(
            `  ${bold('Auto-installing top result:')} ${c('cyan', top.name)}`
          );
          console.log(`  ${dim('Source:')} https://github.com/${skillPath}`);
          console.log();

          const installSpinner = new Spinner(
            `Fetching SKILL.md from ${skillPath}...`
          ).start();
          let installError: string | null = null;
          let skillContent: string | null = null;
          try {
            skillContent = await readSkillFromGitHub(
              top.source.split('/')[0],
              top.source.split('/').slice(1).join('/'),
              `${top.skillId}/SKILL.md`,
              'main'
            );
          } catch (err) {
            installError = String(err);
          }
          installSpinner.stop();

          if (installError || !skillContent) {
            console.log(
              `  ${c('red', '✗')} Could not fetch skill: ${installError ?? 'empty content'}`
            );
            console.log(
              `  ${dim('Try manually:')} octocode skills install --local <path>`
            );
          } else {
            const { mkdirSync: mkd, writeFileSync: wf } =
              await import('node:fs');
            let installed = 0;
            let failed = 0;
            for (const dest of targetDestinations) {
              const skillDir = path.join(dest.destDir, top.skillId);
              try {
                if (!force && dirExists(skillDir)) {
                  continue;
                }
                mkd(skillDir, { recursive: true, mode: 0o755 });
                wf(path.join(skillDir, 'SKILL.md'), skillContent, 'utf8');
                installed++;
              } catch (err) {
                console.log(
                  `  ${c('red', 'X')} Failed to write to ${skillDir}: ${err}`
                );
                failed++;
              }
            }
            if (failed === 0 && installed > 0) {
              console.log(
                `  ${c('green', '✅')} Installed ${top.name} to ${installed} target(s)`
              );
            } else if (installed === 0 && failed === 0) {
              console.log(
                `  ${c('yellow', '⚠')} Already installed in all targets — use --force to overwrite`
              );
            } else if (failed > 0) {
              console.log(
                `  ${c('red', 'X')} Installation failed for ${failed} target(s)`
              );
              process.exitCode = 1;
            }
          }
        }

        console.log();
        return;
      }

      if (jsonOutput) {
        console.log(
          JSON.stringify({
            query,
            instruction:
              'Read the skill protocol and all its referenced files, then execute the search for the given query.',
            skillProtocol: {
              url: SEARCH_SKILL_URL,
              raw: SEARCH_SKILL_RAW,
              references: SEARCH_SKILL_REFS,
            },
          })
        );
        return;
      }

      console.log();
      console.log(`  ${bold(`Skill Search: "${query}"`)}`);
      console.log(
        `  ${dim('This is the agent search protocol. For human-readable results now, add --direct.')}`
      );
      console.log();
      console.log(
        `  Read and follow the search protocol, then search for: ${c('cyan', bold(query))}`
      );
      console.log();
      console.log(`  ${bold('Protocol:')}  ${SEARCH_SKILL_URL}`);
      console.log(`  ${bold('References:')} ${SEARCH_SKILL_REFS}`);
      console.log();
      console.log(
        `  ${dim('For immediate results:  octocode skills search <query> --direct')}`
      );

      console.log();
      return;
    }

    if (subcommand === 'list') {
      if (targetFilter) {
        const valid = SKILL_INSTALL_TARGETS.includes(
          targetFilter as (typeof SKILL_INSTALL_TARGETS)[number]
        );
        if (!valid) {
          if (jsonOutput) {
            console.log(
              JSON.stringify({
                error: `Invalid target: ${targetFilter}. Valid: ${formatSkillInstallTargets()}`,
              })
            );
          } else {
            console.log();
            console.log(`  ${c('red', '✗')} Invalid --target: ${targetFilter}`);
            console.log(
              `  ${dim('Valid targets:')} ${formatSkillInstallTargets()}`
            );
            console.log();
          }
          process.exitCode = 1;
          return;
        }
      }

      const targetsToScan = targetFilter
        ? SKILL_INSTALL_TARGETS.filter(t => t === targetFilter)
        : SKILL_INSTALL_TARGETS;

      const allTargets = targetsToScan.map(target => {
        const destDir = getSkillsDirForTarget(target);
        const exists = dirExists(destDir);
        const skills = exists
          ? listSubdirectories(destDir)
              .filter(folder => isSafeSkillName(folder))
              .map(folder => {
                const skillDir = path.join(destDir, folder);
                const meta = getSkillMetadata(skillDir);
                const fullDesc = meta?.description || null;
                const description =
                  fullDesc && fullDesc.length > 200
                    ? `${fullDesc.slice(0, 200)}…`
                    : fullDesc;
                return {
                  folder,
                  name: meta?.name || folder,
                  description,
                  path: path.join(skillDir, 'SKILL.md'),
                };
              })
          : [];
        return { target, destDir, exists, skills };
      });

      if (jsonOutput) {
        console.log(JSON.stringify({ targets: allTargets }));
        return;
      }

      const totalSkills = allTargets.reduce((n, t) => n + t.skills.length, 0);
      console.log();
      console.log(
        `  ${bold('Skills on OS')}  ${dim(`(${totalSkills} total)`)}`
      );

      for (const t of allTargets) {
        console.log();
        console.log(`  ${c('cyan', bold(t.target))}  ${dim(t.destDir)}`);
        if (!t.exists) {
          console.log(`    ${dim('(directory not found)')}`);
          continue;
        }
        if (t.skills.length === 0) {
          console.log(`    ${dim('(no skills installed)')}`);
          continue;
        }
        for (const skill of t.skills) {
          const title =
            skill.name !== skill.folder
              ? `${bold(skill.name)} ${dim(`(${skill.folder})`)}`
              : bold(skill.folder);
          console.log(`    ${c('green', '•')} ${title}`);
          if (skill.description) {
            const rawDesc = skill.description;
            const truncDesc =
              rawDesc.length > 200 ? `${rawDesc.slice(0, 200)}…` : rawDesc;
            console.log(`      ${dim(truncDesc)}`);
          }
          console.log(`      ${dim(skill.path)}`);
        }
      }

      console.log();
      if (availableSkills.length > 0) {
        console.log(
          `  ${dim('Available to install:')} ${availableSkills.join(', ')}`
        );
        console.log(
          `  ${dim('Install:')} octocode skills install --targets <targets>`
        );
      }
      console.log();
      return;
    }

    if (subcommand === 'install') {
      if (localPath) {
        const absLocal = resolvePath(
          localPath.startsWith('~/')
            ? localPath.replace('~/', `${HOME}/`)
            : localPath
        );
        const skillMdPath = absLocal.endsWith('SKILL.md')
          ? absLocal
          : path.join(absLocal, 'SKILL.md');
        const localSkillDir = skillMdPath.replace(/\/SKILL\.md$/, '');
        const localSkillName = pathBasename(localSkillDir);

        if (!fileExists(skillMdPath)) {
          if (jsonOutput) {
            console.log(
              JSON.stringify({
                success: false,
                error: `SKILL.md not found at ${skillMdPath}`,
              })
            );
          } else {
            console.log();
            console.log(
              `  ${c('red', '✗')} SKILL.md not found at ${skillMdPath}`
            );
            console.log();
          }
          process.exitCode = 1;
          return;
        }

        const spinner = jsonOutput
          ? null
          : new Spinner(
              `Installing ${localSkillName} from local path...`
            ).start();

        const summary = installAllSkillsForTargets({
          skillNames: [localSkillName],
          sourceDir: path.dirname(localSkillDir),
          destinations: targetDestinations,
          strategy: installStrategy,
          force,
        });

        if (jsonOutput) {
          spinner?.stop();
          console.log(
            JSON.stringify({
              installed: summary.installed,
              skipped: summary.skipped,
              failed: summary.failed,
              skill: localSkillName,
              source: absLocal,
              targets: targetDestinations.map(d => d.target),
            })
          );
          if (summary.failed > 0) process.exitCode = 1;
          return;
        }

        if (summary.failed === 0) {
          spinner?.succeed(`Installed ${localSkillName}!`);
          console.log();
          console.log(
            `  ${c('green', '✅')} Installed to ${summary.installed}/${summary.targetCount} targets`
          );
        } else {
          spinner?.fail(`Failed to install ${localSkillName}`);
          process.exitCode = 1;
        }
        console.log();
        return;
      }

      if (specificSkill) {
        console.log();
        console.log(`  ${bold(`Installing skill: ${specificSkill}`)}`);
        console.log();
        if (!availableSkills.includes(specificSkill)) {
          console.log(`  ${c('red', '✗')} Skill not found: ${specificSkill}`);
          console.log();
          console.log(`  ${dim('Available skills:')}`);
          for (const s of availableSkills) {
            console.log(`    ${c('cyan', '•')} ${s}`);
          }
          console.log();
          process.exitCode = 1;
          return;
        }
        const spinner = jsonOutput
          ? null
          : new Spinner(`Installing ${specificSkill}...`).start();
        const summary = installSkillForTargets({
          skillName: specificSkill,
          sourceDir: srcDir,
          destinations: targetDestinations,
          strategy: installStrategy,
          force,
        });
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              installed: summary.installed,
              skipped: summary.skipped,
              failed: summary.failed,
              skill: specificSkill,
              targets: targetDestinations.map(d => d.target),
            })
          );
          if (summary.failed > 0) process.exitCode = 1;
          return;
        }
        if (summary.failed === 0) {
          spinner?.succeed(`Installed ${specificSkill}!`);
          console.log();
          console.log(
            `  ${c('green', '✅')} Installed to ${summary.installed}/${summary.targetCount} targets`
          );
          for (const destination of targetDestinations) {
            console.log(
              `    ${c('cyan', '•')} ${destination.target}: ${path.join(destination.destDir, specificSkill)}`
            );
          }
          if (summary.skipped > 0) {
            console.log(
              `  ${c('yellow', 'WARN')} Skipped ${summary.skipped} existing target(s) ${dim('(use --force to overwrite)')}`
            );
          }
        } else {
          spinner?.fail(`Failed to install ${specificSkill}`);
          process.exitCode = 1;
        }
        console.log();
        return;
      }

      if (!jsonOutput) {
        console.log();
        console.log(`  ${bold('Installing Octocode Skills')}`);
        console.log();
      }
      if (availableSkills.length === 0) {
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              dryRun,
              skills: [],
              targets: targetDestinations.map(d => d.target),
              plan: [],
            })
          );
        } else {
          console.log(`  ${c('yellow', '⚠')} No skills to install.`);
          console.log();
        }
        return;
      }

      if (dryRun) {
        const dryResult = availableSkills.flatMap(skill =>
          targetDestinations.map(d => ({
            skill,
            target: d.target,
            dest: path.join(d.destDir, skill),
            exists: dirExists(path.join(d.destDir, skill)),
          }))
        );
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              dryRun: true,
              skills: availableSkills,
              targets: targetDestinations.map(d => d.target),
              plan: dryResult,
            })
          );
          return;
        }
        console.log(
          `  ${c('yellow', 'DRY RUN')}  ${dim('(no files written)')}`
        );
        console.log();
        for (const item of dryResult) {
          const status = item.exists
            ? force
              ? c('yellow', 'overwrite')
              : c('dim', 'skip (exists)')
            : c('green', 'install');
          console.log(
            `  ${c('cyan', item.target)} / ${bold(item.skill)}  ${status}`
          );
          console.log(`    ${dim(item.dest)}`);
        }
        console.log();
        console.log(`  ${dim('Remove --dry-run to apply.')}`);
        console.log();
        return;
      }

      const spinner = jsonOutput
        ? null
        : new Spinner('Installing skills...').start();
      const summary = installAllSkillsForTargets({
        skillNames: availableSkills,
        sourceDir: srcDir,
        destinations: targetDestinations,
        strategy: installStrategy,
        force,
      });
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            installed: summary.installed,
            skipped: summary.skipped,
            failed: summary.failed,
            targets: targetDestinations.map(d => d.target),
          })
        );
        if (summary.failed > 0) process.exitCode = 1;
        return;
      }
      if (summary.failed === 0) {
        spinner?.succeed('Skills installation complete!');
      } else {
        spinner?.fail('Skills installation completed with errors');
      }
      console.log();
      if (summary.installed > 0) {
        console.log(
          `  ${c('green', '✅')} Installed ${summary.installed} skill target(s)`
        );
      }
      if (summary.skipped > 0) {
        console.log(
          `  ${c('yellow', 'WARN')} Skipped ${summary.skipped} existing skill target(s)`
        );
        console.log(
          `  ${dim('Use')} ${c('cyan', '--force')} ${dim('to overwrite.')}`
        );
      }
      if (summary.failed > 0) {
        console.log(
          `  ${c('red', 'X')} Failed ${summary.failed} skill target(s)`
        );
        process.exitCode = 1;
      }
      console.log();
      console.log(`  ${bold('Targets:')}`);
      for (const destination of targetDestinations) {
        console.log(
          `    ${c('cyan', '•')} ${destination.target}: ${destination.destDir}`
        );
      }
      console.log();
      console.log(`  ${bold('Skills installation finished.')}`);
      console.log();
      return;
    }

    if (subcommand === 'remove') {
      const effectiveSkill =
        specificSkill ??
        (localPath
          ? pathBasename(
              resolvePath(
                localPath.startsWith('~/')
                  ? localPath.replace('~/', `${HOME}/`)
                  : localPath
              ).replace(/\/SKILL\.md$/, '')
            )
          : undefined);

      if (!effectiveSkill) {
        console.log();
        console.log(
          `  ${c('red', 'X')} Missing required option: ${c('cyan', '--skill <name>')} or ${c('cyan', '--local <path>')}`
        );
        console.log();
        console.log(`  ${dim('Usage:')} octocode skills remove --skill <name>`);
        console.log(
          `  ${dim('   or:')} octocode skills remove --local ./my-skill`
        );
        console.log();
        process.exitCode = 1;
        return;
      }
      console.log();
      console.log(`  ${bold(`Removing skill: ${effectiveSkill}`)}`);
      console.log();
      const summary = removeSkillFromTargets({
        skillName: effectiveSkill,
        destinations: targetDestinations,
      });

      const invalidSkillName = summary.failures.some(
        failure => failure.reason === 'invalid-skill-name'
      );
      if (invalidSkillName) {
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              removed: 0,
              missing: 0,
              failed: 1,
              targets: targetDestinations.map(d => d.target),
              error: `Invalid skill name: ${effectiveSkill}`,
            })
          );
          process.exitCode = 1;
          return;
        }
        console.log(`  ${c('red', 'X')} Invalid skill name: ${effectiveSkill}`);
        process.exitCode = 1;
        return;
      }

      if (jsonOutput) {
        console.log(
          JSON.stringify({
            removed: summary.removed,
            missing: summary.missing,
            failed: summary.failed,
            skill: effectiveSkill,
            targets: targetDestinations.map(d => d.target),
          })
        );
        if (summary.failed > 0) process.exitCode = 1;
        return;
      }

      for (const failure of summary.failures) {
        if (failure.reason === 'remove-failed') {
          console.log(
            `  ${c('red', 'X')} Failed to remove from ${failure.target}: ${failure.path}`
          );
        }
      }
      if (summary.removed > 0) {
        console.log(
          `  ${c('green', '✅')} Removed from ${summary.removed}/${summary.targetCount} targets`
        );
      }
      if (summary.missing > 0) {
        console.log(
          `  ${c('yellow', 'WARN')} Not found in ${summary.missing} target(s) ${dim('(already absent)')}`
        );
      }
      if (summary.failed > 0) {
        process.exitCode = 1;
      }
      console.log();
      return;
    }

    if (subcommand === 'sync') {
      const fromTarget = args.args[1];
      const toTarget = args.args[2];

      if (!fromTarget || !toTarget) {
        const msg = 'Usage: octocode skills sync <from-target> <to-target>';
        if (jsonOutput) {
          console.log(JSON.stringify({ success: false, error: msg }));
        } else {
          console.log();
          console.log(`  ${c('red', '✗')} ${msg}`);
          console.log(
            `  ${dim('Example:')} octocode skills sync cursor agents`
          );
          console.log(`  ${dim('Targets:')} ${formatSkillInstallTargets()}`);
          console.log();
        }
        process.exitCode = 1;
        return;
      }

      const fromNorm = SKILL_INSTALL_TARGETS.find(
        t => t === fromTarget.toLowerCase()
      );
      const toNorm = SKILL_INSTALL_TARGETS.find(
        t => t === toTarget.toLowerCase()
      );
      const invalidTargets = [
        !fromNorm && fromTarget,
        !toNorm && toTarget,
      ].filter(Boolean);

      if (invalidTargets.length > 0) {
        const msg = `Invalid target(s): ${invalidTargets.join(', ')}. Valid: ${formatSkillInstallTargets()}`;
        if (jsonOutput) {
          console.log(JSON.stringify({ success: false, error: msg }));
        } else {
          console.log();
          console.log(`  ${c('red', '✗')} ${msg}`);
          console.log();
        }
        process.exitCode = 1;
        return;
      }

      const fromDir = getSkillsDirForTarget(fromNorm!);
      const toDir = getSkillsDirForTarget(toNorm!);

      if (!dirExists(fromDir)) {
        const msg = `Source target has no skills: ${fromNorm} (${fromDir})`;
        if (jsonOutput) {
          console.log(JSON.stringify({ success: false, error: msg }));
        } else {
          console.log();
          console.log(`  ${c('red', '✗')} ${msg}`);
          console.log();
        }
        process.exitCode = 1;
        return;
      }

      const skillsInFrom = listSubdirectories(fromDir).filter(isSafeSkillName);

      if (dryRun) {
        const plan = skillsInFrom.map(skill => ({
          skill,
          from: path.join(fromDir, skill),
          to: path.join(toDir, skill),
          exists: dirExists(path.join(toDir, skill)),
        }));
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              dryRun: true,
              from: fromNorm,
              to: toNorm,
              skills: skillsInFrom,
              plan,
            })
          );
          return;
        }
        console.log();
        console.log(
          `  ${c('yellow', 'DRY RUN')} sync ${c('cyan', fromNorm!)} → ${c('cyan', toNorm!)}  ${dim('(no files written)')}`
        );
        console.log();
        for (const item of plan) {
          const status = item.exists
            ? force
              ? c('yellow', 'overwrite')
              : c('dim', 'skip (exists)')
            : c('green', 'copy');
          console.log(`  ${c('green', '•')} ${bold(item.skill)}  ${status}`);
        }
        console.log();
        return;
      }

      const spinner = jsonOutput
        ? null
        : new Spinner(
            `Syncing ${skillsInFrom.length} skills from ${fromNorm} → ${toNorm}...`
          ).start();

      const syncDestinations = getSkillTargetDestinations(
        [toNorm!],
        getSkillsDestDir()
      );
      const summary = installAllSkillsForTargets({
        skillNames: skillsInFrom,
        sourceDir: fromDir,
        destinations: syncDestinations,
        strategy: 'copy',
        force,
      });

      spinner?.stop();

      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: summary.failed === 0,
            from: fromNorm,
            to: toNorm,
            installed: summary.installed,
            skipped: summary.skipped,
            failed: summary.failed,
            skills: skillsInFrom,
          })
        );
        if (summary.failed > 0) process.exitCode = 1;
        return;
      }

      console.log();
      console.log(
        `  ${c('green', '✅')} Synced ${summary.installed} skill(s) from ${c('cyan', fromNorm!)} → ${c('cyan', toNorm!)}`
      );
      if (summary.skipped > 0) {
        console.log(
          `  ${c('yellow', 'WARN')} Skipped ${summary.skipped} (already exists — use --force to overwrite)`
        );
      }
      if (summary.failed > 0) {
        console.log(`  ${c('red', 'X')} Failed ${summary.failed}`);
        process.exitCode = 1;
      }
      console.log();
      return;
    }

    console.log();
    console.log(`  ${c('red', '✗')} Unknown subcommand: ${subcommand}`);
    console.log(
      `  ${dim('Usage:')} octocode skills [search|read|install|remove|list|sync]`
    );
    console.log();
    process.exitCode = 1;
  },
};
