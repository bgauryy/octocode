Load when you need a worked memory-ordering example: publishing data from one thread to another in C++.

# Concurrency: publishing data in C++

## Example: publishing data in C++

```cpp
#include <atomic>
#include <thread>
#include <cassert>

int data = 0;
std::atomic<bool> ready{false};

void producer() {
    data = 42;
    ready.store(true, std::memory_order_release);
}

void consumer() {
    while (!ready.load(std::memory_order_acquire)) {
        std::this_thread::yield();
    }

    assert(data == 42);
}

int main() {
    std::thread a(producer);
    std::thread b(consumer);

    a.join();
    b.join();
}
```

The release store and the acquire load that reads `true` form a happens-before edge, so the consumer sees `data == 42`. The example assumes `data` is written only before publication. ThreadSanitizer (`clang++ -std=c++17 -fsanitize=thread`) reports no race for this program and reports a data race on `data` when both orderings are changed to `relaxed`.
