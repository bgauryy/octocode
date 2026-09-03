import http from 'node:http';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import type {
  OAuthClientInformation,
  OAuthClientMetadata,
  OAuthClientProvider,
  OAuthTokens,
  StreamableHTTPClientTransport,
} from '@modelcontextprotocol/client';
import type { PiContext } from '../types.js';
import { renderOctocodePage } from '../tui/html-page.js';
import { openApprovedExternalUrl } from './local-url-opener.js';

export interface StoredOAuthCredential {
  clientInformation?: OAuthClientInformation;
  tokens?: OAuthTokens;
  codeVerifier?: string;
}

export interface McpOAuthDependencies {
  readCredential(account: string): Promise<StoredOAuthCredential>;
  writeCredential(account: string, value: StoredOAuthCredential): Promise<void>;
  deleteCredential(account: string): Promise<void>;
  openApprovedUrl(target: string): Promise<{ ok: boolean; message?: string }>;
}

const SERVICE = 'octocode-mcp-oauth';

function credentialAccount(serverName: string, serverUrl: string): string {
  return `${serverName}:${createHash('sha256').update(serverUrl).digest('hex').slice(0, 24)}`;
}

function runSecretCommand(command: string, args: string[], input?: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: ['pipe', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk: string) => { stdout += chunk; });
    child.stderr.on('data', (chunk: string) => { stderr += chunk; });
    child.once('error', reject);
    child.once('close', (code) => code === 0 ? resolve(stdout.trim()) : reject(new Error(stderr.trim() || `${command} exited ${code}`)));
    if (input !== undefined) child.stdin.end(input);
    else child.stdin.end();
  });
}

async function readCredential(account: string): Promise<StoredOAuthCredential> {
  try {
    const text = process.platform === 'darwin'
      ? await runSecretCommand('security', ['find-generic-password', '-s', SERVICE, '-a', account, '-w'])
      : process.platform === 'linux'
        ? await runSecretCommand('secret-tool', ['lookup', 'service', SERVICE, 'account', account])
        : '';
    if (!text) return {};
    const parsed = JSON.parse(text) as unknown;
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed as StoredOAuthCredential : {};
  } catch {
    return {};
  }
}

async function writeCredential(account: string, value: StoredOAuthCredential): Promise<void> {
  const text = JSON.stringify(value);
  if (process.platform === 'darwin') {
    await runSecretCommand('security', ['add-generic-password', '-U', '-s', SERVICE, '-a', account, '-w', text]);
    return;
  }
  if (process.platform === 'linux') {
    await runSecretCommand('secret-tool', ['store', '--label', `Octocode MCP OAuth ${account}`, 'service', SERVICE, 'account', account], text);
    return;
  }
  throw new Error('MCP OAuth credential storage requires macOS Keychain or Linux Secret Service on this platform');
}

async function deleteCredential(account: string): Promise<void> {
  try {
    if (process.platform === 'darwin') await runSecretCommand('security', ['delete-generic-password', '-s', SERVICE, '-a', account]);
    else if (process.platform === 'linux') await runSecretCommand('secret-tool', ['clear', 'service', SERVICE, 'account', account]);
  } catch { /* already absent */ }
}

export interface McpOAuthFlow {
  provider: OAuthClientProvider;
  attachTransport(transport: StreamableHTTPClientTransport): void;
  hasTokens(): Promise<boolean>;
  close(): void;
}

/** Credential-health probe for UI/status surfaces. Never returns token material. */
export async function hasStoredMcpOAuthTokens(serverName: string, serverUrl: string): Promise<boolean> {
  return Boolean((await readCredential(credentialAccount(serverName, serverUrl))).tokens);
}

/** Remove all locally persisted OAuth state for a server identity. */
export async function revokeStoredMcpOAuthCredentials(serverName: string, serverUrl: string): Promise<void> {
  await deleteCredential(credentialAccount(serverName, serverUrl));
}

export async function createMcpOAuthFlow(serverName: string, serverUrl: string, ctx?: PiContext): Promise<McpOAuthFlow> {
  return createMcpOAuthFlowWithDependencies(serverName, serverUrl, ctx);
}

