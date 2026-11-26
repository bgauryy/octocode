Load when choosing or naming a transport, a delivery guarantee, or an inter-process mechanism on one machine (pipe, socket, shared memory, message queue).

# Concepts: communication and IPC

## Communication

Choose by requirement: direct calls inside one process, IPC between local processes, HTTP or gRPC across services, messaging for asynchronous work or temporal decoupling. Verify a transport by its framing, delivery guarantee, ordering, timeout owner, and failure mapping ([protocol state and delivery](checks.md#protocol-state-and-delivery)).

Delivery guarantees: at-most-once (fire and forget), at-least-once (retry until acknowledged; duplicates possible), effectively exactly-once (at-least-once plus an idempotent, deduplicating or transactional consumer; no transport provides it alone). Ordering holds per partition, key, or connection, not globally, unless the protocol states otherwise.

### Inter-process communication (IPC)

| Mechanism | Description | When to use |
|---|---|---|
| Pipes | Unidirectional byte stream | Parent-child processes, command pipelines |
| Named pipes (FIFO) | Named channel between processes | Local process communication |
| Unix domain sockets | Local sockets through the socket interface | Local services on Unix-like systems |
| TCP sockets | Network-capable bidirectional connections | Local or remote communication |
| UDP sockets | Connectionless datagrams | Low-latency communication, telemetry |
| Shared memory | Several processes map the same memory | High-throughput data exchange |
| Memory-mapped files | File-backed or anonymous mappings | Large datasets, shared buffers |
| Message queues | Discrete messages between processes | Asynchronous process communication |
| Signals | Lightweight operating-system notifications | Process lifecycle and control |
| RPC | Invokes functionality in another process | Service-like process boundaries |
| D-Bus | Desktop and system message bus | Linux desktop and system services |
| COM | Component interaction technology | Windows application integration |
| XPC | Structured process communication | macOS and Apple platform services |
| Binder | Android's primary IPC mechanism | Android system services |
| Electron IPC | Messages between main and renderer processes | Electron desktop applications |
| stdio (JSON-RPC, MCP) | Framed messages over a child process's standard streams | Tool servers, language servers, agent protocols |

Semaphores, mutexes, and condition variables coordinate access; they transport nothing and are defined under [concurrency](concurrency-models-primitives.md#primitives-and-patterns).
