import {
	type CallToolResult,
	type McpClient,
	type McpRequestOptions,
	type Tool as McpTool,
	type McpTransport,
	StdioTransport,
} from "./client/index.ts";
import { raceWithAbortSignal } from "@octocodeai/octocode-ai/utils/abort";
import type { OAuthChallenge } from "./client/oauth/index.ts";
import { McpCatalogCache } from "./catalog.ts";
import type { McpServerEntry } from "./config.ts";
import type { McpServerLog } from "./log.ts";
import type { McpAuthProvider, McpOAuthCredentialStore, McpOAuthSettings } from "./oauth.ts";
import {
	createConnectionAuthProvider,
	mcpOAuthSettings,
	needsSignIn,
	callWhileDownError,
	signInRequiredMessage,
	unavailableError,
	usesOAuth,
} from "./runtime/auth.ts";
import {
	CONNECT_RETRY_DELAYS_MS,
	ConnectBackoff,
	connectBackoffError,
	connectWithRetries,
	createMcpClient,
	DEFAULT_TIMEOUT_MS,
	type McpRequestHost,
	RetiringClients,
	retryAfterTimeout,
	runRequest,
	type ServerState,
	stderrTail,
	trackToolListChanges,
	trackServerWarnings,
} from "./runtime/client.ts";
import type { McpTransportFactory } from "./runtime/transport.ts";
import { errorMessage } from "@octocodeai/octocode-contracts/guards";
import type { McpToolCaller } from "./tools.ts";

export { createDefaultTransport, type McpTransportFactory } from "./runtime/transport.ts";

interface McpRuntimeOptions {
	entry: McpServerEntry;
	cwd: string;
	createTransport: McpTransportFactory;
	/** Resolves the entry each launch uses (prepared, installed) before its transport is created. */
	prepareEntry?: (entry: McpServerEntry, signal: AbortSignal) => Promise<McpServerEntry>;
	credentials: McpOAuthCredentialStore;
	providerToken?: (provider: string) => Promise<string | undefined>;
	onTools: (connection: McpServerConnection) => void;
	onChange?: (connection: McpServerConnection) => void;
	log?: McpServerLog;
}

/** One configured server. Reconnects lazily when a call finds the connection gone. */
export class McpServerConnection implements McpToolCaller {
	readonly entry: McpServerEntry;
	state: ServerState = "connecting";
	error: string | undefined;
	/** The last start answered too slowly, so it was retried once with a longer timeout (shown in the state). */
	retriedAfterTimeout = false;
	clientError: string | undefined;
	serverWarning: string | undefined;
	tools: McpTool[] = [];
	/** Server instructions from `initialize`, describing its tools as a group. */
	instructions: string | undefined;
	/** Last OAuth challenge from the server; sign-in uses its resource metadata URL and scope. */
	challenge: OAuthChallenge | undefined;
	private readonly catalog = new McpCatalogCache();
	private listRevision = 0;
	private client: McpClient | undefined;
	private opening: Promise<McpClient> | undefined;
	/** Unlike permanent shutdown, stopping an opening can be followed by a fresh reconnect. */
	private openingAbort: AbortController | undefined;
	/** Expired sessions retain their transport until requests already using it have settled. */
	private readonly retiring = new RetiringClients();
	/** Aborted by `close()`; cancels a connect in progress, including the wait between retries. */
	private readonly shutdown = new AbortController();
	/** Pauses new attempts after repeated failures to connect. */
	private readonly backoff = new ConnectBackoff();
	/** Stderr of the last stdio server that failed to connect. */
	private stderrTail: string | undefined;
	private readonly cwd: string;
	private readonly createTransport: McpTransportFactory;
	private readonly prepareEntry: McpRuntimeOptions["prepareEntry"];
	/** Stopped by `stop()` until `reconnect()`: calls do not start the server again. */
	private stopped = false;
	private readonly authProvider: McpAuthProvider | undefined;
	private readonly onTools: (connection: McpServerConnection) => void;
	private readonly onChange: ((connection: McpServerConnection) => void) | undefined;
	private readonly log: McpServerLog | undefined;

	constructor(options: McpRuntimeOptions) {
		this.entry = options.entry;
		this.cwd = options.cwd;
		this.createTransport = options.createTransport;
		this.prepareEntry = options.prepareEntry;
		this.onTools = options.onTools;
		this.onChange = options.onChange;
		this.log = options.log;
		this.authProvider = createConnectionAuthProvider({
			entry: this.entry,
			oauthUrl: this.oauthUrl,
			credentials: options.credentials,
			providerToken: options.providerToken,
			settings: () => this.oauthSettings(),
			onChallenge: (challenge) => {
				this.challenge = challenge;
			},
		});
	}

	get name(): string {
		return this.entry.name;
	}

	get idle(): boolean {
		return this.retiring.idle;
	}

	private get closed(): boolean {
		return this.shutdown.signal.aborted;
	}

	get timeoutMs(): number {
		return this.entry.config.timeout === undefined ? DEFAULT_TIMEOUT_MS : this.entry.config.timeout * 1000;
	}

