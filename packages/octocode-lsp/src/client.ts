import { spawn, ChildProcess } from 'child_process';
import {
  createMessageConnection,
  MessageConnection,
  StreamMessageReader,
  StreamMessageWriter,
} from 'vscode-jsonrpc/node.js';
import {
  InitializeResult,
  InitializedParams,
} from 'vscode-languageserver-protocol';
import type {
  ExactPosition,
  CodeSnippet,
  CallHierarchyItem,
  IncomingCall,
  OutgoingCall,
  LanguageServerConfig,
} from './types.js';
import { LSPDocumentManager } from './lspDocumentManager.js';
import { LSPOperations } from './lspOperations.js';
import {
  buildChildProcessEnv,
  TOOLING_ALLOWED_ENV_VARS,
} from './processEnv.js';
import { buildInitializeParams } from './initParams.js';
import { toUri } from './uri.js';

const STDERR_RETENTION_LINES = 200;

async function raceWithTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number,
  timeoutMessage: string
): Promise<T> {
  let timeoutId: ReturnType<typeof setTimeout> | undefined;
  const timeoutPromise = new Promise<never>((_, reject) => {
    timeoutId = setTimeout(() => reject(new Error(timeoutMessage)), timeoutMs);
  });
  try {
    return await Promise.race([promise, timeoutPromise]);
  } finally {
    if (timeoutId !== undefined) {
      clearTimeout(timeoutId);
    }
  }
}

type WorkspaceConfigurationRequest = {
  items?: Array<{ section?: string; scopeUri?: string }>;
};

export class LSPClient {
  private process: ChildProcess | null = null;
  private connection: MessageConnection | null = null;
  private initialized = false;
  private config: LanguageServerConfig;
  private initializeResult: InitializeResult | null = null;
  private documentManager: LSPDocumentManager;
  private operations: LSPOperations;
  private stderrBuffer: string[] = [];

  // Indexing-wait: resolved once all $/progress tokens have ended (or fallback fires).
  // tsserver only starts loading the project on the first textDocument/didOpen,
  // so the fallback is armed there — not at initialize. A request sent before
  // project load finishes gets file-scoped (partial) results, e.g. references
  // confined to the opened file.
  private static readonly READY_SETTLE_MS = 2_500;
  private static readonly READY_MAX_WAIT_MS = 15_000;
  private readyPromise: Promise<void>;
  private readyResolve!: () => void;
  private readyResolved = false;
  private activeProgressTokens = new Set<string | number>();
  private readyFallbackTimer: ReturnType<typeof setTimeout> | null = null;
  private hasOpenedDocument = false;

  constructor(config: LanguageServerConfig) {
    this.config = config;
    this.readyPromise = new Promise<void>(resolve => {
      this.readyResolve = resolve;
    });
    this.documentManager = new LSPDocumentManager(config);
    this.documentManager.setOnDidOpen(() => this.handleDocumentOpened());
    this.operations = new LSPOperations(
      this.documentManager,
      config.workspaceRoot
    );
    this.operations.setProjectReadyWaiter(() => this.waitForReady());
  }

  private handleDocumentOpened(): void {
    if (this.hasOpenedDocument) return;
    this.hasOpenedDocument = true;
    if (this.readyResolved) return;
    const armedAt = Date.now();
    const settle = (): void => {
      this.readyFallbackTimer = null;
      if (this.readyResolved) return;
      if (
        this.activeProgressTokens.size === 0 ||
        Date.now() - armedAt >= LSPClient.READY_MAX_WAIT_MS
      ) {
        this.resolveReady();
        return;
      }
      // Indexing still in flight — the $/progress end handler resolves
      // readiness; re-arm so a token that never ends cannot block forever.
      this.readyFallbackTimer = setTimeout(settle, LSPClient.READY_SETTLE_MS);
    };
    this.readyFallbackTimer = setTimeout(settle, LSPClient.READY_SETTLE_MS);
  }

  private resolveReady(): void {
    if (this.readyResolved) return;
    this.readyResolved = true;
    // clearTimeout is a no-op for null/undefined — safe without an explicit null check.
    clearTimeout(this.readyFallbackTimer ?? undefined);
    this.readyFallbackTimer = null;
    this.readyResolve();
  }

