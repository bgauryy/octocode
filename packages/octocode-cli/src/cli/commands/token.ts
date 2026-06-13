import type { CLICommand, ParsedArgs } from '../types.js';
import { c, dim } from '../../utils/colors.js';
import { getToken, getTokenType } from '../../features/github-oauth.js';
import {
  type GetTokenSource,
  maskToken,
  safeTokenOutput,
  formatTokenSource,
  printLoginHint,
} from './shared.js';
import https from 'node:https';

function pingGitHubApi(
  token: string,
  hostname: string
): Promise<{
  valid: boolean;
  login?: string;
  rateLimit?: { remaining: number; limit: number; reset: number };
  error?: string;
}> {
  return new Promise(resolve => {
    const apiHost =
      hostname === 'github.com' ? 'api.github.com' : `${hostname}/api/v3`;
    const req = https.request(
      {
        method: 'GET',
        hostname: apiHost.replace(/\/.*/, ''),
        path: apiHost.includes('/')
          ? `/${apiHost.split('/').slice(1).join('/')}/user`
          : '/user',
        headers: {
          Authorization: `Bearer ${token}`,
          'User-Agent': 'octocode-cli/1.0',
          Accept: 'application/vnd.github+json',
        },
        timeout: 8000,
      },
      res => {
        let body = '';
        res.on('data', (chunk: Buffer) => (body += chunk.toString()));
        res.on('end', () => {
          if (res.statusCode === 401 || res.statusCode === 403) {
            resolve({ valid: false, error: `HTTP ${res.statusCode}` });
            return;
          }
          try {
            const json = JSON.parse(body) as {
              login?: string;
              message?: string;
            };
            const rateRemaining = Number(
              res.headers['x-ratelimit-remaining'] ?? 0
            );
            const rateLimit = Number(res.headers['x-ratelimit-limit'] ?? 0);
            const rateReset = Number(res.headers['x-ratelimit-reset'] ?? 0);
            resolve({
              valid: Boolean(json.login),
              login: json.login,
              rateLimit: {
                remaining: rateRemaining,
                limit: rateLimit,
                reset: rateReset,
              },
            });
          } catch {
            resolve({ valid: false, error: 'Invalid API response' });
          }
        });
      }
    );
    req.on('error', err => resolve({ valid: false, error: err.message }));
    req.on('timeout', () => {
      req.destroy();
      resolve({ valid: false, error: 'Request timed out' });
    });
    req.end();
  });
}

