import { bold, c, dim } from '../utils/colors.js';
import { input, loadInquirer } from '../utils/prompts.js';
import { executeToolCommand, showAvailableTools } from '../cli/tool-command.js';
import type { ParsedArgs } from '../cli/types.js';

type TerminalCommand =
  | { kind: 'empty' }
  | { kind: 'exit' }
  | { kind: 'list' }
  | { kind: 'execute'; toolName: string; inputText?: string };

function parseToolTerminalLine(line: string): TerminalCommand {
  const trimmed = line.trim();

  if (trimmed.length === 0) {
    return { kind: 'empty' };
  }

  if (['exit', 'quit', 'back'].includes(trimmed)) {
    return { kind: 'exit' };
  }

  if (trimmed === 'list' || trimmed === 'help') {
    return { kind: 'list' };
  }

  const separatorIndex = trimmed.search(/\s/);
  if (separatorIndex === -1) {
    return { kind: 'execute', toolName: trimmed };
  }

  return {
    kind: 'execute',
    toolName: trimmed.slice(0, separatorIndex),
    inputText: trimmed.slice(separatorIndex).trim(),
  };
}

function buildParsedArgs(toolName: string, inputText?: string): ParsedArgs {
  return {
    command: 'tool',
    args: inputText ? [toolName, inputText] : [toolName],
    options: {},
  };
}

export async function runToolTerminalFlow(): Promise<void> {
  await loadInquirer();

  console.log();
  console.log(`  ${c('magenta', bold('Tool Terminal'))}`);
  console.log(
    `  ${dim('Type')} ${c('cyan', 'list')} ${dim('for tools,')} ${c('cyan', '<toolName> {"path":".","keywords":"runCLI"}')} ${dim('to run, or')} ${c('cyan', 'exit')} ${dim('to return.')}`
  );

  await showAvailableTools();

  while (true) {
    const line = await input({
      message: c('cyan', 'tool>'),
      default: '',
    });

    const command = parseToolTerminalLine(line);

    if (command.kind === 'empty') {
      continue;
    }

    if (command.kind === 'exit') {
      console.log();
      return;
    }

    if (command.kind === 'list') {
      await showAvailableTools();
      continue;
    }

    await executeToolCommand(
      buildParsedArgs(command.toolName, command.inputText)
    );
    console.log();
  }
}
