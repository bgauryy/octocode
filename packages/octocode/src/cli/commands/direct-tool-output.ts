import { c, dim } from '../../utils/colors.js';
import { classifyToolErrorText } from '../exit-codes.js';

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
    // Match `tools <name> --json`: JSON always means the full CallToolResult
    // envelope. Callers that want structuredContent alone use `tools --compact`.
    console.log(JSON.stringify(result, null, 2));
    return;
  }

  const text = getDirectToolText(result);
  if (result.isError) {
    console.error();
    console.error(`  ${c('red', '✗')} Tool execution failed.`);
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
    // Read the failure text so auth/rate-limit errors map to the correct exit
    // code instead of a blanket TOOL — mirrors tool-command.ts.
    process.exitCode = classifyToolErrorText(getDirectToolText(result));
  }
}
