package cache

import "sync"

type Cache struct {
	mu    sync.Mutex
	items map[string]string
}

// Get leaks the lock when the key is missing: the early return skips Unlock.
func (c *Cache) Get(key string) (string, bool) {
	c.mu.Lock()
	value, ok := c.items[key]
	if !ok {
		return "", false
	}
	c.mu.Unlock()
	return value, true
}
