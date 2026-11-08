import { getOctocodeHome } from '@octocodeai/config';
import * as crypto from 'crypto';
import * as http from 'http';
import * as path from 'path';
import * as vscode from 'vscode';

import { errorMessage } from './errors';
import {
  HttpError,
  isJsonContentType,
  readBody,
  tokensEqual,
} from './ideBridgeHttp';
import { LockFile, pruneStaleLocks } from './ideBridgeLock';
import {
  IDE_BRIDGE_METHODS,
  isOpenable,
  methodSpec,
  parseInput,
  requireInWorkspace,
  requireString,
  type LspInput,
  type MethodSpec,
} from './ideBridgeMethods';
import type { Family, Json, Obj } from './ideBridgeTypes';
import { isObject } from './lspJson';

/**
 * Loopback bridge that lets the native `lspSearch` ask this editor's own
 * language providers (the built-in `vscode.execute*` commands) instead of
 * starting a language server. Protocol: RFC "Rung 2: IDE bridge".
 */

export const IDE_BRIDGE_PROTOCOL_VERSION = 1;
/**
 * Hierarchy items handed out by prepare*, incoming/outgoing and super/subtype
 * calls are kept so the caller can pass one back as `item`. The map holds at
 * most this many; the oldest is evicted first, and its handle then answers 404
 * ("unknown or expired") until the caller prepares again.
 */
export const MAX_ITEM_HANDLES = 1024;

export type IdeBridgeOptions = {
  output: (line: string) => void;
  /** Octocode home; default `getOctocodeHome()`. */
  home?: string;
};

type ProviderInput = Omit<LspInput, 'item'> & {
  uri?: vscode.Uri;
  item?: unknown;
};

function workspaceFolderPaths(): string[] {
  return (vscode.workspace.workspaceFolders ?? [])
    .filter(folder => folder.uri.scheme === 'file')
    .map(folder => folder.uri.fsPath);
}

function listen(server: http.Server): Promise<void> {
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      server.off('error', reject);
      resolve();
    });
  });
}

export class IdeBridge implements vscode.Disposable {
  private server: http.Server | undefined;
  private token: string | undefined;
  private port = 0;
  private lock: LockFile | undefined;
  private foldersListener: vscode.Disposable | undefined;
  private readonly handles = new Map<
    string,
    { family: Family; item: unknown }
  >();

  constructor(private readonly options: IdeBridgeOptions) {}

  get running(): boolean {
    return this.server !== undefined;
  }

  async start(): Promise<void> {
    if (this.server) return;
    const server = http.createServer((req, res) => void this.handle(req, res));
    server.on('error', error =>
      this.options.output(`IDE bridge server error: ${errorMessage(error)}`)
    );
    await listen(server);
    this.server = server;
    this.port = (server.address() as { port: number }).port;
    this.token = crypto.randomBytes(32).toString('hex');
    const directory = path.join(this.options.home ?? getOctocodeHome(), 'ide');
    this.lock = new LockFile(path.join(directory, `${process.pid}.json`));
    try {
      await pruneStaleLocks(directory);
      await this.writeLock();
    } catch (error) {
      await this.stop();
      throw error;
    }
    this.foldersListener = vscode.workspace.onDidChangeWorkspaceFolders(() => {
      this.writeLock().catch(error =>
        this.options.output(
          `IDE bridge lock file update failed: ${errorMessage(error)}`
        )
      );
    });
    this.options.output(`IDE bridge listening on 127.0.0.1:${this.port}`);
  }

  /** Stops the server and removes the lock file; safe to call repeatedly. */
  async stop(): Promise<void> {
    const { server, lock } = this;
    this.server = undefined;
    this.token = undefined;
    this.lock = undefined;
    this.handles.clear();
    this.foldersListener?.dispose();
    this.foldersListener = undefined;
    const failure = lock?.remove();
    if (lock && failure) {
      this.options.output(
        `IDE bridge could not remove ${lock.file}: ${failure}`
      );
    }
    if (server) {
      await new Promise<void>(resolve => {
        server.close(() => resolve());
        server.closeAllConnections();
      });
    }
    await lock?.settled();
  }

  dispose(): void {
    void this.stop();
  }

  private writeLock(): Promise<void> {
    return this.lock!.write({
      version: IDE_BRIDGE_PROTOCOL_VERSION,
      pid: process.pid,
      ide: vscode.env.appName,
      port: this.port,
      // The token lives only in the owner-only lock file and this process.
      token: this.token,
      workspaceFolders: workspaceFolderPaths(),
    });
  }

  private async handle(
    req: http.IncomingMessage,
    res: http.ServerResponse
  ): Promise<void> {
    let status = 200;
    let headers: http.OutgoingHttpHeaders = {};
    let body: Json;
    try {
      body = await this.route(req);
    } catch (error) {
      if (error instanceof HttpError) {
        status = error.status;
        headers = error.headers;
        body = {
          error: error.message,
          ...(error.reason ? { reason: error.reason } : {}),
        };
      } else {
        status = 500;
        body = { error: 'bridge failure', reason: errorMessage(error) };
        this.options.output(
          `IDE bridge request failed: ${errorMessage(error)}`
        );
      }
    }
    const payload = JSON.stringify(body);
    res.writeHead(status, {
      'Content-Type': 'application/json; charset=utf-8',
      'Content-Length': Buffer.byteLength(payload),
      'Cache-Control': 'no-store',
      ...headers,
    });
    res.end(payload);
  }

