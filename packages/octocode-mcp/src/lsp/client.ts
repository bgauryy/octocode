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

  constructor(config: LanguageServerConfig) {
    this.config = config;
    this.documentManager = new LSPDocumentManager(config);
    this.operations = new LSPOperations(
      this.documentManager,
      config.workspaceRoot
    );
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

    this.process.on('error', () => {});

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

    try {
      this.connection = createMessageConnection(
        new StreamMessageReader(this.process.stdout),
        new StreamMessageWriter(this.process.stdin)
      );

      this.registerServerInitiatedHandlers(this.connection);
      this.connection.listen();

      await this.initialize();
    } catch (error) {
      await this.stop();
      throw error;
    }
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
    connection.onNotification('$/progress', () => undefined);
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

    this.documentManager.setConnection(this.connection, this.initialized);
    this.operations.setConnection(this.connection, this.initialized);
  }

  async openDocument(filePath: string): Promise<void> {
    return this.documentManager.openDocument(filePath);
  }

  async closeDocument(filePath: string): Promise<void> {
    return this.documentManager.closeDocument(filePath);
  }

  async gotoDefinition(
    filePath: string,
    position: ExactPosition
  ): Promise<CodeSnippet[]> {
    return this.operations.gotoDefinition(filePath, position);
  }

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

  async prepareCallHierarchy(
    filePath: string,
    position: ExactPosition
  ): Promise<CallHierarchyItem[]> {
    return this.operations.prepareCallHierarchy(filePath, position);
  }

  async getIncomingCalls(item: CallHierarchyItem): Promise<IncomingCall[]> {
    return this.operations.getIncomingCalls(item);
  }

  async getOutgoingCalls(item: CallHierarchyItem): Promise<OutgoingCall[]> {
    return this.operations.getOutgoingCalls(item);
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
