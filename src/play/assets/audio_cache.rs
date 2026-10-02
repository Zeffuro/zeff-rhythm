use crate::platform::audio::{AudioClip, load_audio_clip};
use std::collections::VecDeque;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

const MAX_BYTES: usize = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 8;
static CACHE: OnceLock<Mutex<ClipCache>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileIdentity {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

impl FileIdentity {
    fn read(path: &Path) -> Result<Self, Box<dyn Error>> {
        let path = fs::canonicalize(path)?;
        let metadata = fs::metadata(&path)?;
        Ok(Self {
            path,
            bytes: metadata.len(),
            modified: metadata.modified()?,
        })
    }
}

struct Entry {
    identity: FileIdentity,
    clip: Arc<AudioClip>,
    bytes: usize,
}

struct ClipCache {
    entries: VecDeque<Entry>,
    bytes: usize,
    max_bytes: usize,
    max_entries: usize,
}

impl ClipCache {
    fn new(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            max_bytes,
            max_entries,
        }
    }

    fn lookup(&mut self, identity: &FileIdentity) -> Option<Arc<AudioClip>> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.identity.path == identity.path)?;
        let entry = self.entries.remove(index)?;
        if entry.identity != *identity {
            self.bytes -= entry.bytes;
            return None;
        }
        let clip = Arc::clone(&entry.clip);
        self.entries.push_back(entry);
        Some(clip)
    }

    fn admit(&mut self, identity: FileIdentity, clip: Arc<AudioClip>) -> Arc<AudioClip> {
        if let Some(existing) = self.lookup(&identity) {
            return existing;
        }
        // Capacity accounts for the allocation retained by the cache's Arc.
        let bytes = clip.samples.capacity().saturating_mul(size_of::<f32>());
        if bytes > self.max_bytes || self.max_entries == 0 {
            return clip;
        }
        while self.entries.len() >= self.max_entries
            || bytes > self.max_bytes.saturating_sub(self.bytes)
        {
            let Some(entry) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= entry.bytes;
        }
        self.bytes += bytes;
        self.entries.push_back(Entry {
            identity,
            clip: Arc::clone(&clip),
            bytes,
        });
        clip
    }
}

pub(crate) fn load(path: &Path) -> Result<Arc<AudioClip>, Box<dyn Error>> {
    let cache = CACHE.get_or_init(|| Mutex::new(ClipCache::new(MAX_BYTES, MAX_ENTRIES)));
    load_with(path, cache, load_audio_clip)
}

fn load_with(
    path: &Path,
    cache: &Mutex<ClipCache>,
    decode: impl FnOnce(&Path) -> Result<AudioClip, Box<dyn Error>>,
) -> Result<Arc<AudioClip>, Box<dyn Error>> {
    let identity = FileIdentity::read(path)?;
    let cached = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .lookup(&identity);
    if let Some(clip) = cached {
        if FileIdentity::read(path)? == identity {
            return Ok(clip);
        }
        return Err("audio file changed while loading; retry the song".into());
    }

    // File IO and decoding never hold the shared cache lock or create native devices.
    let clip = Arc::new(decode(&identity.path)?);
    if FileIdentity::read(path)? != identity {
        return Err("audio file changed while decoding; retry the song".into());
    }
    Ok(cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .admit(identity, clip))
}

#[cfg(test)]
mod tests;
