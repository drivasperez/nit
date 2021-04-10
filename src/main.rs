use anyhow::anyhow;
use anyhow::Context;
use chrono::Utc;
use nit::{
    database::{Author, Blob, Commit, Database, Tree},
    index::Index,
    refs::Refs,
    workspace::Workspace,
};
use std::path::Path;
use std::{env, io::Read};
use std::{fs, path::PathBuf, str::FromStr};
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

    match opt {
        Opt::Init { path } => init_repository(path),
        Opt::Add { paths } => add_files_to_repository(paths),
        Opt::Commit { message } => create_commit(message),
    }
}

fn init_repository(path: String) -> anyhow::Result<()> {
    let root_path = fs::canonicalize(Path::new(&path))?;
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

fn add_files_to_repository(paths: Vec<String>) -> anyhow::Result<()> {
    let root_path = std::env::current_dir()?;
    let git_path = root_path.join(".git");
    let ws = Workspace::new(root_path);
    let db = Database::new(git_path.join("objects"));
    let mut index = Index::new(&git_path.join("index"));
    index.load_for_update()?;

    let paths: Result<Vec<_>, anyhow::Error> = paths
        .into_iter()
        .map(|path| {
            let path = PathBuf::from_str(&path)
                .with_context(|| format!("Couldn't add file: {:?}", &path))?;
            let path = std::fs::canonicalize(&path)
                .with_context(|| format!("Couldn't add file: {:?}", &path))?;

            let res = ws
                .list_files(&path)
                .with_context(|| format!("Couldn't add file: {:?}", &path))?;

            Ok(res)
        })
        .collect();

    let paths: Vec<_> = paths?.into_iter().flatten().collect();

    for pathname in paths {
        let (stat, blob_oid) = ws
            .read_file(&pathname)
            .and_then(|data| {
                let stat = ws.stat_file(&pathname)?;
                Ok((data, stat))
            })
            .and_then(|(data, stat)| {
                let blob = Blob::new(data);
                let blob_oid = db.store(&blob)?;

                Ok((stat, blob_oid))
            })
            .with_context(|| format!("Could not read from workspace: {:?}", &pathname))?;

        index.add(pathname, blob_oid, stat);
    }

    index.write_updates()?;

    Ok(())
}

fn create_commit(message: Option<String>) -> anyhow::Result<()> {
    let root_path = std::env::current_dir()?;
    let git_path = root_path.join(".git");
    let db_path = git_path.join("objects");
    let index_path = git_path.join("index");

    let db = Database::new(db_path);
    let mut index = Index::new(index_path);
    let refs = Refs::new(&git_path);

    index.load()?;

    let mut root = Tree::build(index.entries().values().cloned().collect());
    root.traverse(&|tree| {
        let oid = db.store(tree)?;
        Ok(oid)
    })?;

    let root_oid = db.store(&root)?;

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
    let commit_oid = db.store(&commit)?;

    refs.update_head(&commit_oid)?;

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
