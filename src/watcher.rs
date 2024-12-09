use std::{fs, path::Path, sync};

use anyhow::{Context, Result};
use chrono::Local;
use notify::{
    event::{Event, EventKind, ModifyKind, RenameMode},
    RecursiveMode, Watcher,
};
use tracing::{debug, error, info};

/// Watch the given file at path and stream back its content
///
/// Fails to detected when the path is replaced by an empty file
pub(crate) fn watch_path(
    path: &Path,
) -> Result<(impl notify::Watcher, sync::mpsc::Receiver<String>)> {
    // Event's paths are absolute
    let path = path.canonicalize().context("canonicalize file path")?;
    let (sender, receiver) = sync::mpsc::channel();

    let content = fs::read_to_string(&path).context("initial file read")?;
    sender.send(content).unwrap(); // receiver is still there

    let mut watcher = {
        let path = path.clone();
        notify::recommended_watcher(move |res: Result<Event, _>| {
            match res {
                Ok(event)
                    if event.paths.contains(&path)
                        && matches!(
                            event.kind,
                            EventKind::Modify(
                                ModifyKind::Data(_) | ModifyKind::Name(RenameMode::To)
                            )
                        ) =>
                {

                    info!("🔥 CHANGE DETECTED AT 👉{} ✅ NOW DOING UPDATE 🚀", Local::now().format("%H:%M:%S"));

                    match fs::read_to_string(&path) {
                        Ok(code) => {
                            if sender.send(code).is_err() {
                                debug!("file watcher found changes but receiver is gone");
                            }
                        }
                        Err(e) => error!("read file: {e}"),
                    }
                }
                Ok(_) => {} // not relevant
                Err(e) => error!("watching file's parent: {e}"),
            }
        })
    }
    .context("create file's parent watcher")?;

    watcher
        .watch(
            path.parent().context("file doesn't have a parent")?,
            RecursiveMode::NonRecursive,
        )
        .context("add parent directory watch")?;

    Ok((watcher, receiver))
}

#[cfg(test)]
mod tests {
    use super::watch_path;

    use std::{
        fs::{self, File},
        io::Write,
        sync::mpsc::TryRecvError,
    };

    use tempfile::TempDir;

    #[test]
    fn show_initial_content() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, "initial").unwrap();

        let (_watcher, changes) = watch_path(&file).unwrap();
        assert_eq!(changes.recv().unwrap(), "initial");

        assert_eq!(changes.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn does_nothing_on_read() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, "initial").unwrap();

        let (_watcher, changes) = watch_path(&file).unwrap();
        assert_eq!(changes.recv().unwrap(), "initial");

        fs::read(file).unwrap();

        assert_eq!(changes.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn handle_recreated() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, "initial").unwrap();

        let (_watcher, changes) = watch_path(&file).unwrap();
        assert_eq!(changes.recv().unwrap(), "initial");

        fs::remove_file(&file).unwrap();
        fs::write(&file, "recreated").unwrap();
        assert_eq!(changes.recv().unwrap(), "recreated");

        assert_eq!(changes.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn handle_renaming_in_place() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, "initial").unwrap();

        let (_watcher, changes) = watch_path(&file).unwrap();
        assert_eq!(changes.recv().unwrap(), "initial");

        let other_file = dir.path().join("other file");
        fs::write(&other_file, "renamed").unwrap();
        fs::rename(&other_file, &file).unwrap();
        assert_eq!(changes.recv().unwrap(), "renamed");

        assert_eq!(changes.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn handle_truncation() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, "initial").unwrap();

        let (_watcher, changes) = watch_path(&file).unwrap();
        assert_eq!(changes.recv().unwrap(), "initial");

        {
            fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(file)
                .unwrap()
                .write_all(b"truncated")
                .unwrap()
        }
        assert_eq!(changes.recv().unwrap(), "truncated");

        assert_eq!(changes.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn handle_flush() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, "initial").unwrap();

        let (_watcher, changes) = watch_path(&file).unwrap();
        assert_eq!(changes.recv().unwrap(), "initial");

        let mut openned = File::create(&file).unwrap();

        openned.write_all(b"flushed").unwrap();
        openned.flush().unwrap();
        assert_eq!(changes.recv().unwrap(), "flushed");

        openned.write_all(b" and closed").unwrap();
        drop(file);
        assert_eq!(changes.recv().unwrap(), "flushed and closed");

        assert_eq!(changes.try_recv(), Err(TryRecvError::Empty));
    }
}
