// Allocation sampling preload: NODE_ALLOC_DIR="$out" node --import node-alloc.mjs app.mjs
// Samples where the main thread allocates, including objects the GC has already collected, and writes a .heapprofile
// that profile-top.mjs reads. --heap-prof keeps only what is still live when the profile is written, so it hides churn:
// short-lived buffers, promises and strings that drive GC work.
// Inputs (environment):
//   NODE_ALLOC_DIR       existing output folder; when unset the preload does nothing
//   NODE_ALLOC_INTERVAL  mean bytes between samples (default 32768)
//   NODE_ALLOC_GATE=1    start when <dir>/start exists and write when <dir>/stop exists (checked every 250 ms), so
//                        the profile covers one phase; without it, sampling runs from load to exit
// Output: <dir>/alloc-<pid>.heapprofile for the main thread, written at exit (or when <dir>/stop appears).
// Effects: opens an in-process inspector session; the gate's timer does not keep the program alive.
// Exit: a missing NODE_ALLOC_DIR folder or a bad NODE_ALLOC_INTERVAL throws at load, so the program exits 1 unstarted.
// Run directly, node node-alloc.mjs --help prints this text and exits 0; any other direct run prints it and exits 2.
import { existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { Session } from "node:inspector";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { isMainThread } from "node:worker_threads";
import { isMainModule } from "./lib/entrypoint.mjs";

const dir = process.env.NODE_ALLOC_DIR;
if (isMainModule(import.meta.url)) {
	// Run as a program rather than preloaded: print the header above as usage.
	const usage = readFileSync(fileURLToPath(import.meta.url), "utf8").match(/^(?:\/\/.*\n)+/)[0].replace(/^\/\/ ?/gm, "");
	const help = process.argv.slice(2).includes("--help");
	(help ? process.stdout : process.stderr).write(usage);
	process.exitCode = help ? 0 : 2;
} else if (isMainThread && dir) {
	if (!statSync(dir, { throwIfNoEntry: false })?.isDirectory()) throw new Error(`NODE_ALLOC_DIR is not a folder: ${dir}`);
	const interval = Number(process.env.NODE_ALLOC_INTERVAL ?? 32768);
	if (!Number.isInteger(interval) || interval < 1) throw new Error("NODE_ALLOC_INTERVAL: a positive integer of bytes");
	const file = path.join(dir, `alloc-${process.pid}.heapprofile`);
	const session = new Session();
	session.connect();
	let state = "waiting";
	const start = () => {
		state = "sampling";
		session.post("HeapProfiler.startSampling", {
			samplingInterval: interval,
			includeObjectsCollectedByMajorGC: true,
			includeObjectsCollectedByMinorGC: true,
		});
	};
	// An in-process session answers synchronously, so this also works inside an `exit` handler.
	const stop = () => {
		if (state !== "sampling") return;
		state = "written";
		session.post("HeapProfiler.stopSampling", (error, result) => {
			if (error) throw error;
			writeFileSync(file, JSON.stringify(result.profile));
		});
	};
	if (process.env.NODE_ALLOC_GATE === "1") {
		const gate = setInterval(() => {
			if (state === "waiting" && existsSync(path.join(dir, "start"))) start();
			if (state === "sampling" && existsSync(path.join(dir, "stop"))) {
				stop();
				clearInterval(gate);
			}
		}, 250);
		gate.unref();
	} else start();
	process.on("exit", stop);
}
