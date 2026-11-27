// Runtime metrics preload: NODE_METRICS_DIR="$out" node --import node-metrics.mjs app.mjs
// Samples the main thread of the launched program without opening an inspector or keeping the event loop alive, and
// leaves the program's stdout untouched. metrics-report.mjs turns the capture into intervals (--json) or HTML.
// Inputs (environment):
//   NODE_METRICS_DIR          capture folder (required; created if missing)
//   NODE_METRICS_INTERVAL_MS  milliseconds between samples, 10 to 2147483647 (default 1000)
// Output: <dir>/<pid>-<epoch ms>.metrics.jsonl, mode 0600, one JSON record per line: "start" (pid, ppid, node,
//   platform, interval), then "sample" every interval and "exit" (with exitCode) at exit. Each measurement record
//   holds cumulative elapsedMs, cpuMs {user, system}, memoryBytes (process.memoryUsage()), resources
//   (process.resourceUsage(): maxRSS KiB, fsRead, fsWrite ...), eventLoop (utilization since start; delayMs p50,
//   p99, max since start) and gc {count, durationMs}. A run killed by a signal has no exit record.
// Effects: an unref'd timer, a GC PerformanceObserver and an event-loop delay histogram in the main thread.
// Exit: an unset NODE_METRICS_DIR or a bad interval throws at load, so the program exits 1 unstarted.
// Run directly, node node-metrics.mjs --help prints this text and exits 0; any other direct run prints it and exits 2.
import { closeSync, mkdirSync, openSync, readFileSync, writeSync } from "node:fs";
import { join, resolve } from "node:path";
import { monitorEventLoopDelay, performance, PerformanceObserver } from "node:perf_hooks";
import { fileURLToPath } from "node:url";
import { isMainThread } from "node:worker_threads";
import { isMainModule } from "./lib/entrypoint.mjs";

if (isMainModule(import.meta.url)) {
	// Run as a program rather than preloaded: print the header above as usage.
	const usage = readFileSync(fileURLToPath(import.meta.url), "utf8").match(/^(?:\/\/.*\n)+/)[0].replace(/^\/\/ ?/gm, "");
	const help = process.argv.slice(2).includes("--help");
	(help ? process.stdout : process.stderr).write(usage);
	process.exitCode = help ? 0 : 2;
} else if (isMainThread) start();

function start() {
	const directory = process.env.NODE_METRICS_DIR;
	if (!directory) throw new Error("node-metrics: set NODE_METRICS_DIR to the capture directory");
	const interval = Number(process.env.NODE_METRICS_INTERVAL_MS ?? 1000);
	if (!Number.isSafeInteger(interval) || interval < 10 || interval > 2147483647) {
		throw new Error("node-metrics: NODE_METRICS_INTERVAL_MS must be an integer from 10 to 2147483647");
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
