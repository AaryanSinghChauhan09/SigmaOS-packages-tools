//! Package cache management for optimization
//! Follows: Cache Conscious Design, Locality of Reference

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Cache entry with metadata
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub path: PathBuf,
    pub size: u64,
    pub accessed: u64,
    pub expires: Option<u64>,
}

impl CacheEntry {
    pub fn new(path: PathBuf, size: u64, ttl_seconds: Option<u64>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let expires = ttl_seconds.map(|ttl| now + ttl);
        
        CacheEntry {
            path,
            size,
            accessed: now,
            expires,
        }
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now > expires
        } else {
            false
        }
    }

    pub fn touch(&mut self) {
        self.accessed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
}

/// Package cache
pub struct PackageCache {
    entries: HashMap<String, CacheEntry>,
    max_size: u64,
    current_size: u64,
}

impl PackageCache {
    pub fn new(max_size: u64) -> Self {
        PackageCache {
            entries: HashMap::new(),
            max_size,
            current_size: 0,
        }
    }

    pub fn add(&mut self, key: String, entry: CacheEntry) -> Result<(), &'static str> {
        if self.current_size + entry.size > self.max_size {
            self.evict(entry.size)?;
        }
        
        self.current_size += entry.size;
        self.entries.insert(key, entry);
        Ok(())
    }

    pub fn get(&mut self, key: &str) -> Option<&CacheEntry> {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.touch();
            Some(entry)
        } else {
            None
        }
    }

    pub fn remove(&mut self, key: &str) -> bool {
        if let Some(entry) = self.entries.remove(key) {
            self.current_size -= entry.size;
            true
        } else {
            false
        }
    }

    fn evict(&mut self, required: u64) -> Result<(), &'static str> {
        // LRU eviction: remove least recently used entries
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by(|a, b| a.1.accessed.cmp(&b.1.accessed));
        
        let mut freed = 0u64;
        for (key, entry) in entries {
            if freed >= required {
                break;
            }
            self.remove(key);
            freed += entry.size;
        }
        
        if freed < required {
            return Err("Cannot free enough space in cache");
        }
        
        Ok(())
    }

    pub fn cleanup_expired(&mut self) {
        let expired: Vec<String> = self.entries
            .iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(key, _)| key.clone())
            .collect();
        
        for key in expired {
            self.remove(&key);
        }
    }
}

impl Default for PackageCache {
    fn default() -> Self {
        Self::new(1024 * 1024 * 1024) // 1GB default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_operations() {
        let mut cache = PackageCache::new(1024);
        let entry = CacheEntry::new(PathBuf::from("/test"), 100, None);
        cache.add("test".to_string(), entry).unwrap();
        assert!(cache.get("test").is_some());
    }
}