  /**
   * Waits until the language server has finished its initial project indexing,
   * or until timeoutMs elapses (whichever comes first). Never throws — callers
   * proceed even on timeout so a slow server does not block the tool forever.
   */
  async waitForReady(timeoutMs = 45_000): Promise<void> {
    if (this.readyResolved) return;
    // If the client was never started, skip the wait and let requireConnection()
    // throw "LSP client not initialized" immediately.
    if (!this.initialized) return;
    // Project load only starts on the first didOpen — before that there is
    // nothing to wait for (and no fallback timer armed yet).
    if (!this.hasOpenedDocument) return;
    await Promise.race([
      this.readyPromise,
      new Promise<void>(resolve => setTimeout(resolve, timeoutMs)),
    ]);
  }

  async start(): Promise<void> {
    if (this.process) {
      throw new Error('LSP client already started');
    }

    this.process = spawn(this.config.command, this.config.args ?? [], {
      cwd: this.config.workspaceRoot,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: buildChildProcessEnv({}, TOOLING_ALLOWED_ENV_VARS),
    });

    if (!this.process.stdin || !this.process.stdout) {
      try {
        this.process.kill();
      } catch {
        void 0;
      }
      this.process = null;
      throw new Error('Failed to create language server process pipes');
    }

    if (typeof this.process.stderr?.setEncoding === 'function') {
      this.process.stderr.setEncoding('utf8');
    }
    this.process.stderr?.on('data', (chunk: string | Buffer) => {
      const text = typeof chunk === 'string' ? chunk : chunk.toString('utf8');
      const lines = text.split(/\r?\n/);
      for (const line of lines) {
        if (!line) continue;
        this.stderrBuffer.push(line);
        if (this.stderrBuffer.length > STDERR_RETENTION_LINES) {
          this.stderrBuffer.shift();
        }
      }
    });

    let onEarlyError: ((error: Error) => void) | undefined;
    let onEarlyClose:
      | ((code: number | null, signal: NodeJS.Signals | null) => void)
      | undefined;
    const earlyExitPromise = new Promise<never>((_, reject) => {
      onEarlyError = (error: Error) => {
        if (!this.initialized) {
          reject(
            new Error(
              this.formatServerStartupFailure(`process error: ${error.message}`)
            )
          );
        }
      };
      onEarlyClose = (code, signal) => {
        if (!this.initialized) {
          reject(
            new Error(
              this.formatServerStartupFailure(
                `process exited before initialize (code=${code ?? 'null'}, signal=${signal ?? 'null'})`
              )
            )
          );
        }
      };
      this.process!.once('error', onEarlyError);
      this.process!.once('close', onEarlyClose);
    });

    try {
      this.connection = createMessageConnection(
        new StreamMessageReader(this.process.stdout),
        new StreamMessageWriter(this.process.stdin)
      );

      this.registerServerInitiatedHandlers(this.connection);
      this.connection.listen();

      await Promise.race([this.initialize(), earlyExitPromise]);
    } catch (error) {
      await this.stop();
      throw error;
    } finally {
      if (onEarlyError) this.process?.off('error', onEarlyError);
      if (onEarlyClose) this.process?.off('close', onEarlyClose);
    }
  }

  private formatServerStartupFailure(reason: string): string {
    const stderr = this.getRecentStderr();
    const stderrSuffix =
      stderr.length > 0 ? `; stderr: ${stderr.slice(-8).join('\n')}` : '';
    return `Language server startup failed: ${reason}${stderrSuffix}`;
  }

