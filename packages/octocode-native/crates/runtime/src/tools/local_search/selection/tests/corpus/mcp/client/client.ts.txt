import {
	Client,
	ProtocolError,
	SdkError,
	SdkErrorCode,
	type StandardSchemaV1,
	type ClientCapabilities,
} from "@modelcontextprotocol/client";
import * as catalog from "./client/catalog.ts";
import { isTool, listAll, toTool, validateCallToolResult } from "./client/results.ts";
import { SdkTransportAdapter } from "./client/sdk-transport.ts";
import type {
	ClientState,
	CloseListener,
	ErrorListener,
	McpClientOptions,
	McpRequestOptions,
	NotificationListener,
	RequestHandler,
} from "./client/types.ts";
import type { CallToolResult, ContentBlock } from "./protocol/content.ts";
import {
	McpAbortError,
	McpConnectionClosedError,
	McpError,
	McpTimeoutError,
	toError,
	isObject,
} from "./protocol/jsonrpc.ts";
import { SUPPORTED_PROTOCOL_VERSIONS, type InitializeResult, type Tool } from "./protocol/types.ts";
import type { McpTransport } from "./transports/transport.ts";

// Extension methods are application-defined. The tool/content projection below remains the host's trust boundary.
const extensionResult: StandardSchemaV1<unknown, unknown> = {
	"~standard": { version: 1, vendor: "octocode", validate: (value) => ({ value }) },
};
// The SDK always arms an idle timer; its longest non-overflowing platform delay preserves disabled/very long settings.
const MAX_IDLE_TIMEOUT_MS = 2_147_483_647;
/** The SDK's reports of a response or progress notification for a request it no longer tracks. */
const LATE_REPLY = /^Received a (?:response for an unknown message ID|progress notification for an unknown token):/;

/** Long absolute deadlines are armed in platform-sized chunks instead of overflowing into a 1 ms timer. */
function absoluteDeadline(timeout: number) {
	const controller = new AbortController();
	let timer: ReturnType<typeof setTimeout> | undefined;
	const expires = Date.now() + timeout;
	const arm = () => {
		const remaining = expires - Date.now();
		if (remaining <= 0) controller.abort(new McpTimeoutError(timeout));
		else timer = setTimeout(arm, Math.min(remaining, MAX_IDLE_TIMEOUT_MS));
	};
	if (Number.isFinite(timeout) && timeout > 0) arm();
	return { signal: controller.signal, dispose: () => clearTimeout(timer) };
}

/** Application compatibility facade; the official SDK owns handshake, dispatch, cancellation and deadlines. */
export class McpClient {
	readonly options: Readonly<McpClientOptions>;
	private readonly sdk: Client;
	private state: ClientState = "idle";
	private adapter?: SdkTransportAdapter;
	private readonly handlers = new Map<string, RequestHandler>();
	private readonly notificationListeners = new Map<string, Set<NotificationListener>>();
	private readonly errorListeners = new Set<ErrorListener>();
	private readonly closeListeners = new Set<CloseListener>();

	constructor(options: McpClientOptions) {
		this.options = Object.freeze({ ...options });
		this.sdk = new Client(
			{ name: options.name, version: options.version, ...(options.title ? { title: options.title } : {}) },
			{
				capabilities: {
					...options.capabilities,
					...(options.roots ? { roots: options.capabilities?.roots ?? {} } : {}),
				} as ClientCapabilities,
				supportedProtocolVersions: options.protocolVersion
					? [
							options.protocolVersion,
							...SUPPORTED_PROTOCOL_VERSIONS.filter((version) => version !== options.protocolVersion),
						]
					: [...SUPPORTED_PROTOCOL_VERSIONS],
			},
		);
		this.sdk.onerror = (error) => this.emitError(error);
		this.sdk.onclose = () => this.markClosed();
		this.sdk.fallbackRequestHandler = async (request, context) => {
			const handler = this.handlers.get(request.method);
			if (!handler) throw new ProtocolError(-32601, `Method not found: ${request.method}`);
			try {
				const result = (await handler(request.params, { signal: context.mcpReq.signal })) ?? {};
				if (!isObject(result)) throw new ProtocolError(-32603, "MCP request handler must return an object");
				return result;
			} catch (error) {
				if (error instanceof McpError) throw new ProtocolError(error.code, error.message, error.data);
				throw error;
			}
		};
		const roots = options.roots;
		if (roots)
			this.sdk.setRequestHandler("roots/list", async () => ({
				roots: [...(typeof roots === "function" ? await roots() : roots)],
			}));
	}

