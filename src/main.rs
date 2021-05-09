use anyhow::anyhow;
use anyhow::Context;
use chrono::Utc;
use nit::{
    database::{Author, Blob, Commit, Database, Tree},
    index::Index,
    lockfile::LockfileError,
    refs::Refs,
    workspace::Workspace,
    Status,
};
use std::fs;
use std::path::Path;
use std::{env, io::Read};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
enum Opt {
    /// Creates a new repository
    Init {
        #[structopt(default_value = ".")]
        path: String,
    },
    /// Record changes to the repository
    Commit {
        #[structopt(long = "message", short = "m")]
        message: Option<String>,
    },
    /// Add file contents to the index
    Add { paths: Vec<String> },

    /// Show the working tree status
    Status,
}

fn handle_opt(opt: Opt, root_path: &Path) -> anyhow::Result<()> {
    match opt {
        Opt::Init { path } => init_repository(&path.as_ref())?,
        Opt::Add { paths } => {
            let paths = paths.iter().map(Path::new).collect();
            add_files_to_repository(paths, &root_path)?;
        }
        Opt::Commit { message } => {
            let msg = create_commit(message, &std::env::current_dir()?)?;
            print!("{}", msg);
        }
        Opt::Status => {
            let msg = get_repository_status(&root_path)?;
            print!("{}", msg);
        }
    };

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let root_path = std::env::current_dir()?;

    handle_opt(opt, &root_path)
}

fn init_repository(path: &Path) -> anyhow::Result<()> {
    let root_path = fs::canonicalize(Path::new(path))?;
    let git_path = root_path.join(".git");
    for &dir in ["objects", "refs"].iter() {
        fs::create_dir_all(git_path.join(dir))?;
    }

    println!(
        "Initialised empty Nit repository in {}",
        git_path.to_str().unwrap_or("Unknown")
    );

    Ok(())
}

fn add_files_to_repository(paths: Vec<&Path>, root_path: &Path) -> anyhow::Result<()> {
    let git_path = root_path.join(".git");
    let mut index = Index::new(git_path.join("index"));
    let workspace = Workspace::new(&root_path);
    let database = Database::new(git_path.join("objects"));

    // Please, try-blocks, please.
    (|| -> anyhow::Result<()> {
        index
            .load_for_update()
            .context("Couldn't load for update")?;

        let paths: Result<Vec<_>, anyhow::Error> = paths
            .into_iter()
            .map(|path| {
                let path = std::fs::canonicalize(&path)
                    .with_context(|| format!("Couldn't add file: {:?}", &path))?;

                let res = workspace
                    .list_files(&path)
                    .with_context(|| format!("Couldn't add file: {:?}", &path))?;

                Ok(res)
            })
            .collect();

        let paths: Vec<_> = paths?.into_iter().flatten().collect();

        for pathname in paths {
            let data = workspace.read_file(&pathname).context("No data")?;
            let stat = workspace.stat_file(&pathname).context("No stat")?;
            let blob = Blob::new(data);
            let blob_oid = database.store(&blob).context("No oid")?;

            index.add(&pathname, blob_oid, stat);
        }

        index.write_updates()?;
        Ok(())
    })()
    .or_else(|e| {
        // Cleanup lockfile if we had issues
        if let Some(nit::Error::Lockfile(LockfileError::LockDenied(_))) = e.downcast_ref() {
            // We couldn't get the lock, so leave it in place.
        } else {
            index.lockfile_mut().rollback()?;
        }

        Err(e)
    })
}

fn get_repository_status(root_path: &Path) -> anyhow::Result<String> {
    let mut status = Status::new(&root_path);
    Ok(status.get()?)
}

