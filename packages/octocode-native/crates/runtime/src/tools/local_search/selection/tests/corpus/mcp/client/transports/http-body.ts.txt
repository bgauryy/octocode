/** Host limits and cancellation around an SDK-owned response parser. */
export function boundedBody(
	body: ReadableStream<Uint8Array>,
	limit: number,
	events: boolean,
	signal?: AbortSignal | null,
): ReadableStream<Uint8Array> {
	const reader = body.getReader();
	let bytes = 0;
	let previous = -1;
	let lineBytes = 0;
	let finished = false;
	let abort = () => {};
	const finish = () => {
		finished = true;
		signal?.removeEventListener("abort", abort);
	};
	return new ReadableStream<Uint8Array>({
		start(controller) {
			abort = () => {
				if (finished) return;
				finish();
				void reader.cancel(signal?.reason).catch(() => {});
				controller.error(signal?.reason ?? new Error("MCP response aborted"));
			};
			if (signal?.aborted) abort();
			else signal?.addEventListener("abort", abort, { once: true });
		},
		async pull(controller) {
			try {
				const { done, value } = await reader.read();
				if (finished) return;
				if (done) {
					finish();
					controller.close();
					return;
				}
				const output = events ? new Uint8Array(value.length) : value;
				let length = 0;
				for (const byte of value) {
					bytes++;
					if (events) {
						if (byte === 10 && previous === 13) {
							previous = byte;
							continue;
						}
						if (byte === 10 || byte === 13) {
							if (lineBytes === 0) bytes = 0;
							lineBytes = 0;
						} else lineBytes++;
						previous = byte;
					}
					if (bytes > limit) throw new Error(`MCP HTTP response exceeds ${limit} bytes`);
					// Normalize line endings for SDK parser versions that defer a trailing CR at EOF.
					if (events) output[length++] = byte === 13 ? 10 : byte;
				}
				controller.enqueue(events ? output.subarray(0, length) : output);
			} catch (error) {
				if (finished) return;
				finish();
				void reader.cancel(error).catch(() => {});
				controller.error(error);
			}
		},
		cancel(reason) {
			finish();
			return reader.cancel(reason);
		},
	});
}
