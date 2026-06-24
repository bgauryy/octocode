import { c, bold, dim } from '../utils/colors.js';
import type { CLICommandSpec } from './types.js';

export function showCommandHelp(command: CLICommandSpec): void {
  const lines = [
    '',
    `  ${c('magenta', bold('🔍🐙 octocode ' + command.name))}`,
    '',
    `  ${command.description}`,
    '',
  ];

  if (command.usage) {
    lines.push(`  ${bold('USAGE')}`);
    lines.push(`    ${command.usage}`);
    lines.push('');
  }

  if (command.scheme && command.scheme.length > 0) {
    lines.push(`  ${bold('SCHEME')}`);
    for (const item of command.scheme) {
      lines.push(`    ${item}`);
    }
    lines.push('');
  }

  if (command.whenToUse && command.whenToUse.length > 0) {
    lines.push(`  ${bold('WHEN TO USE')}`);
    for (const item of command.whenToUse) {
      lines.push(`    - ${item}`);
    }
    lines.push('');
  }

  if (command.examples && command.examples.length > 0) {
    lines.push(`  ${bold('EXAMPLES')}`);
    for (const example of command.examples) {
      lines.push(`    ${c('yellow', example)}`);
    }
    lines.push('');
  }

  if (command.options && command.options.length > 0) {
    lines.push(`  ${bold('OPTIONS')}`);
    // One line per option: `  --flag <value>   description (default: x)`.
    // Halves the section vs the old two-line form — material for commands like
    // `search` with 100+ flags.
    for (const opt of command.options) {
      const flagPart = `--${opt.name}${opt.hasValue ? ' <value>' : ''}`;
      const defaultHint =
        opt.default !== undefined ? dim(` (default: ${opt.default})`) : '';
      const pad =
        flagPart.length < 24 ? ' '.repeat(24 - flagPart.length) : '  ';
      lines.push(
        `    ${c('cyan', flagPart)}${pad}${opt.description}${defaultHint}`
      );
    }
    lines.push('');
  }

  process.stdout.write(`${lines.join('\n')}\n`);
}
