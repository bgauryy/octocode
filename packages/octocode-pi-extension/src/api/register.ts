import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { OctocodeApi, optionsFromEnv, type AgentDirectory } from './bridge.js';
import { wordCompletions, type Subcommands } from '../shared/commands.js';
import { errorMessage } from '../shared/util.js';

/** Wire the external API into Pi: event publishing, lifecycle and `/octocode api`. Subagents never expose one. */
export function registerApi(pi: ExtensionAPI, commands: Subcommands, agents: AgentDirectory, version: () => string | undefined): { api: OctocodeApi; start: (ctx: Parameters<OctocodeApi['start']>[0]) => Promise<void>; stop: () => Promise<void> } {
  const api = new OctocodeApi(pi, agents, version);
  api.observe();

  commands.add('api', {
    description: 'api [on [http] | off] — external API status, start or stop',
    complete: (prefix) => wordCompletions(['on', 'on http', 'off'], prefix),
    handler: async (args, ctx) => {
      const [action, flag] = args.trim().split(/\s+/);
      try {
        if (action === 'on') {
          await api.start(ctx, { socket: process.platform !== 'win32', ...(flag === 'http' || process.platform === 'win32' ? { http: 0 } : {}) });
        } else if (action === 'off') {
          await api.stop();
          // stop() unbinds the session; bind again so a later `api on` works.
          await api.start(ctx, undefined);
        } else if (action) {
          ctx.ui.notify('Usage: /octocode api [on [http] | off]', 'warning');
          return;
        }
      } catch (error) {
        ctx.ui.notify(errorMessage(error), 'error');
        return;
      }
      const info = api.instance;
      ctx.ui.notify(
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
      try {
        await api.start(ctx, optionsFromEnv());
      } catch (error) {
        if (ctx.hasUI) ctx.ui.notify(errorMessage(error), 'warning');
      }
    },
    stop: () => api.stop(),
  };
}
