use crate::platform::audio::AudioClip;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

const DEBOUNCE: Duration = Duration::from_millis(300);
type LoadResult = Result<Arc<AudioClip>, String>;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PreviewRequest {
    pub path: PathBuf,
    pub start_seconds: Option<f64>,
    pub duration_seconds: Option<f64>,
}

#[derive(Default)]
pub(super) struct PreviewLoader {
    request: Option<PreviewRequest>,
    generation: u64,
    ready_at: Option<Instant>,
    active: Option<ActiveLoad>,
}

struct ActiveLoad {
    generation: u64,
    receiver: Receiver<LoadResult>,
}

impl PreviewLoader {
    pub fn request(&mut self, request: Option<PreviewRequest>, now: Instant) -> bool {
        if self.request == request {
            return false;
        }
        self.generation = self.generation.wrapping_add(1);
        self.ready_at = request.as_ref().map(|_| now + DEBOUNCE);
        self.request = request;
        true
    }

    pub fn is_loading(&self) -> bool {
        self.ready_at.is_some() || self.active.is_some()
    }

    pub fn poll(&mut self) -> Option<Result<(PreviewRequest, Arc<AudioClip>), String>> {
        self.poll_with(Instant::now(), |request| {
            let path = request.path.clone();
            let (sender, receiver) = mpsc::channel();
            std::thread::Builder::new()
                .name("song-preview-loader".into())
                .spawn(move || {
                    let result =
                        crate::play::load_cached_audio(&path).map_err(|error| error.to_string());
                    let _ = sender.send(result);
                })
                .map_err(|error| error.to_string())?;
            Ok(receiver)
        })
    }

    fn poll_with(
        &mut self,
        now: Instant,
        start: impl FnOnce(&PreviewRequest) -> Result<Receiver<LoadResult>, String>,
    ) -> Option<Result<(PreviewRequest, Arc<AudioClip>), String>> {
        let mut update = None;
        if let Some(active) = &self.active {
            let result = match active.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Disconnected) => Some(Err("Preview loader stopped".into())),
                Err(TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                if active.generation == self.generation {
                    if let Some(request) = self.request.clone() {
                        update = Some(result.map(|clip| (request, clip)));
                    }
                }
                self.active = None;
            }
        }
        if self.active.is_none() && self.ready_at.is_some_and(|ready| now >= ready) {
            self.ready_at = None;
            if let Some(request) = &self.request {
                match start(request) {
                    Ok(receiver) => {
                        self.active = Some(ActiveLoad {
                            generation: self.generation,
                            receiver,
                        })
                    }
                    Err(error) => update = Some(Err(error)),
                }
            }
        }
        update
    }
}

#[cfg(test)]
mod tests;
