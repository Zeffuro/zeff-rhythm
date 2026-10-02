use std::fs::{self, File};
use std::io;
use std::path::Path;

pub fn acquire(path: &Path) -> io::Result<Option<File>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn second_instance_is_blocked_until_first_closes() {
        let path = std::env::temp_dir().join(format!("zeff-instance-{}.lock", std::process::id()));
        let first = super::acquire(&path).unwrap().unwrap();
        assert!(super::acquire(&path).unwrap().is_none());
        drop(first);
        assert!(super::acquire(&path).unwrap().is_some());
        std::fs::remove_file(path).unwrap();
    }
}
