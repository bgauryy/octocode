Load when attaching Chrome DevTools or an inspector client to a Node process, launched or already running.

# Node inspector and Chrome DevTools

## Attach with Chrome DevTools

Verify PID, command name, owner and the selected runtime first. Launch your own local target with `node --inspect=127.0.0.1:9229 app.mjs`; use `--inspect-brk` only when startup must pause until a debugger resumes it. Select an available port per process. In Chrome, open `chrome://inspect`, configure the loopback endpoint if needed, and choose **inspect** for that exact target (or open dedicated DevTools for Node).

Use **Sources** for conditional breakpoints, exception pauses, stacks and locals. Use **Performance** to record one workload window, stop and save; inspect the same selected interval in **Bottom-Up** (self cost) and **Call Tree** (inclusive callers). Use **Memory** for allocation sampling or approved snapshot comparison. The former JS Profiler panel is deprecated. Browser-page DevTools and dedicated Node DevTools are different targets: a page recording does not automatically capture backend Node CPU.

For an existing target with an authorized inspector, discover its WebSocket URL from `http://127.0.0.1:<port>/json/list`; do not publish this endpoint or URL. On supported Unix Node processes, SIGUSR1 can enable the inspector unless disabled. Verify the PID, ownership and configured listen address before enabling it on a live service. Other runtimes can terminate on this signal. If the inspector cannot be activated through the configured private endpoint, use OS sampling or a separate reproduction. Keep the listener on loopback, including when using a private remote tunnel.

A paused target changes timing, holds resources and can block requests. Resume it and stop recordings before disconnecting; detach rather than killing an existing server. Disconnecting DevTools does not necessarily close the inspector listener: use the launcher's documented shutdown/restart or an authorized `inspector.close()` path. Record which workers/child processes were captured; a main-isolate CPU profile cannot explain their CPU by itself.

Preserve source maps when using transformed TypeScript.

Sources: [Node debugging and inspector security](https://nodejs.org/en/learn/getting-started/debugging), [Chrome Node Performance](https://developer.chrome.com/docs/devtools/performance/nodejs), [Node heap snapshots](https://nodejs.org/en/learn/diagnostics/memory/using-heap-snapshot).
