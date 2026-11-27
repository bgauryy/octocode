Load when choosing an I/O model, reviewing the cost or durability of a file or socket path, or finding blocking calls in async code.

# I/O: models, cost and async hazards

## I/O models

| Model | Completion signal | Fits | Watch |
|---|---|---|---|
| Blocking | The call returns when done; the thread waits | Simple code, modest concurrency | One thread per in-flight operation |
| Non-blocking + readiness (`epoll`, `kqueue`) | Kernel reports "ready", the program retries the syscall | Sockets, pipes | Not for regular files: `select`/`poll` always report them ready, `epoll_ctl` rejects them (`EPERM`), so file I/O still blocks the thread |
| Completion (`io_uring`, IOCP) | Kernel reports "done" | Files and sockets, batched submissions | Buffers must stay valid until completion; cancellation is asynchronous too |
| Thread-pool offload | A worker runs the blocking call | File I/O in libuv, `tokio::fs`, `asyncio.to_thread` | The pool size caps concurrency: libuv's default 4 threads are shared by `fs`, `dns.lookup`, `crypto` and `zlib` (`UV_THREADPOOL_SIZE`) |
| Memory-mapped (`mmap`) | Page faults load data on touch | Random reads of large read-mostly files | Faults block the thread invisibly; I/O errors arrive as `SIGBUS`, not return codes |

## I/O cost and correctness

| Concern | Rule | Watch |
|---|---|---|
| Syscall overhead | Buffer small writes; batch with `writev`/`readv`, `BufWriter`, `io_uring` batches | Flush at message and exit boundaries; an unflushed buffer is lost on crash |
| Partial reads and writes | Loop until the full count or EOF; a socket or pipe read returns what is available | Frame messages (length prefix or delimiter); one read is not one message |
| Durability | `write` reaches the page cache; `fsync`/`fdatasync` makes it durable; after creating or renaming a file, `fsync` its directory too | After an `fsync` error on Linux, dirty pages can be dropped and a retried `fsync` can succeed: treat the first error as data loss ([PostgreSQL fsync errors](https://wiki.postgresql.org/wiki/Fsync_Errors)) |
| Atomic replace | Write a temp file in the same directory, `fsync`, `rename` over the target, `fsync` the directory | `rename` across filesystems fails (`EXDEV`): write the temporary file on the target filesystem; a cross-filesystem move is never atomic |
| Sequential vs random | Prefer sequential, large blocks; sort or batch random reads | Read-ahead helps only sequential access |
| Page cache | Repeated reads hit memory | A benchmark after the first run measures the cache: state warm or cold |
| Direct I/O (`O_DIRECT`) | Bypass the page cache when the application caches itself | Buffer, offset and length alignment rules |
| Chatty access | One query or request per item (N+1) | Batch endpoints, `IN` lists, dataloader-style coalescing |
| Descriptor and pool limits | Budget file descriptors (`ulimit -n`), pool sizes and ephemeral ports | `EMFILE` and port exhaustion appear only under fan-out |
| Network and overlay filesystems | NFS, SMB and container overlays change `fsync`, lock, rename and mtime semantics | Test durability and locking on the real mount |

## Async vs sync hazards

| Hazard | Symptom | Fix |
|---|---|---|
| Blocking call in async code (sync `fs`, `requests` in asyncio, `std::fs` or `std::thread::sleep` in Tokio, a CPU loop) | Event-loop delay; unrelated requests on that loop or worker share the p99 spike | Use the async API, or offload to a bounded pool (`spawn_blocking`, `asyncio.to_thread`, worker threads) |
| Sync over async (`.Result`, `.Wait()`, `futures::executor::block_on` on a runtime thread; Tokio's `block_on` there panics instead) | Deadlock under a single-threaded context; thread-pool starvation | Async all the way up, or one explicit runtime boundary on a dedicated thread |
| Async over sync (an async signature wrapping a blocking call) | Looks concurrent, still holds a thread per call | Document it; bound the pool behind it |
| Lock held across `await` | Tasks queue behind a suspended holder; a `std::sync::Mutex` guard across `.await` makes a Tokio future non-`Send` | Copy out and release before awaiting; use an async mutex only when the hold must span an await |
| Sequential awaits of independent work | Latency is the sum instead of the maximum | Start them together and await as a group, with a concurrency bound |
| Unbounded fan-out (`Promise.all` over every item, a goroutine per item, `asyncio.gather` over a huge list) | Socket and descriptor exhaustion, 429s, memory spikes | Semaphore, `p-limit`, `errgroup.SetLimit`, `buffer_unordered(n)`, `asyncio.Semaphore` |
| Fire-and-forget without an owner | Unhandled rejections, lost errors, work after shutdown | Structured concurrency: task groups, nurseries, `asyncio.TaskGroup`; track and await every task |
| Missing cancellation | Abandoned requests keep consuming downstream | Pass `AbortSignal`, `CancellationToken` or Go's request context through every await and blocking point |
| Virtual-thread pinning | Carrier threads blocked by waiting virtual threads | JDK 24+ no longer pins on `synchronized` ([JEP 491](https://openjdk.org/jeps/491)); native frames still pin |

Runtime notes: Go code looks synchronous because the runtime parks goroutines on its network poller, while blocking file syscalls still occupy an OS thread. CPython's GIL limits CPU parallelism in threads (see [concurrency models](concurrency-models-primitives.md#concurrency-models)); asyncio debug mode (`PYTHONASYNCIODEBUG=1`) logs callbacks slower than `loop.slow_callback_duration`. Synchronous versus asynchronous communication between services is a transport choice: see [communication](concepts-communication.md#communication).