	get connectionState(): ClientState {
		return this.state;
	}
	get serverInfo() {
		return this.sdk.getServerVersion();
	}
	get serverCapabilities() {
		return this.sdk.getServerCapabilities();
	}
	get instructions() {
		return this.sdk.getInstructions();
	}
	get protocolVersion() {
		return this.sdk.getNegotiatedProtocolVersion();
	}

	async connect(transport: McpTransport): Promise<InitializeResult> {
		if (this.state !== "idle") throw new Error(`Cannot connect MCP client in ${this.state} state`);
		this.state = "connecting";
		const adapter = new SdkTransportAdapter(transport, (method, params) => {
			for (const listener of this.notificationListeners.get(method) ?? []) {
				try {
					listener(params);
				} catch (error) {
					this.emitError(error);
				}
			}
		});
		this.adapter = adapter;
		const total = this.options.maxTotalTimeoutMs ?? 30 * 60_000;
		const deadline = absoluteDeadline(total);
		try {
			await this.sdk.connect(adapter, this.requestOptions({ signal: deadline.signal }));
			if ((this.state as ClientState) === "closed")
				throw new McpConnectionClosedError("MCP client closed while connecting");
			this.state = "connected";
			return {
				protocolVersion: this.protocolVersion!,
				capabilities: this.serverCapabilities!,
				serverInfo: this.serverInfo!,
				...(this.instructions === undefined ? {} : { instructions: this.instructions }),
			};
		} catch (error) {
			await this.close().catch(() => {});
			if (deadline.signal.aborted) throw deadline.signal.reason;
			throw this.mapError(error, {});
		} finally {
			deadline.dispose();
		}
	}

	async request<Result = unknown>(
		method: string,
		params?: Record<string, unknown>,
		options: McpRequestOptions = {},
	): Promise<Result> {
		this.requireConnected();
		if (options.signal?.aborted) throw new McpAbortError(undefined, options.signal.reason);
		const total = options.maxTotalTimeoutMs ?? this.options.maxTotalTimeoutMs ?? 30 * 60_000;
		const deadline = absoluteDeadline(total);
		const signal = options.signal ? AbortSignal.any([options.signal, deadline.signal]) : deadline.signal;
		try {
			return (await this.sdk.request(
				{ method, ...(params === undefined ? {} : { params }) },
				extensionResult,
				this.requestOptions({ ...options, signal }),
			)) as Result;
		} catch (error) {
			if (deadline.signal.aborted && !options.signal?.aborted) throw deadline.signal.reason;
			throw this.mapError(error, options);
		} finally {
			deadline.dispose();
		}
	}

	async notify(method: string, params?: Record<string, unknown>): Promise<void> {
		this.requireConnected();
		await this.sdk.notification({ method, ...(params === undefined ? {} : { params }) });
	}

	setRequestHandler(method: string, handler: RequestHandler): () => void {
		this.handlers.set(method, handler);
		return () => {
			if (this.handlers.get(method) === handler) this.handlers.delete(method);
		};
	}

