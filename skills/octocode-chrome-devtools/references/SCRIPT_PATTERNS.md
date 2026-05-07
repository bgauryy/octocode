# Script Pattern Router

Use this file to choose the smallest pattern detail file. Do not load every code-heavy pattern by default.

## Loading Rule

1. Pick the pattern that matches the missing helper or script shape.
2. Load only the listed `SCRIPT_PATTERNS_*.md` file and section.
3. Treat code as an adaptable example, not a fixed framework.
4. Verify current CDP params in `CDP_AGENT_REFERENCE.md`; feature-detect browser/Web APIs inside the page.

## Detail Files

| File | Patterns | Load when |
|---|---|---|
| `SCRIPT_PATTERNS_OBSERVE.md` | `Network Console (most common)`, `Performance Audit`, `Core Web Vitals (inject before navigate)`, `DOM Accessibility Audit`, `Heap Memory Audit (leak detection)`, `Security Audit` | Load for console/network capture, performance, DOM, memory, and security observation patterns. |
| `SCRIPT_PATTERNS_BROWSER.md` | `WebSocket Surveillance`, `Search Text Across All Resources`, `File Upload`, `Save Files Screenshots PDFs and Metadata`, `Shadow DOM Querying Inside Shadow Roots`, `Source Map Resolution` | Load for WebSocket frames, page resource search, file upload, artifact saving, shadow DOM, and source-map resolution patterns. |
| `SCRIPT_PATTERNS_ASYNC_WORKERS.md` | `waitForNetworkIdle`, `waitForSelector with Actionability`, `Service Worker Lifecycle`, `WebSocket inside Workers` | Load for wait helpers, Service Worker lifecycle tracking, and WebSocket traffic inside worker sessions. |
| `SCRIPT_PATTERNS_SPECIAL.md` | `Storage Audit`, `Consent Audit`, `Full Audit (combine all)` | Load for storage, consent, and full-audit pointers that delegate to intent detail files. |

## Pattern Index

| Need | Details |
|---|---|
| Network Console (most common) | [SCRIPT_PATTERNS_OBSERVE.md#network-console-most-common](SCRIPT_PATTERNS_OBSERVE.md#network-console-most-common) |
| Performance Audit | [SCRIPT_PATTERNS_OBSERVE.md#performance-audit](SCRIPT_PATTERNS_OBSERVE.md#performance-audit) |
| Core Web Vitals (inject before navigate) | [SCRIPT_PATTERNS_OBSERVE.md#core-web-vitals-inject-before-navigate](SCRIPT_PATTERNS_OBSERVE.md#core-web-vitals-inject-before-navigate) |
| DOM Accessibility Audit | [SCRIPT_PATTERNS_OBSERVE.md#dom-accessibility-audit](SCRIPT_PATTERNS_OBSERVE.md#dom-accessibility-audit) |
| Heap Memory Audit (leak detection) | [SCRIPT_PATTERNS_OBSERVE.md#heap-memory-audit-leak-detection](SCRIPT_PATTERNS_OBSERVE.md#heap-memory-audit-leak-detection) |
| Security Audit | [SCRIPT_PATTERNS_OBSERVE.md#security-audit](SCRIPT_PATTERNS_OBSERVE.md#security-audit) |
| WebSocket Surveillance | [SCRIPT_PATTERNS_BROWSER.md#websocket-surveillance](SCRIPT_PATTERNS_BROWSER.md#websocket-surveillance) |
| Search Text Across All Resources | [SCRIPT_PATTERNS_BROWSER.md#search-text-across-all-resources](SCRIPT_PATTERNS_BROWSER.md#search-text-across-all-resources) |
| File Upload | [SCRIPT_PATTERNS_BROWSER.md#file-upload](SCRIPT_PATTERNS_BROWSER.md#file-upload) |
| waitForNetworkIdle | [SCRIPT_PATTERNS_ASYNC_WORKERS.md#waitfornetworkidle](SCRIPT_PATTERNS_ASYNC_WORKERS.md#waitfornetworkidle) |
| waitForSelector with Actionability | [SCRIPT_PATTERNS_ASYNC_WORKERS.md#waitforselector-with-actionability](SCRIPT_PATTERNS_ASYNC_WORKERS.md#waitforselector-with-actionability) |
| Save Files Screenshots PDFs and Metadata | [SCRIPT_PATTERNS_BROWSER.md#save-files-screenshots-pdfs-and-metadata](SCRIPT_PATTERNS_BROWSER.md#save-files-screenshots-pdfs-and-metadata) |
| Shadow DOM Querying Inside Shadow Roots | [SCRIPT_PATTERNS_BROWSER.md#shadow-dom-querying-inside-shadow-roots](SCRIPT_PATTERNS_BROWSER.md#shadow-dom-querying-inside-shadow-roots) |
| Source Map Resolution | [SCRIPT_PATTERNS_BROWSER.md#source-map-resolution](SCRIPT_PATTERNS_BROWSER.md#source-map-resolution) |
| Service Worker Lifecycle | [SCRIPT_PATTERNS_ASYNC_WORKERS.md#service-worker-lifecycle](SCRIPT_PATTERNS_ASYNC_WORKERS.md#service-worker-lifecycle) |
| WebSocket inside Workers | [SCRIPT_PATTERNS_ASYNC_WORKERS.md#websocket-inside-workers](SCRIPT_PATTERNS_ASYNC_WORKERS.md#websocket-inside-workers) |
| Storage Audit | [SCRIPT_PATTERNS_SPECIAL.md#storage-audit](SCRIPT_PATTERNS_SPECIAL.md#storage-audit) |
| Consent Audit | [SCRIPT_PATTERNS_SPECIAL.md#consent-audit](SCRIPT_PATTERNS_SPECIAL.md#consent-audit) |
| Full Audit (combine all) | [SCRIPT_PATTERNS_SPECIAL.md#full-audit-combine-all](SCRIPT_PATTERNS_SPECIAL.md#full-audit-combine-all) |