export async function createMcpOAuthFlowWithDependencies(
  serverName: string,
  serverUrl: string,
  ctx?: PiContext,
  dependencies: Partial<McpOAuthDependencies> = {},
): Promise<McpOAuthFlow> {
  const deps: McpOAuthDependencies = {
    readCredential,
    writeCredential,
    deleteCredential,
    openApprovedUrl: (target) => openApprovedExternalUrl(target, true),
    ...dependencies,
  };
  const account = credentialAccount(serverName, serverUrl);
  let stored = await deps.readCredential(account);
  let transport: StreamableHTTPClientTransport | undefined;
  let callbackResolve!: () => void;
  let callbackReject!: (error: Error) => void;
  let expectedState: string | undefined;
  const callbackComplete = new Promise<void>((resolve, reject) => {
    callbackResolve = resolve;
    callbackReject = reject;
  });
  const callbackServer = http.createServer((req, res) => {
    void (async () => {
      try {
        const url = new URL(req.url ?? '/', redirectUrl);
        if (url.pathname !== '/oauth/callback') throw new Error('Invalid OAuth callback path');
        if (expectedState && url.searchParams.get('state') !== expectedState) throw new Error('OAuth state mismatch');
        if (url.searchParams.get('error')) throw new Error(`OAuth authorization failed: ${url.searchParams.get('error')}`);
        if (!url.searchParams.get('code')) throw new Error('OAuth callback did not include a code');
        if (!transport) throw new Error('OAuth transport is unavailable');
        await transport.finishAuth(url.searchParams);
        res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
        res.end(renderOctocodePage({
          title: 'MCP authorized',
          bodyHtml: '<section><h2>Authorization complete</h2><p>You may close this tab and return to Octocode.</p></section>',
        }));
        callbackResolve();
      } catch (error) {
        res.writeHead(400, { 'content-type': 'text/plain; charset=utf-8' });
        res.end((error as Error).message);
        callbackReject(error as Error);
      } finally {
        callbackServer.close();
      }
    })();
  });
  await new Promise<void>((resolve, reject) => {
    callbackServer.once('error', reject);
    callbackServer.listen(0, '127.0.0.1', resolve);
  });
  callbackServer.unref();
  const address = callbackServer.address();
  if (!address || typeof address === 'string') throw new Error('Could not allocate MCP OAuth callback port');
  const redirectUrl = new URL(`http://127.0.0.1:${address.port}/oauth/callback`);
  const persist = async (): Promise<void> => deps.writeCredential(account, stored);
  const provider: OAuthClientProvider = {
    get redirectUrl() { return redirectUrl; },
    get clientMetadata(): OAuthClientMetadata {
      return {
        client_name: 'Octocode Agent',
        redirect_uris: [redirectUrl.href],
        grant_types: ['authorization_code', 'refresh_token'],
        response_types: ['code'],
        token_endpoint_auth_method: 'none',
      };
    },
    clientInformation: async () => stored.clientInformation,
    saveClientInformation: async (value) => { stored = { ...stored, clientInformation: value }; await persist(); },
    tokens: async () => stored.tokens,
    saveTokens: async (value) => { stored = { ...stored, tokens: value }; await persist(); },
    redirectToAuthorization: async (authorizationUrl) => {
      if (!ctx?.hasUI || !ctx.ui?.confirm) throw new Error('MCP OAuth requires an interactive session');
      expectedState = authorizationUrl.searchParams.get('state') ?? undefined;
      const approved = await ctx.ui.confirm(`Authorize MCP server ${serverName}?`, authorizationUrl.origin);
      if (!approved) throw new Error('MCP OAuth authorization denied by user');
      const opened = await deps.openApprovedUrl(authorizationUrl.href);
      if (!opened.ok) throw new Error(opened.message ?? 'Could not open MCP OAuth authorization URL');
      await callbackComplete;
    },
    saveCodeVerifier: async (value) => { stored = { ...stored, codeVerifier: value }; await persist(); },
    codeVerifier: async () => {
      if (!stored.codeVerifier) throw new Error('MCP OAuth code verifier is unavailable');
      return stored.codeVerifier;
    },
    invalidateCredentials: async (scope) => {
      if (scope === 'all') {
        stored = {};
        await deps.deleteCredential(account);
      } else {
        if (scope === 'client') delete stored.clientInformation;
        if (scope === 'tokens') delete stored.tokens;
        if (scope === 'verifier') delete stored.codeVerifier;
        await persist();
      }
    },
  };
  return {
    provider,
    attachTransport(value) { transport = value; },
    async hasTokens() { return Boolean((await deps.readCredential(account)).tokens); },
    close() { callbackServer.close(); },
  };
}
