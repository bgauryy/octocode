Load when writing the bidirectional index: the full class.

# Data structures: reverse lookup implementation

## Reverse lookup implementation

The `BiIndex` class that the [reverse lookup pattern](data-structures-reverse-lookup.md#reverse-lookup-pattern) uses and describes.

```ts
class BiIndex<K, V> {
  private forward = new Map<K, V>();
  private reverse = new Map<V, Set<K>>();

  set(key: K, value: V): void {
    this.delete(key);

    this.forward.set(key, value);

    if (!this.reverse.has(value)) {
      this.reverse.set(value, new Set());
    }
    this.reverse.get(value)!.add(key);
  }

  get(key: K): V | undefined {
    return this.forward.get(key);
  }

  getKeys(value: V): ReadonlySet<K> {
    return this.reverse.get(value) ?? new Set<K>();
  }

  delete(key: K): boolean {
    if (!this.forward.has(key)) return false;

    const value = this.forward.get(key)!;
    this.forward.delete(key);

    const keys = this.reverse.get(value)!;
    keys.delete(key);

    if (keys.size === 0) {
      this.reverse.delete(value);
    }
    return true;
  }
}
```
