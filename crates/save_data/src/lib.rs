pub mod plot_data;

use std::fs::{self, File};
use std::io;
use std::path::Path;

/// `path` is never left half written.
pub fn replace_file<E: From<io::Error>>(
    path: impl AsRef<Path>,
    write: impl FnOnce(&mut File) -> Result<(), E>,
) -> Result<(), E> {
    let path = path.as_ref();
    let mut unfinished = path.as_os_str().to_owned();
    unfinished.push(".tmp");
    let mut file = File::create(&unfinished)?;
    write(&mut file)?;
    file.sync_data()?;
    fs::rename(unfinished, path)?;
    Ok(())
}