	/** Server URL when the server authenticates with OAuth. */
	get oauthUrl(): string | undefined {
		return usesOAuth(this.entry) && "url" in this.entry.config ? this.entry.config.url : undefined;
	}

	oauthSettings(): McpOAuthSettings {
		return mcpOAuthSettings(this.entry);
	}

	getClient(): Promise<McpClient> {
		const unavailable = unavailableError(this.entry, { stopped: this.stopped, closed: this.closed });
		if (unavailable) return Promise.reject(unavailable);
		if (this.client?.connectionState === "connected") return Promise.resolve(this.client);
		const wait = this.opening ? 0 : this.backoff.remainingMs();
		if (wait > 0) return Promise.reject(connectBackoffError(this.entry.name, this.backoff.count, this.error, wait));
		this.opening ??= this.open().finally(() => {
			this.opening = undefined;
		});
		return this.opening;
	}

	callTool(name: string, args: Record<string, unknown>, options: McpRequestOptions): Promise<CallToolResult> {
		return runRequest(this.requests, (client) => client.callTool(name, args, options), options.signal);
	}

	/** The connection as `runRequest` uses it. */
	private readonly requests: McpRequestHost = {
		getClient: () => this.getClient(),
		retiring: this.retiring,
		expireSession: (client) => {
			if (this.client === client) {
				this.client = undefined;
				this.listRevision++;
			}
		},
		requireSignIn: async (client, error) => {
			if (!needsSignIn(error, this.authProvider)) return undefined;
			await this.dropClient(client);
			this.markNeedsAuth();
			return new Error(signInRequiredMessage(this.entry));
		},
	};

	/**
	 * The error for a call made while the connection is not up, or undefined when connected. A call does not wait: after a
	 * drop it starts the reconnect in the background (the outcome shows in `state` and `error`) and fails with how to restart.
	 */
	rejectWhileDown(): Error | undefined {
		const failure = callWhileDownError(this.entry, {
			state: this.state,
			error: this.error,
			stopped: this.stopped,
			closed: this.closed,
		});
		if (failure && this.state === "disconnected" && !this.stopped && !this.closed)
			void this.getClient().catch(() => undefined);
		return failure;
	}

	/** Cancel a connect in progress (a hung one a reconnect replaces): its callers see the failure; a later call starts anew. */
	abortOpening(): void {
		this.openingAbort?.abort();
	}

	/** Connect again with fresh credentials, for example after signing in. A connect still in progress is cancelled. */
	async reconnect(): Promise<void> {
		this.stopped = false;
		this.abortOpening();
		await this.opening?.catch(() => undefined);
		// After the cancelled attempt, whose failure counted against the backoff.
		this.backoff.reset();
		if (this.client) await this.dropClient(this.client);
		await this.getClient();
	}

	/** Stop the server for this session: its tools are withdrawn and calls fail instead of starting it; `reconnect()` starts it. */
	async stop(): Promise<void> {
		this.stopped = true;
		this.abortOpening();
		await Promise.all([this.opening?.catch(() => undefined), this.retiring.closeRetiredClients()]);
		await Promise.all([this.client && this.dropClient(this.client), this.retiring.closeRetiredClients()]);
		this.withdrawCatalog();
		this.state = "disconnected";
		this.error = undefined;
		this.changed();
	}

	get isStopped(): boolean {
		return this.stopped;
	}

	/** Disconnect after the stored credentials were removed. */
	async signOut(): Promise<void> {
		await this.opening?.catch(() => undefined);
		if (this.client) await this.dropClient(this.client);
		this.withdrawCatalog();
		if (!this.closed) this.markNeedsAuth();
	}

	/** The tools stay declared: a server that needs a sign-in again is a transient state, not a withdrawal. */
	private markNeedsAuth(): void {
		this.state = "needs-auth";
		this.error = undefined;
		this.changed();
	}

	private changed(): void {
		this.onChange?.(this);
	}

	private async dropClient(client: McpClient): Promise<void> {
		if (this.client === client) {
			this.client = undefined;
			this.listRevision++;
		}
		await client.close().catch(() => undefined);
	}

	private async open(): Promise<McpClient> {
		const openingAbort = new AbortController();
		this.openingAbort = openingAbort;
		const signal = AbortSignal.any([this.shutdown.signal, openingAbort.signal]);
		this.state = "connecting";
		this.retriedAfterTimeout = false;
		this.changed();
		const retries = "url" in this.entry.config ? CONNECT_RETRY_DELAYS_MS : [];
		try {
			return await connectWithRetries(
				retries,
				signal,
				() => {
					this.stderrTail = undefined;
					return retryAfterTimeout(
						this.timeoutMs,
						signal,
						(timeoutMs) => this.connectOnce(signal, timeoutMs),
						() => {
							this.retriedAfterTimeout = true;
							this.changed();
						},
					);
				},
				(error) => this.connectFailed(error),
			);
		} finally {
			if (this.openingAbort === openingAbort) this.openingAbort = undefined;
		}
	}

