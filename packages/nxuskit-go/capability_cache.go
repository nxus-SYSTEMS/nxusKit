package nxuskit

import (
	"sync"
	"time"
)

// cacheEntry is an internal type for cached capabilities with expiration.
type cacheEntry struct {
	capabilities ModelCapabilities
	expiresAt    time.Time
}

// CacheStats provides statistics about the cache state.
type CacheStats struct {
	// TotalEntries is the current number of entries in the cache.
	TotalEntries int

	// ExpiredEntries is the number of entries past their TTL.
	ExpiredEntries int

	// TTL is the configured time-to-live for entries.
	TTL time.Duration
}

// CapabilityCache provides thread-safe caching of model capabilities.
//
// Use this to avoid repeated API calls for capability detection.
// Entries automatically expire after the configured TTL.
//
// Example:
//
//	cache := NewCapabilityCache(time.Hour)
//
//	// Check cache first
//	if caps := cache.Get("llava:latest"); caps != nil {
//	    return *caps, nil
//	}
//
//	// Cache miss - fetch from provider
//	caps, err := ollama.GetModelCapabilities(ctx, "llava:latest")
//	if err != nil {
//	    return ModelCapabilities{}, err
//	}
//
//	// Store in cache
//	cache.Insert("llava:latest", caps)
//	return caps, nil
type CapabilityCache struct {
	mu    sync.RWMutex
	cache map[string]cacheEntry
	ttl   time.Duration
}

// NewCapabilityCache creates a new capability cache with the given TTL.
//
// The TTL determines how long entries remain valid before expiring.
// A typical value is 1 hour (time.Hour) since model capabilities rarely change.
func NewCapabilityCache(ttl time.Duration) *CapabilityCache {
	return &CapabilityCache{
		cache: make(map[string]cacheEntry),
		ttl:   ttl,
	}
}

// Get retrieves capabilities for a model from the cache.
//
// Returns nil if the model is not cached or the entry has expired.
// This is a read operation and is safe for concurrent use.
func (c *CapabilityCache) Get(model string) *ModelCapabilities {
	c.mu.RLock()
	defer c.mu.RUnlock()

	if entry, ok := c.cache[model]; ok {
		if time.Now().Before(entry.expiresAt) {
			caps := entry.capabilities
			return &caps
		}
	}
	return nil
}

// Insert adds or updates capabilities for a model in the cache.
//
// The entry will expire after the cache's configured TTL.
// This is a write operation and blocks concurrent reads briefly.
func (c *CapabilityCache) Insert(model string, caps ModelCapabilities) {
	c.mu.Lock()
	defer c.mu.Unlock()

	c.cache[model] = cacheEntry{
		capabilities: caps,
		expiresAt:    time.Now().Add(c.ttl),
	}
}

// Cleanup removes expired entries from the cache.
//
// Returns the number of entries removed.
// Call this periodically to prevent unbounded memory growth.
func (c *CapabilityCache) Cleanup() int {
	c.mu.Lock()
	defer c.mu.Unlock()

	now := time.Now()
	removed := 0
	for key, entry := range c.cache {
		if now.After(entry.expiresAt) {
			delete(c.cache, key)
			removed++
		}
	}
	return removed
}

// Clear removes all entries from the cache.
//
// Use this to force fresh capability detection on next access.
func (c *CapabilityCache) Clear() {
	c.mu.Lock()
	defer c.mu.Unlock()

	c.cache = make(map[string]cacheEntry)
}

// Stats returns statistics about the cache state.
//
// Use this for monitoring and debugging.
func (c *CapabilityCache) Stats() CacheStats {
	c.mu.RLock()
	defer c.mu.RUnlock()

	now := time.Now()
	expired := 0
	for _, entry := range c.cache {
		if now.After(entry.expiresAt) {
			expired++
		}
	}

	return CacheStats{
		TotalEntries:   len(c.cache),
		ExpiredEntries: expired,
		TTL:            c.ttl,
	}
}

// Len returns the current number of entries in the cache (including expired).
func (c *CapabilityCache) Len() int {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return len(c.cache)
}

// IsEmpty returns true if the cache has no entries.
func (c *CapabilityCache) IsEmpty() bool {
	return c.Len() == 0
}
