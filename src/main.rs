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
        Opt::Commit { message } => create_commit(message),
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

fn create_commit(message: Option<String>) -> anyhow::Result<()> {
    let root_path = std::env::current_dir()?;
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
    use std::{fs::File, io::prelude::*, path::PathBuf};

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

        let entries: Vec<_> = repository.index().entries().keys().collect();

        assert_eq!(entries, vec!["hello.txt"]);
        cleanup(&subdir).unwrap();
    }
}