fn create_commit(message: Option<String>, root_path: &Path) -> anyhow::Result<String> {
    let git_path = root_path.join(".git");
    let mut index = Index::new(git_path.join("index"));
    let database = Database::new(git_path.join("objects"));
    let refs = Refs::new(&git_path);

    (|| -> anyhow::Result<String> {
        index.load()?;

        let mut root = Tree::build(index.entries().values().cloned().collect());
        root.traverse(&mut |tree| {
            let oid = database.store(tree)?;
            Ok(oid)
        })?;

        let root_oid = database.store(&root)?;

        let parent = refs.read_head();
        let name = env::var("GIT_AUTHOR_NAME")
            .context("Could not load GIT_AUTHOR_NAME environment variable")?;
        let email = env::var("GIT_AUTHOR_EMAIL")
            .context("Could not load GIT_AUTHOR_EMAIL environment variable")?;

        let author = Author::new(name, email, Utc::now());

        let msg = message
            .or_else(|| {
                let mut msg = Vec::new();
                std::io::stdin().read_to_end(&mut msg).ok()?;
                let str = String::from_utf8(msg).ok()?;
                Some(str)
            })
            .ok_or_else(|| anyhow!("No commit message, aborting"))?;

        let commit = Commit::new(parent.as_deref(), root_oid, author, msg);
        let commit_oid = database.store(&commit)?;

        refs.update_head(&commit_oid)?;

        let root_msg = match parent {
            Some(_) => "",
            None => "(root-commit) ",
        };

        let msg = format!(
            "[{}{}] {}",
            root_msg,
            commit_oid,
            commit.message().lines().next().unwrap_or("")
        );

        Ok(msg)
    })()
    .or_else(|e| {
        // Cleanup lockfile if we had issues
        if let Some(nit::Error::Lockfile(LockfileError::LockDenied(_))) = e.downcast_ref() {
            // We couldn't get the lock, so leave it in place.
        } else {
            index.lockfile_mut().rollback()?;
        }

        Err(e)
    })
}

#[cfg(test)]
mod test {
    use std::os::unix::fs::PermissionsExt;
    use std::{fs::File, io::prelude::*, path::PathBuf};
    const REGULAR_MODE: u32 = 0o100644;
    const EXECUTABLE_MODE: u32 = 0o100755;

    use super::*;

