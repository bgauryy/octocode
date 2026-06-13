import { c, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';

type TextContent = {
  readonly type?: string;
  readonly text?: string;
};

type DirectToolResult = {
  readonly content?: readonly TextContent[];
  readonly structuredContent?: unknown;
  readonly isError?: boolean;
};

export function getDirectToolText(result: DirectToolResult): string {
  const text = (result.content ?? [])
    .filter(item => item.type === 'text' && typeof item.text === 'string')
    .map(item => item.text)
    .join('\n')
    .trim();

  if (text.length > 0) {
    return text;
  }

  return JSON.stringify(result.structuredContent ?? result, null, 2);
}

export function printDirectToolResult(
  result: DirectToolResult,
  jsonOutput: boolean
): void {
  if (jsonOutput) {
    console.log(JSON.stringify(result.structuredContent ?? result, null, 2));
    return;
  }

  const text = getDirectToolText(result);
  if (result.isError) {
    console.error();
    console.error(`  ${c('red', 'x')} Tool execution failed.`);
    console.error(`  ${dim(text)}`);
    console.error();
    return;
  }

  console.log();
  console.log(text);
  console.log();
}

export function markDirectToolFailure(result: DirectToolResult): void {
  if (result.isError) {
    process.exitCode = EXIT.TOOL;
  }
}
