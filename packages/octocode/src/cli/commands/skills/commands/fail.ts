import { c } from '../../../../utils/colors.js';

/**
 * Report a skill command failure on the channel the caller asked for:
 * the shared tool-error envelope, the command's JSON result, or a human line.
 */
export function reportFailure(
  message: string,
  json: boolean,
  jsonErrors = false,
  {
    okKey = 'success',
    human = `\n  ${c('red', '✗')} ${message}\n`,
  }: { okKey?: 'success' | 'ok'; human?: string } = {}
): void {
  if (jsonErrors)
    console.log(
      JSON.stringify({ kind: 'octocode.toolError', version: 1, error: message })
    );
  else if (json)
    console.log(JSON.stringify({ [okKey]: false, error: message }));
  else console.error(human);
  process.exitCode = 1;
}
