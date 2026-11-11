// Preload with --import; observe the main thread without opening an inspector or retaining the event loop.
import { closeSync, mkdirSync, openSync, writeSync } from "node:fs";
import { join, resolve } from "node:path";
import { monitorEventLoopDelay, performance, PerformanceObserver } from "node:perf_hooks";
import { isMainThread } from "node:worker_threads";

if (isMainThread) start();

function start() {
	const directory = process.env.OCTOCODE_PROFILE_DIR;
	if (!directory) throw new Error("node-metrics: set OCTOCODE_PROFILE_DIR to the capture directory");
	const interval = Number(process.env.OCTOCODE_PROFILE_INTERVAL_MS ?? 1000);
	if (!Number.isSafeInteger(interval) || interval < 10 || interval > 2147483647) {
		throw new Error("node-metrics: OCTOCODE_PROFILE_INTERVAL_MS must be an integer from 10 to 2147483647");
	}
	mkdirSync(resolve(directory), { recursive: true });
	const file = openSync(join(resolve(directory), `${process.pid}-${Date.now()}.metrics.jsonl`), "wx", 0o600);
	const started = performance.now();
	const initialCpu = process.cpuUsage();
	const initialLoop = performance.eventLoopUtilization();
	const delay = monitorEventLoopDelay({ resolution: 20 });
	const gc = { count: 0, durationMs: 0 };
	const collect = (entries) => {
		for (const entry of entries) {
			gc.count++;
			gc.durationMs += entry.duration;
		}
	};
	const observer = new PerformanceObserver((list) => collect(list.getEntries()));
	observer.observe({ entryTypes: ["gc"] });
	delay.enable();
	const write = (row) => writeSync(file, `${JSON.stringify(row)}\n`);
	write({
		event: "start",
		pid: process.pid,
		ppid: process.ppid,
		node: process.version,
		platform: process.platform,
		thread: "main",
		startedAt: new Date().toISOString(),
		intervalMs: interval,
		delayResolutionMs: 20,
	});
	function sample(event, exitCode) {
		collect(observer.takeRecords());
		const cpu = process.cpuUsage(initialCpu);
		write({
			event,
			exitCode,
			elapsedMs: performance.now() - started,
			cpuMs: { user: cpu.user / 1000, system: cpu.system / 1000 },
			memoryBytes: process.memoryUsage(),
			resources: process.resourceUsage(),
			eventLoop: {
				...performance.eventLoopUtilization(initialLoop),
				delayMs:
					delay.count > 0
						? {
								p50: delay.percentile(50) / 1e6,
								p99: delay.percentile(99) / 1e6,
								max: delay.max / 1e6,
							}
						: null,
			},
			gc,
		});
	}
	const timer = setInterval(() => sample("sample"), interval);
	timer.unref();
	process.once("exit", (code) => {
		clearInterval(timer);
		sample("exit", code);
		delay.disable();
		observer.disconnect();
		closeSync(file);
	});
}