  /** Host, then auth, then route: nothing is read or run before both pass. */
  private async route(req: http.IncomingMessage): Promise<Json> {
    if (req.headers.host !== `127.0.0.1:${this.port}`) {
      throw new HttpError(400, 'invalid Host header');
    }
    const authorization = req.headers.authorization ?? '';
    const supplied = authorization.startsWith('Bearer ')
      ? authorization.slice('Bearer '.length)
      : '';
    if (!this.token || !tokensEqual(this.token, supplied)) {
      throw new HttpError(401, 'missing or invalid bearer token', undefined, {
        'WWW-Authenticate': 'Bearer',
      });
    }
    const pathname = new URL(req.url ?? '/', 'http://127.0.0.1').pathname;
    if (pathname === '/health') {
      this.requireMethod(req, 'GET');
      return {
        version: IDE_BRIDGE_PROTOCOL_VERSION,
        ide: vscode.env.appName,
        workspaceFolders: workspaceFolderPaths(),
        methods: IDE_BRIDGE_METHODS,
      };
    }
    if (pathname === '/lsp') {
      this.requireMethod(req, 'POST');
      if (!isJsonContentType(req.headers['content-type'])) {
        throw new HttpError(400, 'Content-Type must be application/json');
      }
      return this.lsp(await this.parseBody(req));
    }
    throw new HttpError(404, 'unknown route', `${pathname} is not served`);
  }

  private requireMethod(req: http.IncomingMessage, allowed: string): void {
    if (req.method !== allowed) {
      throw new HttpError(405, 'method not allowed', `use ${allowed}`, {
        Allow: allowed,
      });
    }
  }

  private async parseBody(req: http.IncomingMessage): Promise<Obj> {
    const text = await readBody(req);
    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch {
      throw new HttpError(400, 'request body is not valid JSON');
    }
    if (!isObject(parsed)) {
      throw new HttpError(400, 'request body must be a JSON object');
    }
    return parsed;
  }

  private async lsp(body: Obj): Promise<Json> {
    const method = requireString(body, 'method');
    const spec = methodSpec(method);
    if (!spec) {
      throw new HttpError(
        404,
        'unknown method',
        `"${method}" is not one of: ${IDE_BRIDGE_METHODS.join(', ')}`
      );
    }
    const input = parseInput(spec, body);
    if (input.item) {
      const item = this.lookup(method, spec, input.item.data);
      return { result: await this.answer(method, spec, { item }) };
    }
    const inside = input.path
      ? await requireInWorkspace(input.path)
      : undefined;
    const document =
      input.path && inside && (await isOpenable(spec, input.path))
        ? await this.open(inside)
        : undefined;
    const result = await this.answer(method, spec, {
      ...input,
      uri: document?.uri,
    });
    const empty = result === null || (Array.isArray(result) && !result.length);
    if (
      empty &&
      document &&
      !(await this.hasProvider(document, method === 'documentSymbols'))
    ) {
      return { unsupported: true };
    }
    return { result };
  }

  private async open(uri: vscode.Uri): Promise<vscode.TextDocument> {
    try {
      return await vscode.workspace.openTextDocument(uri);
    } catch (error) {
      throw new HttpError(422, 'cannot open document', errorMessage(error));
    }
  }

  /** Runs the provider command and converts its answer to LSP JSON. */
  private async answer(
    method: string,
    spec: MethodSpec,
    input: ProviderInput
  ): Promise<Json> {
    let raw: unknown;
    try {
      raw = await this.execute(method, spec, input);
    } catch (error) {
      throw new HttpError(500, 'provider failed', errorMessage(error));
    }
    return spec.convert(raw, (family, item) => this.remember(family, item));
  }

  private execute(
    method: string,
    spec: MethodSpec,
    input: ProviderInput
  ): unknown {
    if (method === 'diagnostics') {
      return vscode.languages.getDiagnostics(input.uri!);
    }
    const args = {
      item: [input.item],
      workspace: [input.query],
      document: [input.uri],
      position: [input.uri, input.position],
    }[spec.shape];
    return vscode.commands.executeCommand(spec.command!, ...args);
  }

  private lookup(method: string, spec: MethodSpec, handle: string): unknown {
    const entry = this.handles.get(handle);
    if (!entry) {
      throw new HttpError(
        404,
        'unknown item handle',
        `the handle is unknown or was evicted (the bridge keeps the latest ${MAX_ITEM_HANDLES} items); call the prepare method again`
      );
    }
    if (entry.family !== spec.family) {
      throw new HttpError(
        400,
        'item handle belongs to another hierarchy',
        `"${method}" needs a ${spec.family} hierarchy item`
      );
    }
    return entry.item;
  }

  private remember(family: Family, item: unknown): string {
    const handle = crypto.randomUUID();
    this.handles.set(handle, { family, item });
    if (this.handles.size > MAX_ITEM_HANDLES) {
      this.handles.delete(this.handles.keys().next().value!);
    }
    return handle;
  }

  /**
   * An empty answer is only trusted when the editor shows a language service
   * for the document: its language is registered and its document-symbol
   * provider answers. Otherwise the caller gets `unsupported` and falls to a
   * server instead of reporting absence.
   */
  private async hasProvider(
    document: vscode.TextDocument,
    answeredWithSymbols: boolean
  ): Promise<boolean> {
    if (answeredWithSymbols || document.languageId === 'plaintext') {
      return false;
    }
    if (
      !(await vscode.languages.getLanguages()).includes(document.languageId)
    ) {
      return false;
    }
    try {
      const symbols = await vscode.commands.executeCommand(
        'vscode.executeDocumentSymbolProvider',
        document.uri
      );
      return Array.isArray(symbols) && symbols.length > 0;
    } catch {
      return false;
    }
  }
}
