//! Cache tracking and metrics for monitoring cache efficiency

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Metrics for cache performance
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheMetrics {
    /// Total cache hits
    pub cache_hits: u64,
    /// Total cache misses
    pub cache_misses: u64,
    /// Cache writes (new content cached)
    pub cache_writes: u64,
    /// Total tokens served from cache
    pub cached_tokens: u64,
    /// Total tokens that had to be re-sent
    pub uncached_tokens: u64,
    /// Estimated cost savings from caching (in tokens * 0.9)
    pub estimated_savings: u64,
    /// Cache hit rate (0.0 - 1.0)
    pub hit_rate: f64,
}

impl CacheMetrics {
    pub fn record_hit(&mut self, tokens: usize) {
        self.cache_hits += 1;
        self.cached_tokens += tokens as u64;
        self.estimated_savings += (tokens as f64 * 0.9) as u64;
        self.update_hit_rate();
    }

    pub fn record_miss(&mut self, tokens: usize) {
        self.cache_misses += 1;
        self.uncached_tokens += tokens as u64;
        self.update_hit_rate();
    }

    pub fn record_write(&mut self, tokens: usize) {
        self.cache_writes += 1;
        self.uncached_tokens += tokens as u64;
    }

    fn update_hit_rate(&mut self) {
        let total = self.cache_hits + self.cache_misses;
        self.hit_rate = if total > 0 {
            self.cache_hits as f64 / total as f64
        } else {
            0.0
        };
    }

    /// Merge another metrics instance into this one
    pub fn merge(&mut self, other: &CacheMetrics) {
        self.cache_hits += other.cache_hits;
        self.cache_misses += other.cache_misses;
        self.cache_writes += other.cache_writes;
        self.cached_tokens += other.cached_tokens;
        self.uncached_tokens += other.uncached_tokens;
        self.estimated_savings += other.estimated_savings;
        self.update_hit_rate();
    }
}

impl std::fmt::Display for CacheMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Cache Metrics ===")?;
        writeln!(f, "Cache hits: {}", self.cache_hits)?;
        writeln!(f, "Cache misses: {}", self.cache_misses)?;
        writeln!(f, "Hit rate: {:.1}%", self.hit_rate * 100.0)?;
        writeln!(f, "Tokens from cache: {}", self.cached_tokens)?;
        writeln!(f, "Tokens re-sent: {}", self.uncached_tokens)?;
        writeln!(f, "Est. token savings: {}", self.estimated_savings)?;
        Ok(())
    }
}

/// Entry in the cache tracker
#[derive(Debug, Clone)]
struct CacheEntry {
    /// Hash of the content
    content_hash: u64,
    /// Token count of the content
    token_count: usize,
    /// When this entry was created
    created_at: Instant,
    /// When this entry was last accessed
    last_accessed: Instant,
    /// Number of times this entry was hit
    hit_count: u64,
    /// Stability classification
    stability: ContentStabilityLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum ContentStabilityLevel {
    /// Content that should be cached indefinitely
    Permanent,
    /// Content cached for the session
    Session,
    /// Content with time-based expiry
    Temporal(Duration),
}

/// Tracks cache state across requests
pub struct CacheTracker {
    /// Cache entries by key
    entries: Arc<Mutex<HashMap<String, CacheEntry>>>,
    /// Aggregated metrics
    metrics: Arc<Mutex<CacheMetrics>>,
    /// Maximum entries to keep
    max_entries: usize,
    /// Session start time
    session_start: Instant,
}

impl CacheTracker {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            metrics: Arc::new(Mutex::new(CacheMetrics::default())),
            max_entries,
            session_start: Instant::now(),
        }
    }

    /// Register content as cached
    pub fn cache_content(&self, key: &str, content_hash: u64, token_count: usize, permanent: bool) {
        let mut entries = self.entries.lock().unwrap();

        // Evict old entries if at capacity
        if entries.len() >= self.max_entries {
            self.evict_lru(&mut entries);
        }

        let now = Instant::now();
        entries.insert(
            key.to_string(),
            CacheEntry {
                content_hash,
                token_count,
                created_at: now,
                last_accessed: now,
                hit_count: 0,
                stability: if permanent {
                    ContentStabilityLevel::Permanent
                } else {
                    ContentStabilityLevel::Session
                },
            },
        );

        // Record the write
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.record_write(token_count);
        }
    }

    /// Check if content is cached and matches
    pub fn check(&self, key: &str, content_hash: u64) -> CacheStatus {
        let mut entries = self.entries.lock().unwrap();

        if let Some(entry) = entries.get_mut(key) {
            if entry.content_hash == content_hash {
                // Update access time and hit count
                entry.last_accessed = Instant::now();
                entry.hit_count += 1;

                let token_count = entry.token_count;

                // Record the hit
                if let Ok(mut metrics) = self.metrics.lock() {
                    metrics.record_hit(token_count);
                }

                CacheStatus::Hit {
                    tokens: token_count,
                    age: entry.created_at.elapsed(),
                }
            } else {
                // Content changed
                let old_tokens = entry.token_count;

                // Record as miss
                if let Ok(mut metrics) = self.metrics.lock() {
                    metrics.record_miss(old_tokens);
                }

                CacheStatus::Stale
            }
        } else {
            CacheStatus::Miss
        }
    }

    /// Invalidate a cache entry
    pub fn invalidate(&self, key: &str) {
        let mut entries = self.entries.lock().unwrap();
        entries.remove(key);
    }

    /// Clear all cache entries
    pub fn clear(&self) {
        let mut entries = self.entries.lock().unwrap();
        entries.clear();
    }

    /// Get current metrics
    pub fn get_metrics(&self) -> CacheMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Reset metrics
    pub fn reset_metrics(&self) {
        let mut metrics = self.metrics.lock().unwrap();
        *metrics = CacheMetrics::default();
    }

    /// Get cache entry count
    pub fn entry_count(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    /// Get total cached tokens
    pub fn total_cached_tokens(&self) -> usize {
        self.entries
            .lock()
            .unwrap()
            .values()
            .map(|e| e.token_count)
            .sum()
    }

    /// Evict least recently used entries
    fn evict_lru(&self, entries: &mut HashMap<String, CacheEntry>) {
        // Find entries to evict (oldest 25%)
        let evict_count = self.max_entries / 4;

        let mut by_access: Vec<_> = entries
            .iter()
            .filter(|(_, e)| e.stability != ContentStabilityLevel::Permanent)
            .map(|(k, e)| (k.clone(), e.last_accessed))
            .collect();

        by_access.sort_by_key(|(_, t)| *t);

        for (key, _) in by_access.into_iter().take(evict_count) {
            entries.remove(&key);
        }
    }

    /// Get a summary of cache state
    pub fn summary(&self) -> CacheSummary {
        let entries = self.entries.lock().unwrap();
        let metrics = self.metrics.lock().unwrap();

        let mut permanent_tokens = 0;
        let mut session_tokens = 0;

        for entry in entries.values() {
            match entry.stability {
                ContentStabilityLevel::Permanent => permanent_tokens += entry.token_count,
                _ => session_tokens += entry.token_count,
            }
        }

        CacheSummary {
            entry_count: entries.len(),
            permanent_tokens,
            session_tokens,
            total_hits: metrics.cache_hits,
            total_misses: metrics.cache_misses,
            hit_rate: metrics.hit_rate,
            estimated_savings: metrics.estimated_savings,
            session_duration: self.session_start.elapsed(),
        }
    }
}

