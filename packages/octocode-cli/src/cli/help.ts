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

  if (command.options && command.options.length > 0) {
    lines.push(`  ${bold('OPTIONS')}`);
    for (const opt of command.options) {
      const shortFlag = opt.short ? `-${opt.short}, ` : '    ';
      const longFlag = `--${opt.name}`;
      const valueHint = opt.hasValue ? ` <value>` : '';
      const defaultHint =
        opt.default !== undefined ? dim(` (default: ${opt.default})`) : '';
      lines.push(
        `    ${c('cyan', shortFlag + longFlag + valueHint)}${defaultHint}`
      );
      lines.push(`        ${opt.description}`);
    }
    lines.push('');
  }

  process.stdout.write(`${lines.join('\n')}\n`);
}
