use std::{
    ffi::OsString,
    fs::{self, Metadata},
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("Unexpected IO Error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Couldn't get path: {0}")]
    Path(PathBuf),
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

    fn _list_files(&self, path: Option<&Path>) -> Result<Vec<OsString>, WorkspaceError> {
        let path = path.unwrap_or(&self.pathname);

        let res = if std::fs::metadata(path)?.is_dir() {
            let dirs = std::fs::read_dir(path)?;
            let mut file_names = Vec::new();
            for dir in dirs {
                let path = dir?.path();
                if !&[".", "..", ".git"].iter().any(|&s| path.ends_with(s)) {
                    let file_name = path
                        .file_name()
                        .ok_or_else(|| WorkspaceError::Path(path.clone()))?
                        .to_owned();

                    file_names.push(file_name);
                }
            }
            file_names
                .iter()
                .map(|name| self._list_files(Some(&path.join(name))))
                .flat_map(|result| match result {
                    Ok(vec) => vec.into_iter().map(Ok).collect(),
                    Err(e) => vec![Err(e)],
                })
                .collect()
        } else {
            Ok(vec![crate::utils::diff_paths(path, &self.pathname)
                .ok_or_else(|| WorkspaceError::Path(path.to_owned()))?
                .as_os_str()
                .to_owned()])
        };

        res
    }

    /// Lists all files in a path, relative to this workspace's base directory.
    pub fn list_files<P>(&self, path: P) -> Result<Vec<OsString>, WorkspaceError>
    where
        P: AsRef<Path>,
    {
        self._list_files(Some(path.as_ref()))
    }

    /// Lists all files in a workspace's base directory.
    pub fn list_files_in_root(&self) -> Result<Vec<OsString>, WorkspaceError> {
        self._list_files(None)
    }

    pub fn read_file<P: AsRef<Path>>(&self, path: P) -> Result<Vec<u8>, WorkspaceError> {
        let r = std::fs::read(&self.pathname.join(&path))?;
        Ok(r)
    }

    pub fn stat_file<P: AsRef<Path>>(&self, path: P) -> Result<Metadata, WorkspaceError> {
        let metadata = fs::metadata(&path)?;
        Ok(metadata)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn list_files() {
        let ws = Workspace::new("../testnit");

        let entries = ws.list_files_in_root().unwrap();

        assert_eq!(
            entries,
            vec![
                "woop.txt",
                "a/b/hello.txt",
                "cool.rs",
                "hi.c",
                "COMMIT_MSG.txt",
                "wap.json"
            ]
        );
    }
}