impl Default for CacheTracker {
    fn default() -> Self {
        Self::new(1000) // Default to 1000 entries
    }
}

/// Status of a cache check
#[derive(Debug)]
pub enum CacheStatus {
    /// Content is cached and matches
    Hit { tokens: usize, age: Duration },
    /// Content was cached but has changed
    Stale,
    /// Content is not in cache
    Miss,
}

/// Summary of cache state
#[derive(Debug, Serialize)]
pub struct CacheSummary {
    pub entry_count: usize,
    pub permanent_tokens: usize,
    pub session_tokens: usize,
    pub total_hits: u64,
    pub total_misses: u64,
    pub hit_rate: f64,
    pub estimated_savings: u64,
    #[serde(skip)]
    pub session_duration: Duration,
}

impl std::fmt::Display for CacheSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Cache Summary ===")?;
        writeln!(f, "Cached entries: {}", self.entry_count)?;
        writeln!(f, "Permanent tokens: {}", self.permanent_tokens)?;
        writeln!(f, "Session tokens: {}", self.session_tokens)?;
        writeln!(f, "Total hits: {}", self.total_hits)?;
        writeln!(f, "Total misses: {}", self.total_misses)?;
        writeln!(f, "Hit rate: {:.1}%", self.hit_rate * 100.0)?;
        writeln!(f, "Est. token savings: {}", self.estimated_savings)?;
        writeln!(f, "Session duration: {:?}", self.session_duration)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(s: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn test_cache_hit() {
        let tracker = CacheTracker::new(100);
        let content = "test content";
        let content_hash = hash(content);

        tracker.cache_content("key1", content_hash, 100, false);

        match tracker.check("key1", content_hash) {
            CacheStatus::Hit { tokens, .. } => assert_eq!(tokens, 100),
            _ => panic!("Expected cache hit"),
        }
    }

    #[test]
    fn test_cache_miss() {
        let tracker = CacheTracker::new(100);

        match tracker.check("nonexistent", 12345) {
            CacheStatus::Miss => {}
            _ => panic!("Expected cache miss"),
        }
    }

    #[test]
    fn test_cache_stale() {
        let tracker = CacheTracker::new(100);

        tracker.cache_content("key1", 11111, 100, false);

        match tracker.check("key1", 22222) {
            CacheStatus::Stale => {}
            _ => panic!("Expected stale status"),
        }
    }

    #[test]
    fn test_metrics_hit_rate() {
        let mut metrics = CacheMetrics::default();

        metrics.record_hit(100);
        metrics.record_hit(100);
        metrics.record_miss(100);

        assert!((metrics.hit_rate - 0.666).abs() < 0.01);
    }
}
