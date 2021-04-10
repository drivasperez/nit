use anyhow::anyhow;
use anyhow::Context;
use chrono::Utc;
use nit::{
    database::{Author, Blob, Commit, Tree},
    repository::Repository,
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
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let root_path = std::env::current_dir()?;

    match opt {
        Opt::Init { path } => init_repository(&path.as_ref()),
        Opt::Add { paths } => {
            let paths = paths.iter().map(Path::new).collect();
            add_files_to_repository(paths, &root_path)
        }
        Opt::Commit { message } => create_commit(message, &std::env::current_dir()?),
    }
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
    let mut repo = Repository::new(root_path.join(".git"));

    repo.index()
        .load_for_update()
        .context("Couldn't load for update")?;

    let paths: Result<Vec<_>, anyhow::Error> = paths
        .into_iter()
        .map(|path| {
            let path = std::fs::canonicalize(&path)
                .with_context(|| format!("Couldn't add file: {:?}", &path))?;

            let res = repo
                .workspace()
                .list_files(&path)
                .with_context(|| format!("Couldn't add file: {:?}", &path))?;

            Ok(res)
        })
        .collect();

    let paths: Vec<_> = paths?.into_iter().flatten().collect();

    for pathname in paths {
        let data = repo.workspace().read_file(&pathname).context("No data")?;
        let stat = repo.workspace().stat_file(&pathname).context("No stat")?;
        let blob = Blob::new(data);
        let blob_oid = repo.database().store(&blob).context("No oid")?;

        repo.index().add(pathname, blob_oid, stat);
    }

    repo.index().write_updates()?;

    Ok(())
}

fn create_commit(message: Option<String>, root_path: &Path) -> anyhow::Result<()> {
    let mut repo = Repository::new(root_path.join(".git"));

    repo.index().load()?;

    let mut root = Tree::build(repo.index().entries().values().cloned().collect());
    root.traverse(&mut |tree| {
        let oid = repo.database().store(tree)?;
        Ok(oid)
    })?;

    let root_oid = repo.database().store(&root)?;

    let parent = repo.refs().read_head();
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
    let commit_oid = repo.database().store(&commit)?;

    repo.refs().update_head(&commit_oid)?;

    let root_msg = match parent {
        Some(_) => "",
        None => "(root-commit) ",
    };

    println!(
        "[{}{}] {}",
        root_msg,
        commit_oid,
        commit.message().lines().next().unwrap_or("")
    );

    Ok(())
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
        let mut repository = Repository::new(tmp_path(&subdir).join(".git"));
        let file_path = tmp_path(&subdir).join("hello.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();

        add_files_to_repository(vec![&file_path], &tmp_path(&subdir)).unwrap();

        repository.index().load_for_update().unwrap();

        let entries: Vec<_> = repository
            .index()
            .entries()
            .values()
            .map(|entry| (entry.mode(), entry.path()))
            .collect();

        assert_eq!(
            entries,
            vec![(REGULAR_MODE, &std::ffi::OsString::from("hello.txt"))]
        );
        cleanup(&subdir).unwrap();
    }

    #[test]
    fn adds_an_executable_file_to_the_index() {
        let subdir = "adds_executable";
        init(&subdir).unwrap();
        let mut repository = Repository::new(tmp_path(&subdir).join(".git"));
        let file_path = tmp_path(&subdir).join("hello.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();

        // Set it to executable.
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o755);
        file.set_permissions(permissions).unwrap();

        add_files_to_repository(vec![&file_path], &tmp_path(&subdir)).unwrap();

        repository.index().load_for_update().unwrap();

        let entries: Vec<_> = repository
            .index()
            .entries()
            .values()
            .map(|entry| (entry.mode(), entry.path()))
            .collect();

        assert_eq!(
            entries,
            vec![(EXECUTABLE_MODE, &std::ffi::OsString::from("hello.txt"))]
        );
        cleanup(&subdir).unwrap();
    }

    #[test]
    fn adds_multiple_files_to_index() {
        let subdir = "adds_multiple";
        init(&subdir).unwrap();
        let mut repository = Repository::new(tmp_path(&subdir).join(".git"));

        let file_path = tmp_path(&subdir).join("hello.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();

        let file_path_2 = tmp_path(&subdir).join("hohoho.txt");
        let mut file = File::create(&file_path_2).unwrap();
        file.write_all("Merry christmas!".as_bytes()).unwrap();

        add_files_to_repository(vec![&file_path, &file_path_2], &tmp_path(&subdir)).unwrap();

        repository.index().load_for_update().unwrap();

        let entries: Vec<_> = repository
            .index()
            .entries()
            .values()
            .map(|entry| (entry.mode(), entry.path()))
            .collect();

        assert_eq!(
            entries,
            vec![
                (REGULAR_MODE, &std::ffi::OsString::from("hello.txt")),
                (REGULAR_MODE, &std::ffi::OsString::from("hohoho.txt"))
            ]
        );
        cleanup(&subdir).unwrap();
    }

    #[test]
    fn incrementally_add_files_to_index() {
        let subdir = "adds_incrementally";
        init(&subdir).unwrap();
        let mut repository = Repository::new(tmp_path(&subdir).join(".git"));
        let file_path = tmp_path(&subdir).join("hello.txt");

        let mut file = File::create(&file_path).unwrap();
        file.write_all("Hello, world".as_bytes()).unwrap();
        add_files_to_repository(vec![&file_path], &tmp_path(&subdir)).unwrap();

        repository.index().load_for_update().unwrap();

        let entries: Vec<_> = repository
            .index()
            .entries()
            .values()
            .map(|entry| (entry.mode(), entry.path()))
            .collect();

        assert_eq!(
            entries,
            vec![(REGULAR_MODE, &std::ffi::OsString::from("hello.txt"))]
        );

        // Add another file, reload and reread entries

        let file_path_2 = tmp_path(&subdir).join("hohoho.txt");
        let mut file = File::create(&file_path_2).unwrap();
        file.write_all("Merry christmas!".as_bytes()).unwrap();

        add_files_to_repository(vec![&file_path_2], &tmp_path(&subdir)).unwrap();

        repository.index().load_for_update().unwrap();

        let entries: Vec<_> = repository
            .index()
            .entries()
            .values()
            .map(|entry| (entry.mode(), entry.path()))
            .collect();

        assert_eq!(
            entries,
            vec![
                (REGULAR_MODE, &std::ffi::OsString::from("hello.txt")),
                (REGULAR_MODE, &std::ffi::OsString::from("hohoho.txt"))
            ]
        );

        cleanup(&subdir).unwrap();
    }

    #[test]
    fn adds_a_directory_to_the_index() {
        let subdir = "adds_dir";
        let tmp_path = tmp_path(&subdir);

        init(&subdir).unwrap();
        let mut repository = Repository::new(tmp_path.join(".git"));

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

        repository.index().load_for_update().unwrap();

        let entries: Vec<_> = repository
            .index()
            .entries()
            .values()
            .map(|entry| (entry.mode(), entry.path()))
            .collect();

        assert_eq!(
            entries,
            vec![
                (REGULAR_MODE, &std::ffi::OsString::from("a/b.txt")),
                (REGULAR_MODE, &std::ffi::OsString::from("a/c.txt"))
            ]
        );
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
}