    fn tmp_path(subdir: &dyn AsRef<Path>) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tmp")
            .join(subdir.as_ref())
    }

    fn init(subdir: &dyn AsRef<Path>) -> anyhow::Result<()> {
        std::fs::create_dir(tmp_path(subdir))?;
        let path = tmp_path(subdir);
        init_repository(&path)
    }

    fn cleanup(subdir: &dyn AsRef<Path>) -> anyhow::Result<()> {
        let path = tmp_path(subdir);
        std::fs::remove_dir_all(path)?;
        Ok(())
    }

    #[test]
    fn inits_a_repository() {
        let subdir = "inits";
        init(&subdir).unwrap();
        let dirs: Vec<_> = std::fs::read_dir(tmp_path(&subdir).join(".git"))
            .unwrap()
            .map(|p| {
                let p = p.unwrap();
                p.file_name()
            })
            .collect();

        assert_eq!(dirs, vec!["refs", "objects"]);

        cleanup(&subdir).unwrap();
    }

    #[test]
    fn adds_a_file_to_the_index() {
        let subdir = "adds";
        init(&subdir).unwrap();
        let git_dir = tmp_path(&subdir).join(".git");
        let index_dir = git_dir.join("index");
        let mut index = Index::new(index_dir);

        let file_path = tmp_path(&subdir).join("hello.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();

        add_files_to_repository(vec![&file_path], &tmp_path(&subdir)).unwrap();

        index.load_for_update().unwrap();

        let entries: Vec<_> = index
            .entries()
            .values()
            .map(|entry| (entry.mode(), entry.path()))
            .collect();

        assert_eq!(entries, vec![(REGULAR_MODE, Path::new("hello.txt"))]);
        cleanup(&subdir).unwrap();
    }

    #[test]
    fn adds_an_executable_file_to_the_index() {
        let subdir = "adds_executable";
        init(&subdir).unwrap();
        let git_dir = tmp_path(&subdir).join(".git");
        let mut index = Index::new(git_dir.join("index"));
        let file_path = tmp_path(&subdir).join("hello.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();

        // Set it to executable.
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o755);
        file.set_permissions(permissions).unwrap();

        add_files_to_repository(vec![&file_path], &tmp_path(&subdir)).unwrap();

        index.load_for_update().unwrap();

        let entries: Vec<_> = index
            .entries()
            .values()
            .map(|entry| (entry.mode(), entry.path()))
            .collect();

        assert_eq!(entries, vec![(EXECUTABLE_MODE, Path::new("hello.txt"))]);
        cleanup(&subdir).unwrap();
    }

    #[test]
    fn adds_multiple_files_to_index() {
        let subdir = "adds_multiple";
        init(&subdir).unwrap();
        let git_dir = tmp_path(&subdir).join(".git");
        let mut index = Index::new(git_dir.join("index"));

        let file_path = tmp_path(&subdir).join("hello.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();

        let file_path_2 = tmp_path(&subdir).join("hohoho.txt");
        let mut file = File::create(&file_path_2).unwrap();
        file.write_all("Merry christmas!".as_bytes()).unwrap();

        add_files_to_repository(vec![&file_path, &file_path_2], &tmp_path(&subdir)).unwrap();

        index.load_for_update().unwrap();

        let entries: Vec<_> = index
            .entries()
            .values()
            .map(|entry| (entry.mode(), entry.path()))
            .collect();

        assert_eq!(
            entries,
            vec![
                (REGULAR_MODE, Path::new("hello.txt")),
                (REGULAR_MODE, Path::new("hohoho.txt"))
            ]
        );
        cleanup(&subdir).unwrap();
    }

    #[test]
    fn incrementally_add_files_to_index() {
        let subdir = "adds_incrementally";
        init(&subdir).unwrap();
        let git_dir = tmp_path(&subdir).join(".git");
        let mut index = Index::new(git_dir.join("index"));
        let file_path = tmp_path(&subdir).join("hello.txt");

        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();
        add_files_to_repository(vec![&file_path], &tmp_path(&subdir)).unwrap();

        index.load_for_update().unwrap();

        let entries: Vec<_> = index
            .entries()
            .values()
            .map(|entry| (entry.mode(), entry.path()))
            .collect();

        assert_eq!(entries, vec![(REGULAR_MODE, Path::new("hello.txt"))]);

        // Add another file, reload and reread entries

        let file_path_2 = tmp_path(&subdir).join("hohoho.txt");
        let mut file = File::create(&file_path_2).unwrap();
        file.write_all("Merry christmas!".as_bytes()).unwrap();

        add_files_to_repository(vec![&file_path_2], &tmp_path(&subdir)).unwrap();

        index.load_for_update().unwrap();

        let entries: Vec<_> = index
            .entries()
            .values()
            .map(|entry| (entry.mode(), entry.path()))
            .collect();

        assert_eq!(
            entries,
            vec![
                (REGULAR_MODE, Path::new("hello.txt")),
                (REGULAR_MODE, Path::new("hohoho.txt"))
            ]
        );

        cleanup(&subdir).unwrap();
    }

    #[test]
    fn adds_a_directory_to_the_index() {
        let subdir = "adds_dir";
        let tmp_path = tmp_path(&subdir);
        let git_dir = tmp_path.join(".git");
        let mut index = Index::new(git_dir.join("index"));

        init(&subdir).unwrap();

        std::fs::create_dir(tmp_path.join("a")).unwrap();

        let file_path = tmp_path.join("hello.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();

        let file_path_2 = tmp_path.join("hohoho.txt");
        let mut file = File::create(&file_path_2).unwrap();
        file.write_all("Merry christmas!".as_bytes()).unwrap();

        let file_path_3 = tmp_path.join("a").join("b.txt");
        let mut file = File::create(&file_path_3).unwrap();
        file.write_all("bbbb".as_bytes()).unwrap();

        let file_path_4 = tmp_path.join("a").join("c.txt");
        let mut file = File::create(&file_path_4).unwrap();
        file.write_all("cccc".as_bytes()).unwrap();

        add_files_to_repository(vec![&tmp_path.join("a")], &tmp_path).unwrap();

        index.load_for_update().unwrap();

        let entries: Vec<_> = index
            .entries()
            .values()
            .map(|entry| (entry.mode(), entry.path()))
            .collect();

        assert_eq!(
            entries,
            vec![
                (REGULAR_MODE, Path::new("a/b.txt")),
                (REGULAR_MODE, Path::new("a/c.txt"))
            ]
        );
        cleanup(&subdir).unwrap();
    }

    #[test]
    fn fails_for_non_existent_files() {
        let subdir = "non_existent";
        let tmp_path = tmp_path(&subdir);

        init(&subdir).unwrap();

        assert!(add_files_to_repository(vec![&tmp_path.join("a")], &tmp_path).is_err());

        cleanup(&subdir).unwrap();
    }
    #[test]
    fn fails_for_unreadable_existent_files() {
        let subdir = "unreadable";
        let tmp_path = tmp_path(&subdir);

        init(&subdir).unwrap();

        let file = File::create(tmp_path.join("shhh.txt")).unwrap();

        let mut permissions = file.metadata().unwrap().permissions();
        let mode = permissions.mode();
        // Set it to unreadable.
        permissions.set_mode(mode & 0b1011111111);
        file.set_permissions(permissions).unwrap();

        // assert!(add_files_to_repository(vec![&tmp_path.join("shhh.txt")], &tmp_path).is_err());

        cleanup(&subdir).unwrap();
    }

    #[test]
    fn makes_a_commit() {
        let subdir = "commits";
        let tmp_path = tmp_path(&subdir);

        init(&subdir).unwrap();
        let file_path = &tmp_path.join("hello.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();

        add_files_to_repository(vec![&file_path], &tmp_path).unwrap();

        create_commit(Some("Commit message is here".to_owned()), &tmp_path).unwrap();

        cleanup(&subdir).unwrap();
    }

    #[test]
    fn lists_untracked_files_in_name_order() {
        let subdir = "lists_untracked_files";
        let tmp_path = tmp_path(&subdir);

        init(&subdir).unwrap();

        let file_path = &tmp_path.join("hello.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();

        let file_path = &tmp_path.join("goodbye.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();

        let status = get_repository_status(&tmp_path).unwrap();

        assert_eq!(status, "?? goodbye.txt\n?? hello.txt");
        cleanup(&subdir).unwrap();
    }

    #[test]
    fn lists_files_as_untracked_if_not_in_the_index() {
        let subdir = "lists_unindexed_files";
        let tmp_path = tmp_path(&subdir);

        init(&subdir).unwrap();

        let file_path = &tmp_path.join("hello.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();

        add_files_to_repository(vec![&file_path], &tmp_path).unwrap();
        create_commit(Some(String::from("Commit message")), &tmp_path).unwrap();

        let file_path = &tmp_path.join("goodbye.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Goodbye".as_bytes()).unwrap();

        let status = get_repository_status(&tmp_path).unwrap();

        assert_eq!(status, "?? goodbye.txt");
        cleanup(&subdir).unwrap();
    }

    #[test]
    fn lists_untracked_directories_instead_of_their_contents() {
        let subdir = "lists_untracked_directories";
        let tmp_path = tmp_path(&subdir);

        init(&subdir).unwrap();

        let file_path = &tmp_path.join("hello.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();

        let dir_path = tmp_path.join("nested");
        let file_path = dir_path.join("extra.txt");
        fs::create_dir_all(&dir_path).unwrap();
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"This is a nested file").unwrap();

        let status = get_repository_status(&tmp_path).unwrap();

        assert_eq!(status, "?? hello.txt\n?? nested/");
        cleanup(&subdir).unwrap();
    }

    #[test]
    fn lists_untracked_files_inside_tracked_directories() {
        let subdir = "lists_untracked_files_inside_tracked_directories";
        let tmp_path = tmp_path(&subdir);

        init(&subdir).unwrap();

        let dir_path = tmp_path.join("a/b/c");
        fs::create_dir_all(&dir_path).unwrap();
        let file_path = tmp_path.join("a/b/hello.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();

        add_files_to_repository(vec![&tmp_path], &tmp_path).unwrap();
        create_commit(Some(String::from("msg")), &tmp_path).unwrap();

        fs::write(tmp_path.join("a/outer.txt"), b"").unwrap();
        fs::write(tmp_path.join("a/b/c/file.txt"), b"").unwrap();

        let status = get_repository_status(&tmp_path).unwrap();

        assert_eq!(status, "?? a/b/c/\n?? a/outer.txt");
        cleanup(&subdir).unwrap();
    }

    #[test]
    fn does_not_list_empty_untracked_directories() {
        let subdir = "does_not_list_empty_untracked_directories";
        let tmp_path = tmp_path(&subdir);
        init(&subdir).unwrap();

        std::fs::create_dir(tmp_path.join("outer")).unwrap();
        let status = get_repository_status(&tmp_path).unwrap();
        assert_eq!(status, "");

        cleanup(&subdir).unwrap();

        init(&subdir).unwrap();
    }

    #[test]
    fn lists_untracked_directories_that_indirectly_contain_files() {
        let subdir = "lists_untracked_directories_that_indirectly_contain_files";
        let tmp_path = tmp_path(&subdir);
        init(&subdir).unwrap();

        std::fs::create_dir_all(tmp_path.join("outer/inner")).unwrap();
        std::fs::write(tmp_path.join("outer/inner/file.txt"), "").unwrap();
        let status = get_repository_status(&tmp_path).unwrap();
        assert_eq!(status, "?? outer/");

        cleanup(&subdir).unwrap();

        init(&subdir).unwrap();
    }
}