	onNotification(method: string, listener: NotificationListener): () => void {
		const listeners = this.notificationListeners.get(method) ?? new Set<NotificationListener>();
		this.notificationListeners.set(method, listeners);
		listeners.add(listener);
		return () => {
			listeners.delete(listener);
			if (!listeners.size) this.notificationListeners.delete(method);
		};
	}
	onError(listener: ErrorListener): () => void {
		this.errorListeners.add(listener);
		return () => this.errorListeners.delete(listener);
	}
	onClose(listener: CloseListener): () => void {
		this.closeListeners.add(listener);
		return () => this.closeListeners.delete(listener);
	}
	async ping(options: McpRequestOptions = {}): Promise<void> {
		await this.request("ping", undefined, options);
	}
	async listTools(options: McpRequestOptions = {}): Promise<Tool[]> {
		return (
			await listAll(
				(method, params, requestOptions) => this.request(method, params, requestOptions),
				"tools/list",
				"tools",
				isTool,
				options,
			)
		).map(toTool);
	}
	async callTool(
		name: string,
		args?: Record<string, unknown>,
		options: McpRequestOptions = {},
	): Promise<CallToolResult> {
		return validateCallToolResult(
			await this.request("tools/call", { name, ...(args === undefined ? {} : { arguments: args }) }, options),
		);
	}
	/** Resources and templates the server lists; none when it does not declare the `resources` capability. */
	async listResources(options: McpRequestOptions = {}): Promise<catalog.McpResource[]> {
		return this.serverCapabilities?.resources ? catalog.listResources(this.lister, options) : [];
	}
	async listResourceTemplates(options: McpRequestOptions = {}): Promise<catalog.McpResourceTemplate[]> {
		return this.serverCapabilities?.resources ? catalog.listResourceTemplates(this.lister, options) : [];
	}
	async readResource(uri: string, options: McpRequestOptions = {}): Promise<ContentBlock[]> {
		this.requireCapability("resources");
		return catalog.readResourceContent(await this.request("resources/read", { uri }, options));
	}
	/** Prompts the server lists; none when it does not declare the `prompts` capability. */
	async listPrompts(options: McpRequestOptions = {}): Promise<catalog.McpPrompt[]> {
		return this.serverCapabilities?.prompts ? catalog.listPrompts(this.lister, options) : [];
	}
	async getPrompt(
		name: string,
		args: Record<string, string>,
		options: McpRequestOptions = {},
	): Promise<catalog.McpPromptMessage[]> {
		this.requireCapability("prompts");
		return catalog.promptMessages(await this.request("prompts/get", { name, arguments: args }, options));
	}
	async close(): Promise<void> {
		this.markClosed();
		try {
			await this.sdk.close();
		} finally {
			await this.adapter?.close();
		}
	}

	private requestOptions(options: McpRequestOptions) {
		const total = options.maxTotalTimeoutMs ?? this.options.maxTotalTimeoutMs ?? 30 * 60_000;
		const idle = options.timeoutMs ?? this.options.requestTimeoutMs ?? 30_000;
		return {
			signal: options.signal,
			timeout: Number.isFinite(idle) && idle > 0 ? Math.min(idle, MAX_IDLE_TIMEOUT_MS) : MAX_IDLE_TIMEOUT_MS,
			maxTotalTimeout: Number.isFinite(total) && total > 0 ? total : undefined,
			resetTimeoutOnProgress: true,
			onprogress: options.onProgress
				? (progress: Parameters<NonNullable<McpRequestOptions["onProgress"]>>[0]) => {
						try {
							options.onProgress!(progress);
						} catch (error) {
							this.emitError(error);
						}
					}
				: undefined,
		};
	}
	private mapError(error: unknown, options: McpRequestOptions): Error {
		if (options.signal?.aborted) return new McpAbortError(undefined, options.signal.reason);
		if (error instanceof ProtocolError) return new McpError(error.code, error.message, error.data);
		if (error instanceof SdkError && error.code === SdkErrorCode.ConnectionClosed)
			return new McpConnectionClosedError();
		if (error instanceof SdkError && error.code === SdkErrorCode.RequestTimeout) {
			const timeout =
				isObject(error.data) && typeof error.data.timeout === "number"
					? error.data.timeout
					: (options.timeoutMs ?? this.options.requestTimeoutMs ?? 30_000);
			return new McpTimeoutError(timeout);
		}
		return toError(error);
	}
	private readonly lister = (method: string, params: Record<string, unknown> | undefined, options: McpRequestOptions) =>
		this.request(method, params, options);
	private requireCapability(name: "resources" | "prompts"): void {
		this.requireConnected();
		if (!this.serverCapabilities?.[name])
			throw new McpError(-32601, `MCP server "${this.serverInfo?.name ?? "server"}" offers no ${name}`);
	}
	private requireConnected(): void {
		if (this.state !== "connected") throw new McpConnectionClosedError(`MCP client is ${this.state}`);
	}
	private markClosed(): void {
		if (this.state === "closed") return;
		this.state = "closed";
		for (const listener of Array.from(this.closeListeners)) {
			try {
				listener();
			} catch (error) {
				this.emitError(error);
			}
		}
	}
	private emitError(error: unknown): void {
		// A server may still answer or report progress on a request this client cancelled or timed out (MCP
		// cancellation: the sender ignores later responses). Nothing waits for them, so they are not errors.
		if (error instanceof Error && LATE_REPLY.test(error.message)) return;
		for (const listener of this.errorListeners) {
			try {
				listener(toError(error));
			} catch {
				/* Error observers cannot interrupt protocol cleanup. */
			}
		}
	}
}
