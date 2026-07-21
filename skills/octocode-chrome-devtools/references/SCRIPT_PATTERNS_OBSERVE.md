# CDP Observation Patterns

Load for passive evidence collection. Why: listeners must be attached before navigation/action.

## Network Console
Enable Network, Runtime, and Log first. Capture failed requests, non-2xx/3xx statuses of interest, console errors, exceptions, and source locations.

## Performance Audit
Start before navigation; collect navigation/resource timing, long tasks, metrics, and bounded trace data only when needed.

## Core Web Vitals
Inject observers before navigation when possible. Feature-detect PerformanceObserver and emit missing-support as uncertainty.

## DOM Accessibility
Use Accessibility tree plus DOM role/name/label checks. Report selectors and impact; avoid generic dumps.

## Heap Memory
Use bounded heap snapshots/samples. Compare before/after action only when a leak hypothesis exists.

## Security Audit
Inspect headers, CSP, mixed content, TLS, cookie flags by name, storage risks, and third-party script origins without secret values.

Next: recovery table in `references/RECOVERY.md` when signals conflict or disappear.
