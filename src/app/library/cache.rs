use super::{AppLibrary, LibraryEntry, ScanRecord};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

const SCHEMA: u32 = 3;

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Cache {
    pub schema: u32,
    pub roots: Vec<PathBuf>,
    pub files: Vec<CachedFile>,
    pub walk_records: Vec<ScanRecord>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct CachedFile {
    pub path: PathBuf,
    pub signature: Signature,
    pub entries: Vec<LibraryEntry>,
    pub records: Vec<ScanRecord>,
    pub skipped_modes: usize,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Signature {
    size: u64,
    modified_secs: u64,
    modified_nanos: u32,
}

pub(super) fn signature(path: &Path) -> Option<Signature> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(Signature {
        size: metadata.len(),
        modified_secs: modified.as_secs(),
        modified_nanos: modified.subsec_nanos(),
    })
}

pub(super) fn default_directory() -> PathBuf {
    if let Some(directory) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(directory).join("zeff-rhythm/library-cache");
    }
    if let Some(directory) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(directory).join("zeff-rhythm/library-cache");
    }
    if let Some(directory) = std::env::var_os("HOME") {
        return PathBuf::from(directory).join(".cache/zeff-rhythm/library-cache");
    }
    std::env::temp_dir().join("zeff-rhythm-library-cache")
}

pub(super) fn root_identity(roots: &[PathBuf]) -> Vec<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_default();
    roots
        .iter()
        .map(|root| {
            normalize_identity(fs::canonicalize(root).unwrap_or_else(|_| {
                if root.is_absolute() {
                    root.clone()
                } else {
                    cwd.join(root)
                }
            }))
        })
        .collect()
}

pub(super) fn normalize_identity(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        if let Some(Component::Prefix(prefix)) = path.components().next() {
            let rest = path.strip_prefix(prefix.as_os_str()).unwrap();
            match prefix.kind() {
                Prefix::VerbatimDisk(drive) => {
                    return PathBuf::from(format!("{}:\\", char::from(drive))).join(rest);
                }
                Prefix::VerbatimUNC(server, share) => {
                    let mut root = std::ffi::OsString::from("\\\\");
                    root.push(server);
                    root.push("\\");
                    root.push(share);
                    root.push("\\");
                    return PathBuf::from(root).join(rest);
                }
                _ => {}
            }
        }
    }
    path
}

pub(super) fn path(directory: &Path, roots: &[PathBuf]) -> PathBuf {
    let mut hash = 0xcbf29ce484222325u64;
    for root in roots {
        for byte in root.to_string_lossy().as_bytes().iter().copied().chain([0]) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    directory.join(format!("library-{hash:016x}.toml"))
}

pub(super) fn load(directory: &Path, roots: &[PathBuf]) -> Option<Cache> {
    let source = fs::read_to_string(path(directory, roots)).ok()?;
    let cache: Cache = toml::from_str(&source).ok()?;
    (cache.schema == SCHEMA && cache.roots == roots).then_some(cache)
}

pub(super) fn save(directory: &Path, mut cache: Cache) -> std::io::Result<()> {
    cache.schema = SCHEMA;
    fs::create_dir_all(directory)?;
    let destination = path(directory, &cache.roots);
    let source = toml::to_string(&cache).map_err(std::io::Error::other)?;
    let nonce = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = destination.with_extension(format!("{}.{}.tmp", std::process::id(), nonce));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(source.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub(super) fn append(file: &CachedFile, library: &mut AppLibrary) {
    library.files_scanned += 1;
    library.skipped_modes += file.skipped_modes;
    let mut entries = file.entries.clone();
    let mut records = file.records.clone();
    for entry in &mut entries {
        if entry.problem.is_none() && !entry.audio_path.as_ref().is_some_and(|path| path.is_file())
        {
            entry.problem = Some("Audio file is missing".to_owned());
        }
        if let Some(record) = records
            .iter_mut()
            .find(|record| record.chart_index == Some(entry.chart_index))
        {
            record.status = if entry.problem.is_some() {
                "error"
            } else {
                "ready"
            }
            .to_owned();
            record.detail = entry.problem.as_ref().unwrap_or(&entry.subtitle).clone();
        }
    }
    library.entries.extend(entries);
    library.records.extend(records);
}

pub(super) fn snapshot(roots: &[PathBuf], directory: &Path) -> Option<AppLibrary> {
    let start = Instant::now();
    let cache = load(directory, &root_identity(roots))?;
    let mut library = AppLibrary {
        roots: roots.to_vec(),
        is_cached_snapshot: true,
        records: cache.walk_records,
        ..AppLibrary::default()
    };
    for file in &cache.files {
        append(file, &mut library);
    }
    super::scan::sort(&mut library);
    library.scan_metrics.cache_load_micros = start.elapsed().as_micros() as u64;
    library.scan_metrics.total_micros = library.scan_metrics.cache_load_micros;
    Some(library)
}

pub(super) mod format {
    use crate::play::ChartFormat;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(
        format: &ChartFormat,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match format {
            ChartFormat::OsuMania => "osu",
            ChartFormat::StepMania => "sm",
        })
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ChartFormat, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "osu" => Ok(ChartFormat::OsuMania),
            "sm" => Ok(ChartFormat::StepMania),
            _ => Err(serde::de::Error::custom("unknown chart format")),
        }
    }
}
