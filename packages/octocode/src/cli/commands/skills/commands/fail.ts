import { c } from '../../../../utils/colors.js';

/**
 * Report a skill command failure in the caller's output mode: the CLI-wide
 * `octocode.toolError` envelope on stdout with `--json`, else a human line.
 */
export function reportFailure(
  message: string,
  json: boolean,
  { human = `\n  ${c('red', '✗')} ${message}\n` }: { human?: string } = {}
): void {
  if (json)
    console.log(
      JSON.stringify({ kind: 'octocode.toolError', version: 1, error: message })
    );
  else console.error(human);
  process.exitCode = 1;
}
