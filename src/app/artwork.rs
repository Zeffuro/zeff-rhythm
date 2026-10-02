use super::library::resolve_artwork_path;
use image::{ImageFormat, ImageReader, Limits, imageops::FilterType};
use std::collections::VecDeque;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime};

#[cfg(test)]
mod tests;

const DEBOUNCE: Duration = Duration::from_millis(100);
const MAX_ENCODED_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DECODE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_DIMENSION: u32 = 8192;
const CACHE_BYTES: usize = 32 * 1024 * 1024;
const CACHE_ENTRIES: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtworkRequest {
    pub chart_path: PathBuf,
    pub background_path: Option<PathBuf>,
    pub banner_path: Option<PathBuf>,
}

#[derive(Debug)]
pub struct DecodedArtwork {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub fit: ArtworkFit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtworkFit {
    Cover,
    Contain,
}

pub struct ArtworkUpdate {
    pub image: Option<Arc<DecodedArtwork>>,
}

#[derive(Default)]
pub struct ArtworkLoader {
    generation: u64,
    pending: Option<PendingArtwork>,
    active: Option<ActiveArtwork>,
    cache: ArtworkCache,
    clear_pending: bool,
}

struct PendingArtwork {
    generation: u64,
    ready_at: Instant,
    request: ArtworkRequest,
}

struct ActiveArtwork {
    generation: u64,
    receiver: Receiver<(ArtworkCache, Option<Arc<DecodedArtwork>>)>,
}

impl ArtworkLoader {
    pub fn is_loading(&self) -> bool {
        self.pending.is_some() || self.active.is_some()
    }

    pub fn request(&mut self, request: Option<ArtworkRequest>) {
        self.request_at(request, Instant::now());
    }

    fn request_at(&mut self, request: Option<ArtworkRequest>, now: Instant) {
        self.generation = self.generation.wrapping_add(1);
        self.clear_pending = request.is_none();
        self.pending = request.map(|request| PendingArtwork {
            generation: self.generation,
            ready_at: now + DEBOUNCE,
            request,
        });
    }

    pub fn poll(&mut self) -> Option<ArtworkUpdate> {
        self.poll_at(Instant::now())
    }

    fn poll_at(&mut self, now: Instant) -> Option<ArtworkUpdate> {
        if std::mem::take(&mut self.clear_pending) {
            return Some(ArtworkUpdate { image: None });
        }
        let mut update = None;
        if let Some(active) = &self.active {
            match active.receiver.try_recv() {
                Ok((cache, image)) => {
                    if active.generation == self.generation {
                        update = Some(ArtworkUpdate { image });
                    }
                    self.cache = cache;
                    self.active = None;
                }
                Err(TryRecvError::Disconnected) => {
                    if active.generation == self.generation {
                        update = Some(ArtworkUpdate { image: None });
                    }
                    self.active = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.pending.as_ref().is_some_and(|pending| {
            now >= pending.ready_at
                && pending.request.background_path.is_none()
                && pending.request.banner_path.is_none()
        }) {
            self.pending = None;
            update = Some(ArtworkUpdate { image: None });
        }
        if self.active.is_none()
            && self
                .pending
                .as_ref()
                .is_some_and(|pending| now >= pending.ready_at)
        {
            let pending = self.pending.take().unwrap();
            let mut cache = std::mem::take(&mut self.cache);
            let (sender, receiver) = mpsc::channel();
            let started = std::thread::Builder::new()
                .name("chart-artwork-loader".into())
                .spawn(move || {
                    let image = cache.load(&pending.request);
                    let _ = sender.send((cache, image));
                });
            if started.is_ok() {
                self.active = Some(ActiveArtwork {
                    generation: pending.generation,
                    receiver,
                });
            } else {
                update = Some(ArtworkUpdate { image: None });
            }
        }
        update
    }
}

#[derive(Default)]
struct ArtworkCache {
    entries: VecDeque<CachedArtwork>,
    bytes: usize,
}

struct CachedArtwork {
    path: PathBuf,
    signature: Signature,
    image: Arc<DecodedArtwork>,
}

#[derive(Clone, PartialEq, Eq)]
struct Signature {
    size: u64,
    modified: SystemTime,
}

impl ArtworkCache {
    fn load(&mut self, request: &ArtworkRequest) -> Option<Arc<DecodedArtwork>> {
        request
            .background_path
            .as_deref()
            .and_then(|path| self.load_path(&request.chart_path, path, ArtworkFit::Cover))
            .or_else(|| {
                request
                    .banner_path
                    .as_deref()
                    .and_then(|path| self.load_path(&request.chart_path, path, ArtworkFit::Contain))
            })
    }

    fn load_path(
        &mut self,
        chart_path: &Path,
        candidate: &Path,
        fit: ArtworkFit,
    ) -> Option<Arc<DecodedArtwork>> {
        let path = resolve_artwork_path(chart_path, candidate)?;
        let signature = signature(&path)?;
        self.entries
            .retain(|entry| entry.path != path || entry.signature == signature);
        self.recount();
        if signature.size > MAX_ENCODED_BYTES {
            return None;
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.path == path && entry.image.fit == fit)
        {
            let entry = self.entries.remove(index).unwrap();
            let image = entry.image.clone();
            self.entries.push_back(entry);
            return Some(image);
        }
        let file = fs::File::open(&path).ok()?;
        let mut bytes = Vec::with_capacity(signature.size as usize + 1);
        file.take(MAX_ENCODED_BYTES + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() as u64 > MAX_ENCODED_BYTES {
            return None;
        }
        let mut image = decode(&bytes)?;
        image.fit = fit;
        let image = Arc::new(image);
        if self::signature(&path).as_ref() != Some(&signature) {
            return None;
        }
        self.insert(path, signature, image.clone());
        Some(image)
    }

    fn insert(&mut self, path: PathBuf, signature: Signature, image: Arc<DecodedArtwork>) {
        let bytes = image.rgba.capacity();
        if bytes > CACHE_BYTES {
            return;
        }
        while self.bytes + bytes > CACHE_BYTES || self.entries.len() >= CACHE_ENTRIES {
            let Some(old) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= old.image.rgba.capacity();
        }
        self.bytes += bytes;
        self.entries.push_back(CachedArtwork {
            path,
            signature,
            image,
        });
    }

    fn recount(&mut self) {
        self.bytes = self
            .entries
            .iter()
            .map(|entry| entry.image.rgba.capacity())
            .sum();
    }
}

fn signature(path: &Path) -> Option<Signature> {
    let metadata = fs::metadata(path).ok()?;
    Some(Signature {
        size: metadata.len(),
        modified: metadata.modified().ok()?,
    })
}

fn decode(bytes: &[u8]) -> Option<DecodedArtwork> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Bmp)
    ) {
        return None;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let image = if image.width() > 1600 || image.height() > 900 {
        image.resize(1600, 900, FilterType::Triangle)
    } else {
        image
    }
    .to_rgba8();
    Some(DecodedArtwork {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
        fit: ArtworkFit::Cover,
    })
}
