/** Connection lifecycle of the configured servers: start, wait, sign in, reconnect, enable and disable. */

import type { McpContext } from "./host/types.ts";
import { hasDirectTools } from "./exposure.ts";
import { McpSignInCancelledError, type McpSignInPrompt, signInMcpServer } from "./oauth.ts";
import { saveConfig } from "./persistence.ts";
import { loadMcpRuntime } from "./runtime.lazy.ts";
import type { McpServerConnection } from "./runtime.ts";
import { errorMessage } from "@octocodeai/octocode-contracts/guards";
import { describeState, isEnabled, type McpServer } from "./server.ts";
import type { McpExtensionState } from "./state.ts";
import { hideTools, registerTools } from "./tool-registry.ts";

function storedTokens(state: McpExtensionState, connection: McpServerConnection): string {
	const url = connection.oauthUrl;
	const { credentials } = state;
	return url && credentials ? JSON.stringify(credentials.tokens(connection.name, url) ?? null) : "null";
}

function onConnectionChange(state: McpExtensionState, connection: McpServerConnection): void {
	if (connection.state !== "needs-auth") state.tokensAtSignIn.delete(connection);
	else if (!state.tokensAtSignIn.has(connection)) state.tokensAtSignIn.set(connection, storedTokens(state, connection));
	state.emitChange();
}

/** Reconnect servers that need a sign-in when their credentials were stored since. */
export async function reconnectSignedIn(state: McpExtensionState): Promise<void> {
	const signedIn = [...state.tokensAtSignIn].filter(
		([connection, tokens]) => storedTokens(state, connection) !== tokens,
	);
	if (signedIn.length === 0) return;
	for (const [connection] of signedIn) state.tokensAtSignIn.delete(connection);
	await Promise.allSettled(signedIn.map(([connection]) => connection.reconnect()));
}

/** Create the server's connection, loading the MCP runtime on first use. */
async function createConnection(
	state: McpExtensionState,
	server: McpServer,
	isCurrent: () => boolean,
): Promise<McpServerConnection | undefined> {
	const { options } = state;
	const { createServerConnection } = await loadMcpRuntime();
	if (!isCurrent()) return undefined;
	const connection = createServerConnection({
		entry: server.entry,
		cwd: state.sessionCwd,
		createTransport: options.createTransport,
		prepareLaunch: state.prepareLaunch,
		onLaunch: (launched, prepared) => state.preparedLaunches.set(launched, prepared),
		credentials: state.getCredentials(),
		providerToken: async (provider) => state.modelRegistry?.getApiKeyForProvider(provider),
		log: state.getServerLog(),
		onTools: (connection) => {
			if (isCurrent()) {
				for (const current of state.connections())
					if (current === connection || current.state === "connected") registerTools(state, current);
			}
		},
		onChange: (connection) => {
			if (isCurrent()) onConnectionChange(state, connection);
			else state.tokensAtSignIn.delete(connection);
		},
	});
	server.connection = connection;
	state.emitChange();
	return connection;
}

/**
 * Connect the server in the background. `server.ready` settles when it connected or failed;
 * failures show in its state. It starts after `after`, unless disabled, replaced, or shut down
 * meanwhile. The returned promise rejects when the MCP runtime cannot be loaded.
 */
export function startConnection(
	state: McpExtensionState,
	server: McpServer,
	after?: Promise<unknown>,
	/** False for a start that does not choose the server for the session (a hook's call): it stays `unselected`. */
	chosen = true,
): Promise<void> {
	const { signal } = state.session;
	const attempt = Symbol();
	server.attempt = attempt;
	if (chosen) server.unselected = undefined;
	const isCurrent = () =>
		!signal.aborted && server.attempt === attempt && isEnabled(server) && state.servers.includes(server);
	const ready = (async () => {
		await after;
		if (!isCurrent()) return;
		const connection = await createConnection(state, server, isCurrent);
		if (!connection || !isCurrent()) return;
		await connection.getClient().catch(() => undefined);
	})();
	server.ready = ready.catch(() => undefined);
	return ready;
}

/**
 * Wait for the latest connection attempts, including ones queued while waiting, or an abort. Returns the servers whose
 * attempt had not settled when the wait ended.
 */
export async function waitForServers(
	state: McpExtensionState,
	waiting: readonly McpServer[],
	stop: AbortSignal | undefined,
): Promise<McpServer[]> {
	const settled = new Set<Promise<void>>();
	const stillPending = (attempts: { server: McpServer; ready: Promise<void> }[]) =>
		attempts.flatMap(({ server, ready }) => (settled.has(ready) || !isLive(state, server) ? [] : [server]));
	for (;;) {
		const ready = waiting.flatMap((server) =>
			isLive(state, server) && server.ready ? [{ server, ready: server.ready }] : [],
		);
		if (ready.length === 0 || stop?.aborted) return stillPending(ready);
		for (const { ready: attempt } of ready) void attempt.then(() => settled.add(attempt));
		let onAbort: (() => void) | undefined;
		await Promise.race([
			Promise.all(ready.map((attempt) => attempt.ready)),
			new Promise<void>((resolve) => {
				onAbort = () => resolve();
				stop?.addEventListener("abort", onAbort, { once: true });
			}),
		]);
		if (onAbort) stop?.removeEventListener("abort", onAbort);
		// Done when aborted, or when no attempt was replaced by a newer one while waiting.
		if (
			stop?.aborted ||
			ready.every((attempt) => !isLive(state, attempt.server) || attempt.ready === attempt.server.ready)
		)
			return stillPending(ready);
	}
}

