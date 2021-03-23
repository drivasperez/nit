use anyhow::anyhow;
use anyhow::Context;
use chrono::Utc;
use nit::{database::Database, refs::Refs, Author, Commit, Tree};
use nit::{Blob, Entry, Workspace};
use std::fs;
use std::path::Path;
use std::{env, io::Read};
use structopt::StructOpt;

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    match opt {
        Opt::Init { path } => {
            let root_path = fs::canonicalize(Path::new(&path))?;
            let git_path = root_path.join(".git");
            for &dir in ["objects", "refs"].iter() {
                fs::create_dir_all(git_path.join(dir))?;
            }

            println!(
                "Initialised empty Nit repository in {}",
                git_path.to_str().unwrap_or("Unknown")
            );
        }
        Opt::Commit { message } => {
            let root_path = std::env::current_dir()?;
            let git_path = root_path.join(".git");
            let db_path = git_path.join("objects");

            let ws = Workspace::new(root_path);
            let db = Database::new(db_path);
            let refs = Refs::new(&git_path);

            let entries = ws
                .list_files()?
                .iter()
                .map(|path| {
                    let data = ws
                        .read_file(&path)
                        .with_context(|| format!("Couldn't load data from {:?}", &path))?;
                    let blob = Blob::new(data);
                    let blob = db.store(blob).with_context(|| "Could not store blob")?;

                    let mode = ws.stat_file(&path)?;

                    Ok(Entry::new(path, blob.oid().clone(), mode))
                })
                .collect::<anyhow::Result<Vec<Entry>>>()?;

            let tree = Tree::new(entries);
            let tree = db.store(tree)?;

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
                .ok_or(anyhow!("No commit message, aborting"))?;

            let commit = Commit::new(parent.as_deref(), tree.oid().clone(), author, msg);
            let commit = db.store(commit)?;

            refs.update_head(commit.oid())?;

            let root_msg = match parent {
                Some(_) => "(root-commit) ",
                None => "",
            };

            println!(
                "[{}{}] {}",
                root_msg,
                commit.oid(),
                commit.message().lines().next().unwrap_or("")
            );
        }
    };

    Ok(())
}

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
}