export const tokenCommand: CLICommand = {
  name: 'token',
  description: 'Print the GitHub token (matches octocode-mcp priority)',
  usage:
    'octocode token [--type <auto|octocode|gh>] [--hostname <host>] [--source] [--validate] [--reveal] [--json]',
  options: [
    {
      name: 'type',
      description:
        'Token source: auto (default: env→octocode→gh), octocode, gh',
      hasValue: true,
      default: 'auto',
    },
    {
      name: 'hostname',
      description: 'GitHub Enterprise hostname (default: github.com)',
      hasValue: true,
    },
    {
      name: 'source',
      description: 'Show token source and user info',
    },
    {
      name: 'validate',
      description:
        'Ping the GitHub API to verify the token is valid and show rate-limit info',
    },
    {
      name: 'reveal',
      description:
        'Print the full token on screen (default: masked on a terminal; raw when piped, e.g. $(octocode token))',
    },
    {
      name: 'json',
      description:
        'Output as JSON: { token, type, valid?, login?, rateLimit? }',
    },
  ],
  handler: async (args: ParsedArgs) => {
    const hostnameOpt = args.options['hostname'];
    const hostname =
      (typeof hostnameOpt === 'string' ? hostnameOpt : undefined) ||
      'github.com';
    const showSource = Boolean(args.options['source']);
    const validateToken = Boolean(args.options['validate']);
    const reveal = Boolean(args.options['reveal']);
    const jsonOutput = Boolean(args.options['json']);
    const typeOpt = args.options['type'];
    const typeArg =
      (typeof typeOpt === 'string' ? typeOpt : undefined) || 'auto';

    let tokenSource: GetTokenSource;
    switch (typeArg.toLowerCase()) {
      case 'octocode':
        tokenSource = 'octocode';
        break;
      case 'gh':
        tokenSource = 'gh';
        break;
      case 'auto':
        tokenSource = 'auto';
        break;
      default:
        if (jsonOutput) {
          console.log(JSON.stringify({ token: null, type: 'none' }));
          process.exitCode = 1;
          return;
        }
        console.log();
        console.log(`  ${c('red', '✗')} Invalid token type: ${typeArg}`);
        console.log(`  ${dim('Valid options:')} octocode, gh, auto`);
        console.log();
        process.exitCode = 1;
        return;
    }

    const result = await getToken(hostname, tokenSource);

    if (jsonOutput) {
      if (!result.token) {
        console.log(
          JSON.stringify({ token: null, type: 'none', valid: false })
        );
        process.exitCode = 1;
        return;
      }
      if (validateToken) {
        const ping = await pingGitHubApi(result.token, hostname);
        console.log(
          JSON.stringify({
            token: result.token,
            type: getTokenType(result.source, result.envSource),
            valid: ping.valid,
            login: ping.login ?? null,
            rateLimit: ping.rateLimit ?? null,
            error: ping.error ?? null,
          })
        );
        if (!ping.valid) process.exitCode = 1;
        return;
      }
      console.log(
        JSON.stringify({
          token: result.token,
          type: getTokenType(result.source, result.envSource),
        })
      );
      return;
    }

    if (!result.token) {
      console.log();
      if (tokenSource === 'octocode') {
        console.log(
          `  ${c('yellow', '⚠')} No Octocode token found for ${hostname}`
        );
        console.log();
        console.log(`  ${dim('To login with Octocode:')}`);
        console.log(`    ${c('cyan', '→')} ${c('yellow', 'octocode login')}`);
        console.log();
        console.log(`  ${dim('Or use gh CLI token:')}`);
        console.log(
          `    ${c('cyan', '→')} ${c('yellow', 'octocode token --type=gh')}`
        );
      } else if (tokenSource === 'gh') {
        console.log(
          `  ${c('yellow', '⚠')} No gh CLI token found for ${hostname}`
        );
        console.log();
        console.log(`  ${dim('To login with gh CLI:')}`);
        console.log(`    ${c('cyan', '→')} ${c('yellow', 'gh auth login')}`);
        console.log();
        console.log(`  ${dim('Or use Octocode token:')}`);
        console.log(
          `    ${c('cyan', '→')} ${c('yellow', 'octocode token --type=octocode')}`
        );
      } else {
        console.log(`  ${c('yellow', '⚠')} Not authenticated to ${hostname}`);
        console.log();
        printLoginHint();
      }
      console.log();
      process.exitCode = 1;
      return;
    }

    if (validateToken) {
      const { Spinner } = await import('../../utils/spinner.js');
      const spinner = new Spinner(
        'Validating token against GitHub API...'
      ).start();
      const ping = await pingGitHubApi(result.token!, hostname);
      spinner.stop();
      console.log();
      if (ping.valid) {
        console.log(
          `  ${c('green', '✓')} Token is valid — authenticated as ${c('cyan', '@' + (ping.login ?? 'unknown'))}`
        );
        if (ping.rateLimit) {
          const resetDate = new Date(
            ping.rateLimit.reset * 1000
          ).toLocaleTimeString();
          console.log(
            `  ${dim('Rate limit:')} ${ping.rateLimit.remaining}/${ping.rateLimit.limit} remaining, resets at ${resetDate}`
          );
        }
      } else {
        console.log(
          `  ${c('red', '✗')} Token validation failed: ${ping.error ?? 'unknown error'}`
        );
        process.exitCode = 1;
      }
      console.log();
      return;
    }

    if (showSource) {
      console.log();
      console.log(`  ${c('green', '✓')} Token found`);
      console.log(
        `  ${dim('Source:')} ${formatTokenSource(result.source, result.envSource)}`
      );
      if (result.username) {
        console.log(`  ${dim('User:')} ${c('cyan', '@' + result.username)}`);
      }
      console.log();
      console.log(
        `  ${dim('Token:')} ${reveal ? result.token! : maskToken(result.token!)}`
      );
      console.log();
    } else {
      console.log(reveal ? result.token! : safeTokenOutput(result.token!));
    }
  },
};
