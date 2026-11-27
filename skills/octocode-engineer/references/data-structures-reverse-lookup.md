Load when a lookup must also run from value back to its keys and you are deciding on a bidirectional index.

# Data structures: reverse lookup

## Reverse lookup pattern

A TypeScript bidirectional index that allows several keys per value and keeps both directions in step. The class is in [reverse lookup implementation](data-structures-reverse-lookup-code.md#reverse-lookup-implementation).

```ts
// Usage
const users = new BiIndex<number, string>();
users.set(1, "admin");
users.set(2, "admin");
users.set(3, "editor");

console.log([...users.getKeys("admin")]); // [ 1, 2 ]
```

- **Cost.** `set`, `get` and `delete` are O(1) average; getting the reverse set is O(1) average plus O(r) to enumerate r keys. Space is O(n) plus the second map.
- **Reassignment.** `set` deletes the old mapping first, so moving key 1 from `"admin"` to `"editor"` removes it from the `"admin"` set, and an emptied set is dropped.
- **Live view.** `getKeys` returns the internal set. `ReadonlySet` blocks mutation only at compile time, and later writes show through. Return `new Set(keys)` when callers need a snapshot.
- **Equality.** `Map` compares keys with SameValueZero: objects match by identity, not by content.
- **Concurrency.** One event loop makes each method atomic in JavaScript; in a threaded language guard both maps with one lock.
