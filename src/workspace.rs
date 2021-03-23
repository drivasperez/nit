use anyhow::anyhow;
use std::{
    ffi::OsString,
    fs, io,
    os::unix::prelude::MetadataExt,
    path::{Path, PathBuf},
};

#[derive(Debug, Copy, Clone)]
pub enum EntryMode {
    Executable,
    Regular,
}

impl From<fs::Metadata> for EntryMode {
    fn from(metadata: fs::Metadata) -> Self {
        let mode = metadata.mode();
        match (mode & 0o111) != 0 {
            true => Self::Executable,
            false => Self::Regular,
        }
    }
}

pub struct Workspace {
    pathname: PathBuf,
}

impl Workspace {
    pub fn new<P: Into<PathBuf>>(pathname: P) -> Self {
        Self {
            pathname: pathname.into(),
        }
    }

    pub fn list_files(&self) -> anyhow::Result<Vec<OsString>> {
        self._list_files(None)
    }

    fn _list_files(&self, dir: Option<&Path>) -> anyhow::Result<Vec<OsString>> {
        let dir = dir.unwrap_or(&self.pathname);

        let dirs = std::fs::read_dir(dir)?;
        let mut file_names = Vec::new();
        for dir in dirs {
            let path = dir?.path();
            if !&[".", "..", ".git"].iter().any(|&s| path.ends_with(s)) {
                let file_name = path
                    .file_name()
                    .ok_or(anyhow!("Couldn't get path filename"))?
                    .to_owned();

                file_names.push(file_name);
            }
        }

        let file_names = file_names
            .iter()
            .map(|name| -> anyhow::Result<Vec<OsString>> {
                let path = dir.join(name);

                if path.is_dir() {
                    self._list_files(Some(&path))
                } else {
                    Ok(vec![crate::utils::diff_paths(path, &self.pathname)
                        .ok_or(anyhow!("Couldn't get relative path"))?
                        .as_os_str()
                        .to_owned()])
                }
            })
            .flat_map(|result| match result {
                Ok(vec) => vec.into_iter().map(|item| Ok(item)).collect(),
                Err(e) => vec![Err(e)],
            })
            .collect();

        println!("{:?}", file_names);
        file_names
    }

    pub fn read_file<P: AsRef<Path>>(&self, path: P) -> io::Result<Vec<u8>> {
        std::fs::read(&self.pathname.join(&path))
    }

    pub fn stat_file<P: AsRef<Path>>(&self, path: P) -> io::Result<EntryMode> {
        let metadata = fs::metadata(&path)?;
        Ok(EntryMode::from(metadata))
    }
}
