use std::{
    fs::{self, Metadata},
    path::{Path, PathBuf},
};
use thiserror::Error;

use crate::Result;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WorkspaceError {
    #[error("Couldn't get path: {0}")]
    Path(PathBuf),
    #[error("Couldn't parse OsString")]
    CouldNotParseString,
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

    fn _list_files(&self, path: Option<&Path>) -> Result<Vec<PathBuf>> {
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
            let s = crate::utils::diff_paths(path, &self.pathname);
            Ok(vec![s
                .ok_or_else(|| WorkspaceError::Path(path.to_owned()))?
                .to_owned()])
        };

        res
    }

    /// Lists all files in a path, relative to this workspace's base directory.
    pub fn list_files<P>(&self, path: P) -> Result<Vec<PathBuf>>
    where
        P: AsRef<Path>,
    {
        self._list_files(Some(path.as_ref()))
    }

    /// Lists all files in a workspace's base directory.
    pub fn list_files_in_root(&self) -> Result<Vec<PathBuf>> {
        self._list_files(None)
    }

    /// Read a file's contents into a Vec<u8>, based on a path relative to this workspace's base directory.
    pub fn read_file<P: AsRef<Path>>(&self, path: P) -> Result<Vec<u8>> {
        let r = std::fs::read(&self.pathname.join(&path))?;
        Ok(r)
    }

    /// Get a file's metadata, based on a path relative to this workspace's base directory.
    pub fn stat_file<P: AsRef<Path>>(&self, path: P) -> Result<Metadata> {
        let metadata = fs::metadata(&self.pathname.join(path))?;
        Ok(metadata)
    }

    pub fn list_dir(&self, path: &impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        todo!()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn list_files() {
        let tmp_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tmp")
            .join("workspace-list-files");
        std::fs::create_dir_all(&tmp_path).unwrap();

        std::fs::write(tmp_path.join("hello.txt"), "Hey world").unwrap();
        std::fs::write(tmp_path.join("goodbye.txt"), "Hey world").unwrap();
        std::fs::write(tmp_path.join("okay.txt"), "Hey world").unwrap();

        std::fs::create_dir(tmp_path.join("a")).unwrap();
        std::fs::create_dir(tmp_path.join("a").join("b")).unwrap();
        std::fs::write(tmp_path.join("a").join("b").join("what.txt"), "what?").unwrap();

        let ws = Workspace::new(&tmp_path);

        let entries = ws.list_files_in_root().unwrap();

        assert_eq!(
            entries
                .iter()
                .map(|p| p.to_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["a/b/what.txt", "goodbye.txt", "okay.txt", "hello.txt",]
        );

        std::fs::remove_dir_all(&tmp_path).unwrap();
    }
}
