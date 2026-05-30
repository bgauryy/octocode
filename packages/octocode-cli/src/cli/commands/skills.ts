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
} from '../../utils/skills.js';
import {
  getAvailableSkillNames,
  getSkillTargetDestinations,
  installAllSkillsForTargets,
  installSkillForTargets,
  parseSkillTargetList,
  removeSkillFromTargets,
} from '../../features/skills.js';
import { loadInquirer, select, checkbox } from '../../utils/prompts.js';
import { Spinner } from '../../utils/spinner.js';
import path from 'node:path';
import {
  type SkillInstallMode,
  type SkillInstallStrategy,
  type SkillInstallTarget,
} from './shared.js';

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

export const skillsCommand: CLICommand = {
  name: 'skills',
  aliases: ['sk'],
  description: 'Install Octocode skills across AI clients',
  usage:
    'octocode-cli skills [install|remove|list] [--skill <name>] [--targets <list>] [--mode <copy|symlink>]',
  options: [
    { name: 'force', short: 'f', description: 'Overwrite existing skills' },
    {
      name: 'skill',
      short: 'k',
      description: 'Skill folder name (used by install/remove)',
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
  ],
  handler: async (args: ParsedArgs) => {
    const subcommand = args.args[0] || 'list';
    const force = Boolean(args.options['force'] || args.options['f']);
    const rawSkill = args.options['skill'] ?? args.options['k'];
    const specificSkill =
      typeof rawSkill === 'string' && rawSkill.length > 0
        ? rawSkill
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
          `  ${dim('Example:')} octocode-cli skills install --mode symlink`
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

    if (!dirExists(srcDir)) {
      console.log();
      console.log(`  ${c('red', '✗')} Skills directory not found`);
      console.log(`  ${dim('Expected:')} ${srcDir}`);
      console.log();
      process.exitCode = 1;
      return;
    }

    const availableSkills = getAvailableSkillNames(srcDir);

    if (subcommand === 'list') {
      console.log();
      console.log(`  ${bold('Available Octocode Skills')}`);
      console.log();
      console.log(`  ${bold('Install destinations:')}`);
      for (const destination of targetDestinations) {
        console.log(
          `    ${c('cyan', '•')} ${destination.target}: ${destination.destDir}`
        );
      }
      console.log();
      if (availableSkills.length === 0) {
        console.log(`  ${dim('No skills available.')}`);
      } else {
        for (const skill of availableSkills) {
          const installed = targetDestinations.every(destination =>
            dirExists(path.join(destination.destDir, skill))
          );
          const status = installed
            ? c('green', 'installed')
            : dim('not installed');
          console.log(`  ${c('cyan', '•')} ${skill} ${status}`);
        }
      }
      console.log();
      console.log(`  ${dim('To install all:')} octocode-cli skills install`);
      console.log(
        `  ${dim('To install one:')} octocode-cli skills install --skill <name> ${dim('(or -k <name>)')}`
      );
      console.log(
        `  ${dim('Multi-install:')} octocode-cli skills install --targets claude-code,cursor,codex --mode symlink`
      );
      console.log();
      return;
    }

    if (subcommand === 'install') {
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
        const spinner = new Spinner(`Installing ${specificSkill}...`).start();
        const summary = installSkillForTargets({
          skillName: specificSkill,
          sourceDir: srcDir,
          destinations: targetDestinations,
          strategy: installStrategy,
          force,
        });
        if (summary.failed === 0) {
          spinner.succeed(`Installed ${specificSkill}!`);
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
          spinner.fail(`Failed to install ${specificSkill}`);
          process.exitCode = 1;
        }
        console.log();
        return;
      }

      console.log();
      console.log(`  ${bold('Installing Octocode Skills')}`);
      console.log();
      if (availableSkills.length === 0) {
        console.log(`  ${c('yellow', '⚠')} No skills to install.`);
        console.log();
        return;
      }
      const spinner = new Spinner('Installing skills...').start();
      const summary = installAllSkillsForTargets({
        skillNames: availableSkills,
        sourceDir: srcDir,
        destinations: targetDestinations,
        strategy: installStrategy,
        force,
      });
      if (summary.failed === 0) {
        spinner.succeed('Skills installation complete!');
      } else {
        spinner.fail('Skills installation completed with errors');
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
      if (!specificSkill) {
        console.log();
        console.log(
          `  ${c('red', 'X')} Missing required option: ${c('cyan', '--skill <name>')}`
        );
        console.log();
        console.log(
          `  ${dim('Usage:')} octocode-cli skills remove --skill <name>`
        );
        console.log();
        process.exitCode = 1;
        return;
      }
      console.log();
      console.log(`  ${bold(`Removing skill: ${specificSkill}`)}`);
      console.log();
      const summary = removeSkillFromTargets({
        skillName: specificSkill,
        destinations: targetDestinations,
      });

      const invalidSkillName = summary.failures.some(
        failure => failure.reason === 'invalid-skill-name'
      );
      if (invalidSkillName) {
        console.log(`  ${c('red', 'X')} Invalid skill name: ${specificSkill}`);
        process.exitCode = 1;
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

    console.log();
    console.log(`  ${c('red', '✗')} Unknown subcommand: ${subcommand}`);
    console.log(
      `  ${dim('Usage:')} octocode-cli skills [install|remove|list] [--skill <name>]`
    );
    console.log();
    process.exitCode = 1;
  },
};
