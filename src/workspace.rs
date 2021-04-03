use anyhow::anyhow;
use std::{
    ffi::OsString,
    fs::{self, Metadata},
    io,
    path::{Path, PathBuf},
};

pub struct Workspace {
    pathname: PathBuf,
}

impl Workspace {
    pub fn new<P: Into<PathBuf>>(pathname: P) -> Self {
        Self {
            pathname: pathname.into(),
        }
    }

    fn _list_files(&self, path: Option<&Path>) -> anyhow::Result<Vec<OsString>> {
        let path = path.unwrap_or(&self.pathname);

        let res = if std::fs::metadata(path)?.is_dir() {
            let dirs = std::fs::read_dir(path)?;
            let mut file_names = Vec::new();
            for dir in dirs {
                let path = dir?.path();
                if !&[".", "..", ".git"].iter().any(|&s| path.ends_with(s)) {
                    let file_name = path
                        .file_name()
                        .ok_or_else(|| anyhow!("Couldn't get path filename"))?
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
                .ok_or_else(|| anyhow!("Couldn't get relative path"))?
                .as_os_str()
                .to_owned()])
        };

        res
    }

    /// Lists all files in a path, relative to this workspace's base directory.
    pub fn list_files<P>(&self, path: P) -> anyhow::Result<Vec<OsString>>
    where
        P: AsRef<Path>,
    {
        self._list_files(Some(path.as_ref()))
    }

    /// Lists all files in a workspace's base directory.
    pub fn list_files_in_root(&self) -> anyhow::Result<Vec<OsString>> {
        self._list_files(None)
    }

    pub fn read_file<P: AsRef<Path>>(&self, path: P) -> io::Result<Vec<u8>> {
        std::fs::read(&self.pathname.join(&path))
    }

    pub fn stat_file<P: AsRef<Path>>(&self, path: P) -> io::Result<Metadata> {
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
