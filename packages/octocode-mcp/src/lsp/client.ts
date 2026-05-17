/**
 * LSP Client - Spawns and communicates with language servers
 * Uses vscode-jsonrpc for JSON-RPC communication over stdin/stdout
 * @module lsp/client
 */

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
} from '../utils/exec/spawn.js';
import { buildInitializeParams } from './initParams.js';
import { toUri } from './uri.js';

/**
 * Max stderr lines we retain per LSP client. Bounded to keep memory low
 * — last N lines are enough to surface the cause of an initialize/spawn
 * failure (T1.4 — stop silently swallowing stderr).
 */
const STDERR_RETENTION_LINES = 200;

/**
 * Race a promise against a timeout, properly cleaning up the timer
 * when the main promise settles (win or lose). This prevents the
 * "dangling setTimeout" leak that plain Promise.race + setTimeout causes.
 */
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

/**
 * LSP Client class
 * Manages connection to a language server process
 */
export class LSPClient {
  private process: ChildProcess | null = null;
  private connection: MessageConnection | null = null;
  private initialized = false;
  private config: LanguageServerConfig;
  private initializeResult: InitializeResult | null = null;
  private documentManager: LSPDocumentManager;
  private operations: LSPOperations;
  private stderrBuffer: string[] = [];

  constructor(config: LanguageServerConfig) {
    this.config = config;
    this.documentManager = new LSPDocumentManager(config);
    this.operations = new LSPOperations(
      this.documentManager,
      config.workspaceRoot
    );
  }

  /**
   * Start the language server and initialize connection.
   * If initialization fails, the spawned process is cleaned up to prevent leaks.
   */
  async start(): Promise<void> {
    if (this.process) {
      throw new Error('LSP client already started');
    }

    // Spawn the language server process
    this.process = spawn(this.config.command, this.config.args ?? [], {
      cwd: this.config.workspaceRoot,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: buildChildProcessEnv({}, TOOLING_ALLOWED_ENV_VARS),
    });

    if (!this.process.stdin || !this.process.stdout) {
      // Kill orphaned process before throwing
      try {
        this.process.kill();
      } catch {
        // Process may not support kill (e.g. failed spawn) — ignore
      }
      this.process = null;
      throw new Error('Failed to create language server process pipes');
    }

    // Handle process errors silently - errors propagate through the connection
    this.process.on('error', () => {
      // Errors are handled by the connection layer
    });

    // Capture (but cap) stderr — surfaces real failures in error messages
    // without bloating memory. See STDERR_RETENTION_LINES. Guarded so
    // partial/mock streams (e.g. in unit tests) don't blow up the spawn.
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

    // Create JSON-RPC connection and initialize — clean up on any failure
    try {
      this.connection = createMessageConnection(
        new StreamMessageReader(this.process.stdout),
        new StreamMessageWriter(this.process.stdin)
      );

      this.registerServerInitiatedHandlers(this.connection);
      this.connection.listen();

      await this.initialize();
    } catch (error) {
      // Kill the spawned process and clean up connection to prevent leaks
      await this.stop();
      throw error;
    }
  }

  /**
   * Register handlers for requests/notifications initiated by language servers.
   * TypeScript's language server asks clients for workspace configuration when
   * we advertise `workspace.configuration`; returning empty settings keeps the
   * headless client protocol-compliant without adding user-facing schema knobs.
   */
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
    connection.onNotification('$/progress', () => undefined);
  }

  /**
   * Initialize the language server
   */
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

    // Send initialized notification
    const initializedParams: InitializedParams = {};
    await this.connection.sendNotification('initialized', initializedParams);

    this.initialized = true;

    // Update document manager and operations with connection
    this.documentManager.setConnection(this.connection, this.initialized);
    this.operations.setConnection(this.connection, this.initialized);
  }

  /**
   * Open a text document (required before LSP operations)
   */
  async openDocument(filePath: string): Promise<void> {
    return this.documentManager.openDocument(filePath);
  }

  /**
   * Close a text document
   */
  async closeDocument(filePath: string): Promise<void> {
    return this.documentManager.closeDocument(filePath);
  }

  /**
   * Go to definition
   */
  async gotoDefinition(
    filePath: string,
    position: ExactPosition
  ): Promise<CodeSnippet[]> {
    return this.operations.gotoDefinition(filePath, position);
  }

  /**
   * Find references
   */
  async findReferences(
    filePath: string,
    position: ExactPosition,
    includeDeclaration = true
  ): Promise<CodeSnippet[]> {
    return this.operations.findReferences(
      filePath,
      position,
      includeDeclaration
    );
  }

  /**
   * Prepare call hierarchy (get item at position)
   */
  async prepareCallHierarchy(
    filePath: string,
    position: ExactPosition
  ): Promise<CallHierarchyItem[]> {
    return this.operations.prepareCallHierarchy(filePath, position);
  }

  /**
   * Get incoming calls (who calls this function)
   */
  async getIncomingCalls(item: CallHierarchyItem): Promise<IncomingCall[]> {
    return this.operations.getIncomingCalls(item);
  }

  /**
   * Get outgoing calls (what this function calls)
   */
  async getOutgoingCalls(item: CallHierarchyItem): Promise<OutgoingCall[]> {
    return this.operations.getOutgoingCalls(item);
  }

  /**
   * Return the last N stderr lines emitted by the language server.
   * Useful for surfacing the real cause of an initialize/spawn failure.
   * Buffer is capped at {@link STDERR_RETENTION_LINES} to bound memory.
   */
  getRecentStderr(): string[] {
    return [...this.stderrBuffer];
  }

  /**
   * Check if server supports a capability
   */
  hasCapability(capability: string): boolean {
    if (!this.initializeResult?.capabilities) return false;
    const caps = this.initializeResult.capabilities as Record<string, unknown>;
    return !!caps[capability];
  }

  /**
   * Shutdown and close the language server.
   * Always cleans up process and connection, even if partially initialized.
   */
  async stop(): Promise<void> {
    try {
      if (this.connection) {
        // Close all open documents
        await this.documentManager.closeAllDocuments();

        // Send shutdown request (with timeout to avoid hanging)
        await raceWithTimeout(
          this.connection.sendRequest('shutdown'),
          5_000,
          'LSP shutdown timed out'
        );

        // Send exit notification
        await this.connection.sendNotification('exit');
      }
    } catch {
      // Ignore errors during shutdown — cleanup is more important
    } finally {
      // Dispose connection (may throw if already disposed — safe to ignore)
      try {
        this.connection?.dispose();
      } catch {
        // Already disposed or never created
      }
      this.connection = null;

      // Kill process (may throw ESRCH if already exited — safe to ignore)
      try {
        this.process?.kill();
      } catch {
        // Process already exited
      }
      this.process = null;
      this.initialized = false;

      // Clear connection from managers
      this.documentManager.setConnection(null, false);
      this.operations.setConnection(null, false);
    }
  }
}
