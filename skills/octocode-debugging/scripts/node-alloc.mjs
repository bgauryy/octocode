// Preload (`node --import node-alloc.mjs app.mjs`): samples where the main thread allocates, including objects the GC has
// already collected, and writes a `.heapprofile` that `profile-top.mjs` reads. `--heap-prof` keeps only what is still live
// when the profile is written, so it hides churn: short-lived buffers, promises and strings that drive GC work.
//   NODE_ALLOC_DIR       existing output folder (required); the profile is `alloc-<pid>.heapprofile`
//   NODE_ALLOC_INTERVAL  mean bytes between samples (default 32768)
//   NODE_ALLOC_GATE=1    start when `<dir>/start` exists and write when `<dir>/stop` exists (checked every 250 ms), so
//                        the profile covers one phase; without it, sampling runs from load to exit
import { existsSync, statSync, writeFileSync } from "node:fs";
import { Session } from "node:inspector";
import path from "node:path";
import { isMainThread } from "node:worker_threads";

const dir = process.env.NODE_ALLOC_DIR;
if (isMainThread && dir) {
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