	private async connectOnce(signal: AbortSignal, timeoutMs: number): Promise<McpClient> {
		const { client, closeClient } = createMcpClient(this.entry.name, this.cwd, timeoutMs, this.log);
		this.clientError = undefined;
		trackServerWarnings(client, (line) => (this.serverWarning = line)); // starts by clearing the last one
		client.onError((error) => {
			if (signal.aborted || this.closed || (this.client && this.client !== client)) return;
			this.clientError = error.message;
		});
		const onAbort = () => void closeClient();
		signal.addEventListener("abort", onAbort, { once: true });
		let transport: McpTransport | undefined;
		const listChanges = trackToolListChanges(client, {
			isCurrent: () => this.client === client,
			refresh: () => void this.refreshTools(client),
		});
		try {
			const prepared = this.prepareEntry?.(this.entry, signal);
			const entry = prepared ? await raceWithAbortSignal(prepared, signal) : this.entry;
			transport = this.createTransport(entry, this.cwd, this.authProvider);
			await raceWithAbortSignal(client.connect(transport), signal);
			const stdio = transport instanceof StdioTransport ? transport : undefined;
			client.onClose(() => this.handleClientClose(client, stdio));
			// Only request a tool catalog when the server advertises tools.
			const tools = client.serverCapabilities?.tools ? await client.listTools() : [];
			if (signal.aborted) throw new Error("stopped while connecting");
			if (client.connectionState !== "connected") throw new Error("connection closed during setup");
			this.client = client;
			this.backoff.reset();
			this.state = "connected";
			this.error = undefined;
			this.publishCatalog(client, tools);
			if (listChanges.changedDuringConnect()) await this.refreshTools(client);
			this.changed();
			return client;
		} catch (error) {
			await closeClient();
			if (transport instanceof StdioTransport) this.stderrTail = stderrTail(transport);
			throw error;
		} finally {
			signal.removeEventListener("abort", onAbort);
		}
	}

	private connectFailed(error: unknown): Error {
		if (needsSignIn(error, this.authProvider) && !this.closed) {
			this.markNeedsAuth();
			return new Error(signInRequiredMessage(this.entry));
		}
		this.listRevision++;
		if (!this.closed) this.backoff.failed();
		this.state = this.closed ? "closed" : "failed";
		this.error = this.stderrTail ? `${errorMessage(error)}\n${this.stderrTail}` : errorMessage(error);
		this.changed();
		return new Error(`MCP server "${this.entry.name}" failed to connect: ${this.error}`);
	}

	/** The transport dropped. The tools stay declared (the cached prompt prefix holds; `rejectWhileDown` fails calls); the status shows why. */
	private handleClientClose(client: McpClient, stdio: StdioTransport | undefined): void {
		if (this.client !== client || this.closed) return;
		this.client = undefined;
		this.listRevision++;
		this.state = "disconnected";
		const stderr = stderrTail(stdio);
		this.error = stderr ? `Connection closed\n${stderr}` : "Connection closed";
		this.changed();
	}

	private publishCatalog(client: McpClient, tools: McpTool[]): void {
		const catalog = this.catalog.update(client.serverInfo, tools, client.instructions?.trim() || undefined);
		if (this.tools === catalog.tools && this.instructions === catalog.instructions) return;
		this.tools = catalog.tools;
		this.instructions = catalog.instructions;
		this.onTools(this);
	}

	/** Remove the catalog: the server is stopped, signed out or shut down, or its catalog could not be verified. A drop or a failed reconnect keeps it. */
	private withdrawCatalog(): void {
		this.listRevision++;
		if (this.tools.length === 0 && this.instructions === undefined) return;
		this.tools = [];
		this.instructions = undefined;
		this.onTools(this);
	}

	private async refreshTools(client: McpClient): Promise<void> {
		const revision = ++this.listRevision;
		try {
			const tools = client.serverCapabilities?.tools ? await client.listTools() : [];
			if (this.client !== client || this.closed || revision !== this.listRevision) return;
			this.publishCatalog(client, tools);
			this.error = undefined;
		} catch (error) {
			if (this.client !== client || this.closed || revision !== this.listRevision) return;
			// Unlike a drop, the connection is up but its catalog is unverified, so its tools are withdrawn.
			this.withdrawCatalog();
			this.error = `Failed to refresh tools: ${errorMessage(error)}`;
		}
		this.changed();
	}

	/** Resolves once no transport of this server is open, including one that was still connecting. */
	async close(): Promise<void> {
		this.shutdown.abort();
		this.withdrawCatalog();
		this.catalog.clear();
		this.state = "closed";
		this.changed();
		const client = this.client;
		this.client = undefined;
		await Promise.all([
			client?.close().catch(() => undefined),
			this.opening?.catch(() => undefined),
			this.retiring.closeRetiredClients(),
		]);
		// A refresh the server already answered may have rotated the refresh token; exiting before the
		// new tokens are saved would lose the grant.
		await this.authProvider?.settled();
	}
}