function isLive(state: McpExtensionState, server: McpServer): boolean {
	return isEnabled(server) && state.servers.includes(server);
}

/**
 * The first prompt waits for servers whose tools are declared to the model, so they are declared in its first request
 * and every session with the same servers starts from the same cached prefix, but not indefinitely: a slow or hanging
 * server must not hold up the prompt.
 */
export async function waitForDirectServers(state: McpExtensionState, ctx: McpContext): Promise<void> {
	if (state.waitedForStartup) return;
	state.waitedForStartup = true;
	const ready = state.servers.flatMap((server) =>
		isEnabled(server) && hasDirectTools(server.entry) && server.ready ? [server.ready] : [],
	);
	if (ready.length === 0 || ctx.signal?.aborted || state.session.signal.aborted) return;
	const signal = AbortSignal.any([state.session.signal, ...(ctx.signal ? [ctx.signal] : [])]);
	let abort: (() => void) | undefined;
	let timer: NodeJS.Timeout | undefined;
	const finished = await Promise.race([
		Promise.all(ready).then(() => true),
		new Promise<boolean>((resolve) => {
			timer = setTimeout(() => resolve(false), state.startupWaitMs);
			abort = () => resolve(true);
			signal.addEventListener("abort", abort, { once: true });
		}),
	]);
	clearTimeout(timer);
	if (abort) signal.removeEventListener("abort", abort);
	if (!finished) {
		ctx.ui.notify("MCP servers are still connecting; their tools become available once connected.", "info");
	}
}

/** Sign in, then reconnect. Returns an error message on failure. `cancel` or the session's end stops the sign-in. */
export async function signIn(
	state: McpExtensionState,
	server: McpServer,
	prompt: McpSignInPrompt,
	cancel?: AbortSignal,
): Promise<string | undefined> {
	const connection = server.connection;
	const url = connection?.oauthUrl;
	if (!connection || !url) return `MCP server "${server.entry.name}" does not use OAuth.`;
	const signal = cancel ? AbortSignal.any([state.session.signal, cancel]) : state.session.signal;
	try {
		await state.track(
			signInMcpServer({
				serverUrl: url,
				store: state.getCredentials().forServer(server.entry.name, url),
				settings: connection.oauthSettings(),
				challenge: connection.challenge,
				prompt,
				signal,
			}),
		);
	} catch (error) {
		if (error instanceof McpSignInCancelledError) return "Sign-in cancelled.";
		return `Sign-in failed: ${errorMessage(error)}`;
	}
	// The challenge that asked for this sign-in (for example for more scope) is answered.
	connection.challenge = undefined;
	try {
		await connection.reconnect();
	} catch (error) {
		return `Signed in, but ${errorMessage(error)}`;
	}
	return undefined;
}

export async function signOut(state: McpExtensionState, server: McpServer): Promise<boolean> {
	const connection = server.connection;
	const url = connection?.oauthUrl;
	if (!connection || !url) return false;
	const removed = state.getCredentials().remove(server.entry.name, url);
	await connection.signOut();
	return removed;
}

export async function reconnect(server: McpServer): Promise<string | undefined> {
	const connection = server.connection;
	if (!connection) return `MCP server "${server.entry.name}" is ${describeState(server, false)}.`;
	const previous = server.ready;
	// A hung connect is cancelled rather than waited out; `previous` then settles at once.
	connection.abortOpening();
	const ready = (async () => {
		await previous;
		await connection.reconnect();
	})();
	server.ready = ready.catch(() => undefined);
	try {
		await ready;
		return undefined;
	} catch (error) {
		return errorMessage(error);
	}
}

/** Returns an error message when the config could not be saved; connection errors show in the state. */
export async function setEnabled(
	state: McpExtensionState,
	server: McpServer,
	enabled: boolean,
	inProject = false,
): Promise<string | undefined> {
	const wasEnabled = isEnabled(server);
	const failed = saveConfig(state, server, { enabled }, inProject);
	if (failed) return failed;
	if (enabled && wasEnabled) {
		state.emitChange();
		return undefined;
	}
	if (!enabled) {
		const connection = server.connection;
		server.attempt = undefined;
		server.connection = undefined;
		if (connection) {
			// A reconnect may already have detached its old client while awaiting transport
			// shutdown. Closing the connection alone does not wait for that reconnect.
			server.closing = Promise.all([connection.close(), server.ready]).then(() => undefined);
		}
		const closing = server.closing;
		hideTools(state, server.entry.name);
		state.emitChange();
		try {
			await closing;
		} finally {
			if (server.closing === closing) server.closing = undefined;
		}
		return undefined;
	}
	await startConnection(state, server, server.closing);
	return undefined;
}
