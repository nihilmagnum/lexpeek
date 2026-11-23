use std::fs;
use std::io;
use std::path::Path;

use anyhow::{Context, Result};

/// Replaces the file at `path` with the one `write` creates at the temporary path it is given: a file written
/// only partially, for example because the process was killed, never takes the place of the previous one.
pub fn replace_file(path: &Path, write: impl FnOnce(&Path) -> io::Result<()>) -> Result<()> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory).with_context(|| format!("cannot create {}", directory.display()))?;
    }
    let partial = path.with_extension("part");
    write(&partial).with_context(|| format!("cannot write {}", partial.display()))?;
    fs::rename(&partial, path).with_context(|| format!("cannot write {}", path.display()))
}
