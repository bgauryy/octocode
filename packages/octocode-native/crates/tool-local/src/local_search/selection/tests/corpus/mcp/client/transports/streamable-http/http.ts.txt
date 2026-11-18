export const MAX_ERROR_BODY_BYTES = 8 * 1024;
const ERROR_MESSAGE_BODY_CHARS = 500;

export class McpHttpError extends Error {
	readonly status: number;
	readonly body: string;

	constructor(status: number, message: string, body = "") {
		super(message);
		this.name = "McpHttpError";
		this.status = status;
		this.body = body;
	}
}

export class McpAuthRequiredError extends McpHttpError {
	readonly wwwAuthenticate: string | null;

	constructor(response: Response, body = "") {
		super(401, "MCP server requires authentication", body);
		this.name = "McpAuthRequiredError";
		this.wwwAuthenticate = response.headers.get("www-authenticate");
	}
}

export class McpSessionExpiredError extends McpHttpError {
	constructor(body = "") {
		super(404, "MCP session expired", body);
		this.name = "McpSessionExpiredError";
	}
}

export function discard(response: Response): Promise<void> {
	return response.body?.cancel().catch(() => {}) ?? Promise.resolve();
}

/** A credentialed MCP request must never follow a server-selected destination. */
export async function rejectRedirect(response: Response, url: URL): Promise<void> {
	if (!response.redirected && (!response.url || new URL(response.url).origin === url.origin)) return;
	await discard(response);
	throw new Error("MCP HTTP response redirected outside its configured endpoint");
}

/**
 * The body as text, reading at most `maxBytes`: a longer body is cancelled, then refused, or cut at the limit with
 * `truncate`. A plain `response.text()` would buffer whatever the server sends.
 */
export async function readText(response: Response, maxBytes: number, truncate = false): Promise<string> {
	if (!response.body) return "";
	const reader = response.body.getReader();
	const chunks: Uint8Array[] = [];
	let size = 0;
	try {
		while (true) {
			const { done, value } = await reader.read();
			if (done) break;
			if (size + value.byteLength > maxBytes) {
				if (!truncate) throw new Error(`MCP HTTP response exceeds ${maxBytes} bytes`);
				chunks.push(value.subarray(0, maxBytes - size));
				break;
			}
			chunks.push(value);
			size += value.byteLength;
		}
	} finally {
		reader.cancel().catch(() => {});
	}
	return Buffer.concat(chunks).toString("utf8");
}

export function describeHttpFailure(status: number, body: string): string {
	const text = body.trim();
	const snippet = text.length > ERROR_MESSAGE_BODY_CHARS ? `${text.slice(0, ERROR_MESSAGE_BODY_CHARS - 3)}...` : text;
	return `MCP HTTP request failed with status ${status}${snippet ? `: ${snippet}` : ""}`;
}
