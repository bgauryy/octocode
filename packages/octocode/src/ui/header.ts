import { c, bold, dim } from '../utils/colors.js';
import { getAppContext } from '../utils/context.js';

function printLogo(): void {
  const logo = [
    '        ▄▄██████▄▄',
    '      ▄██████████████▄',
    '     ▐████████████████▌',
    '     ▐██▀  ▀████▀  ▀██▌',
    '     ▐██  ▄ ████ ▄  ██▌',
    '     ▐████▄▄▀▀▀▀▄▄████▌',
    '      ▀██████████████▀',
    '    ▄▄▄████▀▀  ▀▀████▄▄▄',
    ' ▄████▀▀▄▄▄██████▄▄▄▀▀████▄',
    '▐██▌  ▄██▀▀      ▀▀██▄  ▐██▌',
    ' ▀▀  ▐██▌          ▐██▌  ▀▀',
    '      ▀▀            ▀▀',
  ];

  for (const line of logo) {
    console.log(c('magenta', '  ' + line));
  }
}

function printTitle(): void {
  const title = [
    ' ██████╗  ██████╗████████╗ ██████╗  ██████╗ ██████╗ ██████╗ ███████╗',
    '██╔═══██╗██╔════╝╚══██╔══╝██╔═══██╗██╔════╝██╔═══██╗██╔══██╗██╔════╝',
    '██║   ██║██║        ██║   ██║   ██║██║     ██║   ██║██║  ██║█████╗  ',
    '██║   ██║██║        ██║   ██║   ██║██║     ██║   ██║██║  ██║██╔══╝  ',
    '╚██████╔╝╚██████╗   ██║   ╚██████╔╝╚██████╗╚██████╔╝██████╔╝███████╗',
    ' ╚═════╝  ╚═════╝   ╚═╝    ╚═════╝  ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝',
  ];

  for (const line of title) {
    console.log(c('magenta', ' ' + line));
  }
}

export function printWelcome(): void {
  console.log();
  printLogo();
  console.log();
  printTitle();
  console.log();
  console.log();

  try {
    const ctx = getAppContext();

    const isIDE = ctx.ide === 'Cursor' || ctx.ide === 'VS Code';
    const pathPart = `${dim('Path')} ${ctx.cwd}`;
    const idePart = isIDE ? `${dim('IDE')} ${bold(ctx.ide)}` : '';
    const gitPart = ctx.git
      ? `${ctx.git.root} ${dim('(')}${ctx.git.branch}${dim(')')}`
      : '';
    const tail = [idePart, gitPart].filter(Boolean).join('    ');
    console.log(`  ${pathPart}${tail ? `    ${tail}` : ''}`);
    console.log();
  } catch {
    console.log();
  }
}

export function printGoodbye(): void {
  console.log();
  console.log(
    `  ${c('cyan', 'Tip')} ${bold('Quick tips for better AI coding with Octocode:')}`
  );
  console.log();
  console.log(
    `     ${c('green', '-')} ${dim('Prompts:')}  Use ${c('cyan', '/research')}, ${c('cyan', '/plan')}, ${c('cyan', '/implement')} in chat`
  );
  console.log(
    `     ${c('green', '-')} ${dim('Context:')}  Add ${c('cyan', 'AGENTS.md')} to your project ${dim('(you can ask octocode)')}`
  );
  console.log();
  console.log(`  Search${c('underscore', 'https://octocode.ai')}`);
  console.log();
}
