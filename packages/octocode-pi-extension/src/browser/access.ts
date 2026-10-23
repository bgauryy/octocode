import fs from 'node:fs';
import path from 'node:path';
import type { ExtensionContext } from '@earendil-works/pi-coding-agent';
import { blockedHost, httpOnlyRefusal, keyOf, parseUrl, portOf } from './network.js';
import type { Permission } from '../permission/ask.js';
import { actionWhy, decide } from '../permission/mode.js';
import { isWithin, workspaceRoot } from '../shared/home.js';
import { classifyHost, isHttpUrl, type HostClass, type HostLookup } from '../web/guard.js';

/** Where a browser check asks for confirmation: the calling tool's context. */
export type Confirmer = Pick<ExtensionContext, 'hasUI' | 'ui'> & { signal?: AbortSignal | undefined };

/**
 * What `checkBrowserUrl` decided: the one classification of the host it made, and, for a non-public host (the only one
 * that asks `net.private`), how long the permission lasts: `call` (Allow once, or a mode that allows it) or `session`
 * (a session grant or kind). Absent for a public host.
 */
interface CheckedUrl {
  host: HostClass;
  privateGrant?: 'call' | 'session';
}

/**
 * The session grant for opening `url`: keyed by host and effective port (80 and 443 when the URL omits it), so a grant
 * given for `http://localhost` does not cover `https://localhost`. The label names the same host and port.
 */
function grantFor(url: URL, prefix = ''): { key: string; label: string } {
  return { key: `${prefix}${keyOf(url)}`, label: `${url.hostname}:${portOf(url)}` };
}

/**
 * The check on a page the *agent* opens (navigate, a new tab, switching to a tab); what the page itself requests is
 * refused by `PagePolicy` without asking. Refuses non-http(s) URLs. A private or local host (loopback, LAN, cloud
 * metadata) is a `net.private` permission ask (allowed per host for the session): page content steers the agent, so it
 * must not reach internal services the `web` tool keeps out. A public page is the `browser` ask, and a private one is
 * one prompt carrying both reasons. `options`: the asking tool call's id, and the resolver to classify the host with.
 */
export async function checkBrowserUrl(raw: string, ctx: Confirmer, permission: Permission, options: { toolCallId: string; lookup?: HostLookup | undefined }): Promise<CheckedUrl> {
  const { toolCallId, lookup } = options;
  const url = parseUrl(raw);
  if (!isHttpUrl(url)) throw new Error(httpOnlyRefusal(url));
  const host = await classifyHost(url, lookup);
  const call = { toolCallId, ...(ctx.signal ? { signal: ctx.signal } : {}) };
  if (host.kind === 'public') {
    const verdict = await permission.check(ctx, { action: 'browser', title: 'Open this page?', what: [url.href], grant: grantFor(url, 'host '), ...call });
    if (!verdict.allowed) throw new Error(verdict.message);
    return { host };
  }
  // A name that does not resolve here may still resolve in Chrome (its own DNS, hosts file, proxy) to a private address.
  const what = host.kind === 'private' ? 'a local or private network address' : 'a name that does not resolve here and may reach a local or private address';
  const alsoAsks = decide(permission.mode, 'browser') === 'ask';
  const verdict = await permission.check(ctx, {
    action: 'net.private',
    title: 'Open a private address?',
    what: [`The browser wants to open ${url.host}, ${what}.`],
    ...(alsoAsks ? { reason: actionWhy('browser') } : {}),
    grant: grantFor(url),
    ...call,
  });
  if (verdict.allowed) return { host, privateGrant: verdict.lasts };
  throw new Error(`${blockedHost(host)}: ${url.host}. ${verdict.message}`);
}

/**
 * Upload paths resolved against `cwd` and through symlinks: the files Chrome will read, returned for the upload. An
 * upload is browser control and asks every call; a path outside the workspace (a symlink inside it pointing out
 * included) makes it an outside upload too, and one prompt names both reasons.
 */
export async function checkUploadPaths(files: readonly string[], ctx: Pick<ExtensionContext, 'cwd'> & Confirmer, permission: Permission, toolCallId?: string): Promise<string[]> {
  // The workspace is the repository root, and none at all when that is the home folder or above it: then every path is outside.
  const workspace = workspaceRoot(ctx.cwd);
  const root = workspace === undefined ? undefined : resolvedTarget(workspace);
  const paths = files.map((file) => resolvedTarget(path.resolve(ctx.cwd, file)));
  const outside = paths.filter((file) => root === undefined || !isWithin(file, root));
  const verdict = await permission.check(ctx, {
    action: 'browser.control',
    ...(outside.length > 0 ? { also: ['upload.outside' as const] } : {}),
    title: outside.length > 0 ? 'Upload files from outside the workspace?' : 'Upload files to the page?',
    what: outside.length > 0 ? outside : paths,
    ...(toolCallId ? { toolCallId } : {}),
    cwd: ctx.cwd,
    ...(ctx.signal ? { signal: ctx.signal } : {}),
  });
  if (verdict.allowed) return paths;
  throw new Error(outside.length > 0 ? `Refused to upload files outside the workspace: ${outside.join(', ')}. ${verdict.message}` : `Refused to upload files: ${verdict.message}`);
}

/** `file` with symlinks resolved; a missing tail (a file that does not exist yet) is kept under its real parent. */
function resolvedTarget(file: string): string {
  try {
    return fs.realpathSync(file);
  } catch {
    const parent = path.dirname(file);
    return parent === file ? file : path.join(resolvedTarget(parent), path.basename(file));
  }
}
