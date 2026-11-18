import {
	InsecureTokenEndpointError,
	IssuerMismatchError,
	RegistrationRejectedError,
} from "@modelcontextprotocol/client";
import type { McpFetch } from "../auth-provider.ts";
import { isObject } from "../protocol/jsonrpc.ts";
import { OAuthInsecureEndpointError, OAuthIssuerMismatchError, OAuthRegistrationError } from "./errors.ts";

/** Preserve the host's public error classes while the SDK owns OAuth protocol decisions. */
export function oauthError(error: unknown): unknown {
	if (error instanceof IssuerMismatchError) return new OAuthIssuerMismatchError(error.expected, error.received);
	if (error instanceof InsecureTokenEndpointError) return new OAuthInsecureEndpointError(error.tokenEndpoint);
	if (error instanceof RegistrationRejectedError) return new OAuthRegistrationError(error.status, error.body);
	return error;
}

export async function sdkOAuth<T>(operation: () => Promise<T>): Promise<T> {
	try {
		return await operation();
	} catch (error) {
		throw oauthError(error);
	}
}

/** Every SDK discovery/token request participates in the host flow's cancellation. */
export function oauthFetch(fetch: McpFetch = globalThis.fetch, signal?: AbortSignal): McpFetch {
	return async (input, init) => {
		const signals = [signal, init?.signal].filter((value): value is AbortSignal => !!value);
		const combined = signals.length > 1 ? AbortSignal.any(signals) : signals[0];
		combined?.throwIfAborted();
		let onAbort: (() => void) | undefined;
		const pending = fetch(input, { ...init, credentials: "omit", signal: combined }).then(async (response) => {
			if (combined?.aborted) {
				try {
					await response.body?.cancel();
				} catch {
					// The response is abandoned either way; the abort below is the error that matters.
				}
				combined.throwIfAborted();
			}
			const wrapped =
				combined && response.body
					? new Response(signalBody(response.body, combined), {
							status: response.status,
							statusText: response.statusText,
							headers: response.headers,
						})
					: response;
			return oauthResponse(wrapped, response);
		});
		try {
			return combined
				? await Promise.race([
						pending,
						new Promise<never>((_, reject) => {
							onAbort = () => reject(combined.reason);
							combined.addEventListener("abort", onAbort, { once: true });
							if (combined.aborted) onAbort();
						}),
					])
				: await pending;
		} finally {
			if (onAbort) combined?.removeEventListener("abort", onAbort);
		}
	};
}

/** Keep SDK redirect checks intact while normalizing JSON, including a cloned error response. */
function oauthResponse(response: Response, original: Response): Response {
	return new Proxy(response, {
		get(target, property) {
			if (property === "url" || property === "redirected" || property === "type") return original[property];
			if (property === "json") return async () => normalizeOptionalFields(await target.json());
			if (property === "clone") return () => oauthResponse(target.clone(), original);
			const value: unknown = Reflect.get(target, property, target);
			return typeof value === "function" ? value.bind(target) : value;
		},
	});
}

/** An injected fetch may ignore its signal after headers. Cancel its reader and error SDK body reads immediately. */
function signalBody(body: ReadableStream<Uint8Array>, signal: AbortSignal): ReadableStream<Uint8Array> {
	const reader = body.getReader();
	let finished = false;
	let controller: ReadableStreamDefaultController<Uint8Array>;
	const finish = () => {
		finished = true;
		signal.removeEventListener("abort", onAbort);
	};
	const cancel = (reason: unknown) => {
		finish();
		return reader
			.cancel(reason)
			.catch(() => {})
			.finally(() => reader.releaseLock());
	};
	const onAbort = () => {
		if (finished) return;
		controller.error(signal.reason);
		void cancel(signal.reason);
	};
	return new ReadableStream<Uint8Array>({
		start(value) {
			controller = value;
			signal.addEventListener("abort", onAbort, { once: true });
			if (signal.aborted) onAbort();
		},
		async pull(value) {
			try {
				const next = await reader.read();
				if (finished) return;
				if (next.done) {
					finish();
					reader.releaseLock();
					value.close();
				} else value.enqueue(next.value);
			} catch (error) {
				if (finished) return;
				finish();
				reader.releaseLock();
				value.error(error);
			}
		},
		cancel,
	});
}

function normalizeOptionalFields(value: unknown): unknown {
	if (!isObject(value)) return value;
	const normalized = { ...value };
	for (const key of ["scope", "refresh_token", "id_token", "client_secret", "expires_in", "registration_endpoint"]) {
		if (normalized[key] === null || normalized[key] === "") delete normalized[key];
	}
	for (const key of [
		"authorization_servers",
		"scopes_supported",
		"grant_types_supported",
		"token_endpoint_auth_methods_supported",
		"code_challenge_methods_supported",
	]) {
		if (normalized[key] === null) delete normalized[key];
	}
	for (const key of ["access_token", "token_type", "client_id"]) {
		if (normalized[key] === "") throw new Error(`Invalid ${key}`);
	}
	return normalized;
}
