package nxuskit

import (
	"sync"
	"testing"
	"time"
)

func TestCapabilityCache_GetMissing(t *testing.T) {
	cache := NewCapabilityCache(time.Hour)

	result := cache.Get("nonexistent")
	if result != nil {
		t.Errorf("expected nil for missing entry, got %v", result)
	}
}

func TestCapabilityCache_InsertAndGet(t *testing.T) {
	cache := NewCapabilityCache(time.Hour)

	caps := ModelCapabilities{
		VisionMode:        VisionModeMultiImage,
		SupportsStreaming: true,
	}

	cache.Insert("llava:latest", caps)

	result := cache.Get("llava:latest")
	if result == nil {
		t.Fatal("expected cached value, got nil")
		return
	}
	if result.VisionMode != VisionModeMultiImage {
		t.Errorf("expected VisionModeMultiImage, got %v", result.VisionMode)
	}
	if !result.SupportsStreaming {
		t.Error("expected SupportsStreaming to be true")
	}
}

func TestCapabilityCache_GetExpired(t *testing.T) {
	// Use a very short TTL
	cache := NewCapabilityCache(10 * time.Millisecond)

	caps := ModelCapabilities{VisionMode: VisionModeSingleImage}
	cache.Insert("model", caps)

	// Wait for expiration
	time.Sleep(50 * time.Millisecond)

	result := cache.Get("model")
	if result != nil {
		t.Errorf("expected nil for expired entry, got %v", result)
	}
}

func TestCapabilityCache_Cleanup(t *testing.T) {
	cache := NewCapabilityCache(10 * time.Millisecond)

	// Add multiple entries
	cache.Insert("model1", ModelCapabilities{})
	cache.Insert("model2", ModelCapabilities{})
	cache.Insert("model3", ModelCapabilities{})

	// Wait for expiration
	time.Sleep(50 * time.Millisecond)

	removed := cache.Cleanup()
	if removed != 3 {
		t.Errorf("expected 3 entries removed, got %d", removed)
	}

	if cache.Len() != 0 {
		t.Errorf("expected 0 entries after cleanup, got %d", cache.Len())
	}
}

func TestCapabilityCache_Clear(t *testing.T) {
	cache := NewCapabilityCache(time.Hour)

	cache.Insert("model1", ModelCapabilities{})
	cache.Insert("model2", ModelCapabilities{})

	cache.Clear()

	if !cache.IsEmpty() {
		t.Error("expected cache to be empty after Clear()")
	}
}

func TestCapabilityCache_Stats(t *testing.T) {
	cache := NewCapabilityCache(time.Hour)

	cache.Insert("model1", ModelCapabilities{})
	cache.Insert("model2", ModelCapabilities{})

	stats := cache.Stats()
	if stats.TotalEntries != 2 {
		t.Errorf("expected 2 total entries, got %d", stats.TotalEntries)
	}
	if stats.ExpiredEntries != 0 {
		t.Errorf("expected 0 expired entries, got %d", stats.ExpiredEntries)
	}
	if stats.TTL != time.Hour {
		t.Errorf("expected TTL of 1 hour, got %v", stats.TTL)
	}
}

func TestCapabilityCache_ThreadSafety(t *testing.T) {
	cache := NewCapabilityCache(time.Hour)

	var wg sync.WaitGroup
	const numGoroutines = 100

	// Concurrent writes
	for i := 0; i < numGoroutines; i++ {
		wg.Add(1)
		go func(n int) {
			defer wg.Done()
			cache.Insert("model", ModelCapabilities{VisionMode: VisionMode(n % 3)})
		}(i)
	}

	// Concurrent reads
	for i := 0; i < numGoroutines; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_ = cache.Get("model")
		}()
	}

	// Concurrent stats
	for i := 0; i < numGoroutines; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_ = cache.Stats()
		}()
	}

	wg.Wait()

	// Verify cache is still functional
	result := cache.Get("model")
	if result == nil {
		t.Fatal("expected cached value after concurrent access")
	}
}

func TestCapabilityCache_LenAndIsEmpty(t *testing.T) {
	cache := NewCapabilityCache(time.Hour)

	if !cache.IsEmpty() {
		t.Error("expected new cache to be empty")
	}

	if cache.Len() != 0 {
		t.Errorf("expected Len() == 0 for new cache, got %d", cache.Len())
	}

	cache.Insert("model", ModelCapabilities{})

	if cache.IsEmpty() {
		t.Error("expected cache to not be empty after insert")
	}

	if cache.Len() != 1 {
		t.Errorf("expected Len() == 1 after insert, got %d", cache.Len())
	}
}
