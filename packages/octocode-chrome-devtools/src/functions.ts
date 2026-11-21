// Native functions: the same typed commands MCP and `/cli` serve, called in-process with no
// protocol in between. Input is validated by the command's own schema before the engine runs.
import { chromeCommands, chromeSchemaFor } from '@octocodeai/config/chrome';
import { invoke, stopEngine } from './invocation.js';

export type ChromeCommandName = keyof typeof chromeCommands;
export type CallOptions = { signal?: AbortSignal };

/** Validates `input` for the command, runs it and returns its capture reply; rejects with the failure reply. */
export async function call(
  name: ChromeCommandName,
  input: unknown,
  options: CallOptions = {}
) {
  return invoke(name, chromeSchemaFor(name).parse(input), options.signal);
}

/** One function per command: `chrome.open({...})`, `chrome.snapshot({...})`, `chrome.helper({ name: 'prune', args: [...] })`. */
export const chrome = Object.fromEntries(
  Object.keys(chromeCommands).map(name => [
    name,
    (input: unknown, options?: CallOptions) =>
      call(name as ChromeCommandName, input, options),
  ])
) as Record<
  ChromeCommandName,
  (input: unknown, options?: CallOptions) => ReturnType<typeof call>
>;

/** Stops engine children this process started; call when the work ends. */
export const closeChrome = stopEngine;
