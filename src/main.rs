use anyhow::Context;
use chrono::Utc;
use nit::{database::Database, Author, Commit, Tree};
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
                "Created empty Nit repository in {}",
                git_path.to_str().unwrap_or("Unknown")
            );
        }
        Opt::Commit {} => {
            let root_path = std::env::current_dir()?;
            let git_path = root_path.join(".git");
            let db_path = git_path.join("objects");

            let ws = Workspace::new(root_path);
            let db = Database::new(db_path);

            let entries = ws
                .list_files()?
                .iter()
                .map(|path| {
                    let data = ws
                        .read_file(&path)
                        .with_context(|| format!("Couldn't load data from {:?}", &path))?;
                    let mut blob = Blob::new(data);
                    db.store(&mut blob)
                        .with_context(|| "Could not store blob")?;

                    Ok(Entry::new(path, blob.oid().unwrap().clone()))
                })
                .collect::<anyhow::Result<Vec<Entry>>>()?;

            let mut tree = Tree::new(entries);
            db.store(&mut tree)?;

            let name = env::var("GIT_AUTHOR_NAME")
                .context("Could not load GIT_AUTHOR_NAME environment variable")?;
            let email = env::var("GIT_AUTHOR_EMAIL")
                .context("Could not load GIT_AUTHOR_EMAIL environment variable")?;

            let author = Author::new(name, email, Utc::now());

            let mut msg = Vec::new();
            std::io::stdin().read_to_end(&mut msg)?;
            let msg = String::from_utf8(msg)?;

            let mut commit = Commit::new(tree.oid().unwrap().clone(), author, msg);
            db.store(&mut commit)?;
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
    Commit,
}
