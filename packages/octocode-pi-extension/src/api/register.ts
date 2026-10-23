import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { OctocodeApi, type AgentDirectory, type ApiOperations } from './bridge.js';
import { optionsFromEnv, transportsFor } from './options.js';
import { notify, say, wordCompletions, type Subcommands } from '../shared/commands.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { errorMessage } from '../shared/util.js';

/** Wire the external API into Pi: event publishing, lifecycle and `/octocode api`. Subagents never expose one. */
export function registerApi(pi: ExtensionAPI, commands: Subcommands, agents: AgentDirectory, version: () => string | undefined, operations?: ApiOperations): { api: OctocodeApi; start: (ctx: Parameters<OctocodeApi['start']>[0]) => Promise<void>; stop: () => Promise<void> } {
  const api = new OctocodeApi(pi, agents, version, process.env, operations);
  api.observe();

  commands.add('api', {
    description: 'api [on [http] | off] — external API status, start or stop',
    complete: (prefix) => wordCompletions(['on', 'on http', 'off'], prefix),
    handler: async (args, ctx) => {
      const line = args.trim();
      if (line !== '' && !/^(?:on(?:\s+http)?|off)$/.test(line)) return say(ctx, 'Usage: /octocode api [on [http] | off]', 'warning');
      const [action, flag] = line.split(/\s+/);
      try {
        if (action === 'on') {
          await api.start(ctx, transportsFor(flag === 'http' ? 0 : undefined));
        } else if (action === 'off') {
          await api.stop();
          // stop() unbinds the session; bind again so a later `api on` works.
          await api.start(ctx, undefined);
        }
      } catch (error) {
        say(ctx, errorMessage(error), 'error');
        return;
      }
      const info = api.instance;
      say(ctx,
        info
          ? [`Octocode API ${info.id}`, info.socket ? `socket: ${info.socket}` : '', info.http ? `http: ${info.http.url} (token in the instance file)` : '', `Discovery: ${api.directory}/instances/${info.id}.json`].filter(Boolean).join('\n')
          : 'Octocode API is off. Start with /octocode api on, or set OCTOCODE_API=1 (OCTOCODE_API_HTTP=<port> adds loopback HTTP).',
        'info',
      );
    },
  });

  return {
    api,
    start: async (ctx) => {
      // A bad environment still binds the session (so `/octocode api on` works) and says why the API is off.
      let options: ReturnType<typeof optionsFromEnv>;
      let failure: unknown;
      try {
        options = optionsFromEnv();
      } catch (error) {
        failure = error;
      }
      try {
        await api.start(ctx, options);
      } catch (error) {
        failure = error;
      }
      if (failure === undefined) return;
      // Print and JSON modes have no UI to notify: the reason goes to stderr (see `say`) so a headless run knows why the API is off.
      if (ctx.hasUI) notify(ctx.ui, errorMessage(failure), 'warning');
      else process.stderr.write(`${sanitizeTerminalText(errorMessage(failure))}\n`);
    },
    stop: () => api.stop(),
  };
}
