//! Capability caching for model capabilities
//!
//! This module provides an in-memory cache for model capabilities to avoid
//! repeated API calls for the same models.

use crate::capability::ModelCapabilities;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

/// A cached entry with timestamp for TTL tracking
#[derive(Debug, Clone)]
struct CacheEntry {
    capabilities: ModelCapabilities,
    timestamp: SystemTime,
}

/// In-memory cache for model capabilities
///
/// # Example
///
/// ```ignore
/// let cache = CapabilityCache::new(Duration::from_secs(3600)); // 1 hour TTL
///
/// // Check cache
/// if let Some(caps) = cache.get("llava:latest") {
///     println!("Got cached: {:?}", caps);
/// } else {
///     // Fetch from provider
///     let caps = provider.get_model_capabilities("llava:latest").await?;
///     cache.insert("llava:latest", caps);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct CapabilityCache {
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    ttl: Duration,
}

impl CapabilityCache {
    /// Create a new capability cache with the given TTL
    ///
    /// # Arguments
    ///
    /// * `ttl` - Time-to-live for cached entries
    ///
    /// # Example
    ///
    /// ```ignore
    /// let cache = CapabilityCache::new(Duration::from_secs(86400)); // 24 hours
    /// ```
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }

    /// Get a capability from the cache if it exists and hasn't expired
    ///
    /// Returns None if the entry doesn't exist or has expired.
    pub fn get(&self, model_name: &str) -> Option<ModelCapabilities> {
        let cache = self.cache.read().ok()?;

        if let Some(entry) = cache.get(model_name) {
            // Check if entry has expired
            match entry.timestamp.elapsed() {
                Ok(elapsed) if elapsed < self.ttl => {
                    return Some(entry.capabilities.clone());
                }
                _ => {
                    // Entry has expired, will be removed on next cleanup
                }
            }
        }

        None
    }

    /// Insert a capability into the cache
    pub fn insert(&self, model_name: impl Into<String>, capabilities: ModelCapabilities) {
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(
                model_name.into(),
                CacheEntry {
                    capabilities,
                    timestamp: SystemTime::now(),
                },
            );
        }
    }

    /// Clear expired entries from the cache
    ///
    /// Returns the number of entries removed.
    pub fn cleanup(&self) -> usize {
        if let Ok(mut cache) = self.cache.write() {
            let initial_len = cache.len();

            cache.retain(|_, entry| {
                match entry.timestamp.elapsed() {
                    Ok(elapsed) => elapsed < self.ttl,
                    Err(_) => true, // Keep if we can't determine age
                }
            });

            initial_len - cache.len()
        } else {
            0
        }
    }

    /// Clear all entries from the cache
    pub fn clear(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }

    /// Get the number of entries currently in the cache
    pub fn len(&self) -> usize {
        self.cache.read().map(|c| c.len()).unwrap_or(0)
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.read().ok();
        let total_entries = cache.as_ref().map(|c| c.len()).unwrap_or(0);

        let expired = cache
            .map(|c| {
                c.values()
                    .filter(|entry| {
                        entry
                            .timestamp
                            .elapsed()
                            .map(|elapsed| elapsed >= self.ttl)
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0);

        CacheStats {
            total_entries,
            expired_entries: expired,
            ttl: self.ttl,
        }
    }
}

/// Statistics about the capability cache
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Total number of entries in the cache
    pub total_entries: usize,
    /// Number of expired entries
    pub expired_entries: usize,
    /// TTL for cache entries
    pub ttl: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::VisionMode;

    #[test]
    fn test_cache_insert_and_get() {
        let cache = CapabilityCache::new(Duration::from_secs(60));
        let caps = ModelCapabilities {
            vision_mode: VisionMode::MultiImage,
            supports_streaming: true,
            supports_function_calling: false,
        };

        cache.insert("test-model", caps.clone());
        assert_eq!(cache.get("test-model"), Some(caps));
    }

    #[test]
    fn test_cache_miss() {
        let cache: CapabilityCache = CapabilityCache::new(Duration::from_secs(60));
        assert_eq!(cache.get("nonexistent"), None);
    }

    #[test]
    fn test_cache_len() {
        let cache = CapabilityCache::new(Duration::from_secs(60));
        let caps = ModelCapabilities {
            vision_mode: VisionMode::SingleImage,
            supports_streaming: true,
            supports_function_calling: false,
        };

        assert_eq!(cache.len(), 0);
        cache.insert("model1", caps.clone());
        assert_eq!(cache.len(), 1);
        cache.insert("model2", caps);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_cache_clear() {
        let cache = CapabilityCache::new(Duration::from_secs(60));
        let caps = ModelCapabilities {
            vision_mode: VisionMode::None,
            supports_streaming: true,
            supports_function_calling: false,
        };

        cache.insert("model1", caps);
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_stats() {
        let cache = CapabilityCache::new(Duration::from_secs(60));
        let caps = ModelCapabilities {
            vision_mode: VisionMode::MultiImage,
            supports_streaming: true,
            supports_function_calling: false,
        };

        cache.insert("model1", caps);
        let stats = cache.stats();

        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.expired_entries, 0);
    }
}