  private registerServerInitiatedHandlers(connection: MessageConnection): void {
    connection.onRequest('workspace/configuration', params => {
      const request = params as WorkspaceConfigurationRequest;
      return (request.items ?? []).map(() => ({}));
    });

    connection.onRequest('workspace/workspaceFolders', () => [
      {
        uri: toUri(this.config.workspaceRoot),
        name:
          this.config.workspaceRoot.split(/\//).filter(Boolean).pop() ??
          this.config.workspaceRoot,
      },
    ]);

    connection.onRequest('client/registerCapability', () => null);
    connection.onRequest('client/unregisterCapability', () => null);
    connection.onRequest('window/workDoneProgress/create', () => null);

    connection.onNotification('window/logMessage', () => undefined);
    connection.onNotification('window/showMessage', () => undefined);

    connection.onNotification(
      '$/progress',
      (params: { token: string | number; value: { kind: string } }) => {
        if (params.value.kind === 'begin') {
          this.activeProgressTokens.add(params.token);
        } else if (params.value.kind === 'end') {
          this.activeProgressTokens.delete(params.token);
          if (this.activeProgressTokens.size === 0) {
            this.resolveReady();
          }
        }
      }
    );
  }

  private async initialize(): Promise<void> {
    if (!this.connection) {
      throw new Error('Connection not established');
    }

    const initParams = buildInitializeParams(this.config);

    this.initializeResult = (await raceWithTimeout(
      this.connection.sendRequest('initialize', initParams),
      30_000,
      'LSP initialize timed out after 30s'
    )) as InitializeResult;

    const initializedParams: InitializedParams = {};
    await this.connection.sendNotification('initialized', initializedParams);

    this.initialized = true;

    this.documentManager.setTextDocumentSync(
      this.initializeResult?.capabilities?.textDocumentSync
    );
    this.documentManager.setConnection(this.connection, this.initialized);
    this.operations.setConnection(this.connection, this.initialized);
    // Readiness fallback is armed on the first didOpen (handleDocumentOpened) —
    // arming it here would let requests race tsserver's project load.
  }

  async openDocument(filePath: string, content?: string): Promise<void> {
    return this.documentManager.openDocument(filePath, content);
  }

  async ensureDocumentSynced(
    filePath: string,
    content?: string
  ): Promise<void> {
    await this.waitForReady();
    return this.documentManager.ensureDocumentSynced(filePath, { content });
  }

  async closeDocument(filePath: string): Promise<void> {
    return this.documentManager.closeDocument(filePath);
  }

  async gotoDefinition(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CodeSnippet[]> {
    await this.waitForReady();
    return this.operations.gotoDefinition(filePath, position, content);
  }

  async findReferences(
    filePath: string,
    position: ExactPosition,
    includeDeclaration = true,
    content?: string
  ): Promise<CodeSnippet[]> {
    await this.waitForReady();
    return this.operations.findReferences(
      filePath,
      position,
      includeDeclaration,
      content
    );
  }

  async prepareCallHierarchy(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CallHierarchyItem[]> {
    await this.waitForReady();
    return this.operations.prepareCallHierarchy(filePath, position, content);
  }

  async getIncomingCalls(item: CallHierarchyItem): Promise<IncomingCall[]> {
    await this.waitForReady();
    return this.operations.getIncomingCalls(item);
  }

  async getOutgoingCalls(item: CallHierarchyItem): Promise<OutgoingCall[]> {
    await this.waitForReady();
    return this.operations.getOutgoingCalls(item);
  }

  async hover(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<unknown> {
    await this.waitForReady();
    return this.operations.hover(filePath, position, content);
  }

  async typeDefinition(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CodeSnippet[]> {
    await this.waitForReady();
    return this.operations.typeDefinition(filePath, position, content);
  }

  async implementation(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CodeSnippet[]> {
    await this.waitForReady();
    return this.operations.implementation(filePath, position, content);
  }

  async documentSymbols(
    filePath: string,
    content?: string
  ): Promise<unknown[]> {
    await this.waitForReady();
    return this.operations.documentSymbols(filePath, content);
  }

  getRecentStderr(): string[] {
    return [...this.stderrBuffer];
  }

  hasCapability(capability: string): boolean {
    if (!this.initializeResult?.capabilities) return false;
    const caps = this.initializeResult.capabilities as Record<string, unknown>;
    return !!caps[capability];
  }

  async stop(): Promise<void> {
    this.resolveReady();
    try {
      if (this.connection) {
        await this.documentManager.closeAllDocuments();

        await raceWithTimeout(
          this.connection.sendRequest('shutdown'),
          5_000,
          'LSP shutdown timed out'
        );

        await this.connection.sendNotification('exit');
      }
    } catch {
      void 0;
    } finally {
      try {
        this.connection?.dispose();
      } catch {
        void 0;
      }
      this.connection = null;

      try {
        this.process?.kill();
      } catch {
        void 0;
      }
      this.process = null;
      this.initialized = false;

      this.documentManager.setConnection(null, false);
      this.operations.setConnection(null, false);
    }
  }
}
