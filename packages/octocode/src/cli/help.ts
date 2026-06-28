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
    // Render each ` | ` alternative on its own line so a multi-form usage reads
    // as a scannable list instead of one dense pipe-wall.
    for (const form of command.usage.split(' | ')) {
      lines.push(`    ${form.trim()}`);
    }
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
    // A flat dump of 100+ flags (e.g. `search`) is unreadable and buries the
    // common ones that USAGE/EXAMPLES already show. For such commands, point to
    // the authoritative typed list (`--scheme`) instead of dumping everything.
    const OPTIONS_DUMP_LIMIT = 40;
    if (command.options.length > OPTIONS_DUMP_LIMIT) {
      // Too many flags to read as name+description lines (e.g. `search` has
      // 100+). List the names in a compact wrapped grid so every flag is still
      // discoverable, and send agents to `--scheme` for types/bounds/defaults.
      lines.push(
        `    ${dim(`${command.options.length} flags — names below; full typed list (types, bounds, defaults):`)} ${c('cyan', `${command.name} --scheme`)}`
      );
      const names = command.options.map(o => `--${o.name}`);
      const perLine = 6;
      for (let i = 0; i < names.length; i += perLine) {
        lines.push(`    ${dim(names.slice(i, i + perLine).join('  '))}`);
      }
    } else {
      // One line per option: `  --flag <value>   description (default: x)`.
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
    }
    lines.push('');
  }

  process.stdout.write(`${lines.join('\n')}\n`);
}
