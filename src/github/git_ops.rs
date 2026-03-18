use std::{path::PathBuf, sync::Arc};

use git2::{
    Cred, FetchOptions, PushOptions, RemoteCallbacks, Repository, Signature,
};
use tracing::info;

use crate::{config::AppConfig, error::Result};

pub struct GitRepo {
    repo: Repository,
    config: Arc<AppConfig>,
}

impl GitRepo {
    /// Opens an existing git repository at `github_repo_path` or clones it
    /// from `github_remote_url` if it doesn't exist.
    pub fn open_or_clone(config: Arc<AppConfig>) -> Result<Self> {
        let path = &config.github_repo_path;
        let repo = if path.join(".git").exists() {
            info!(path = ?path, "Opening existing git repo");
            Repository::open(path)?
        } else {
            info!(path = ?path, url = %config.github_remote_url, "Cloning git repo");
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(|_url, _username, _allowed| {
                Cred::userpass_plaintext(&config.github_username, &config.github_token)
            });
            let mut fetch_opts = FetchOptions::new();
            fetch_opts.remote_callbacks(callbacks);

            let mut builder = git2::build::RepoBuilder::new();
            builder.fetch_options(fetch_opts);
            builder.clone(&config.github_remote_url, path)?
        };

        Ok(Self { repo, config })
    }

    fn make_callbacks(&self) -> RemoteCallbacks<'_> {
        let mut callbacks = RemoteCallbacks::new();
        let username = self.config.github_username.clone();
        let token = self.config.github_token.clone();
        callbacks.credentials(move |_url, _username, _allowed| {
            Cred::userpass_plaintext(&username, &token)
        });
        callbacks
    }

    /// Fetches and fast-forwards `origin/main` (or `origin/master`).
    pub fn pull(&self) -> Result<()> {
        info!("Pulling latest changes from origin");
        let mut remote = self.repo.find_remote("origin")?;

        let callbacks = self.make_callbacks();
        let mut fetch_opts = FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        remote.fetch(&["main", "master"], Some(&mut fetch_opts), None)?;

        // Try to fast-forward to origin/main, fall back to origin/master
        for branch in &["origin/main", "origin/master"] {
            if let Ok(fetch_head) = self.repo.find_reference(&format!("refs/remotes/{}", branch)) {
                let fetch_commit = self.repo.reference_to_annotated_commit(&fetch_head)?;
                let (analysis, _) = self.repo.merge_analysis(&[&fetch_commit])?;

                if analysis.is_up_to_date() {
                    info!("Already up to date");
                    return Ok(());
                } else if analysis.is_fast_forward() {
                    let local_branch = branch.replace("origin/", "refs/heads/");
                    let mut reference = self.repo.find_reference(&local_branch)?;
                    reference.set_target(fetch_commit.id(), "Fast-forward")?;
                    self.repo.set_head(&local_branch)?;
                    self.repo.checkout_head(Some(
                        git2::build::CheckoutBuilder::default().force(),
                    ))?;
                    info!("Fast-forwarded to {}", branch);
                    return Ok(());
                }
                break;
            }
        }

        Ok(())
    }

    /// Stages all changes in the repository working directory.
    pub fn add_all(&self) -> Result<()> {
        let mut index = self.repo.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        Ok(())
    }

    /// Returns true if there are staged changes to commit.
    pub fn has_staged_changes(&self) -> Result<bool> {
        let head = match self.repo.head() {
            Ok(h) => h,
            Err(_) => return Ok(true), // No HEAD means this is first commit
        };
        let head_commit = head.peel_to_commit()?;
        let head_tree = head_commit.tree()?;

        let mut index = self.repo.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        let index_tree_oid = index.write_tree()?;
        let index_tree = self.repo.find_tree(index_tree_oid)?;

        let diff = self
            .repo
            .diff_tree_to_tree(Some(&head_tree), Some(&index_tree), None)?;

        Ok(diff.deltas().count() > 0)
    }

    /// Creates a commit with the given message.
    pub fn commit(&self, message: &str) -> Result<()> {
        let mut index = self.repo.index()?;
        let tree_oid = index.write_tree()?;
        let tree = self.repo.find_tree(tree_oid)?;

        let sig = Signature::now(
            &self.config.github_username,
            &format!("{}@users.noreply.github.com", self.config.github_username),
        )?;

        let parents: Vec<git2::Commit> = match self.repo.head() {
            Ok(head) => vec![head.peel_to_commit()?],
            Err(_) => vec![],
        };
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

        self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)?;

        info!(message, "Created git commit");
        Ok(())
    }

    /// Pushes current HEAD to `origin`.
    pub fn push(&self) -> Result<()> {
        info!("Pushing to origin");
        let mut remote = self.repo.find_remote("origin")?;

        let callbacks = self.make_callbacks();
        let mut push_opts = PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        // Determine current branch
        let head = self.repo.head()?;
        let branch_name = head
            .shorthand()
            .unwrap_or("main");
        let refspec = format!("refs/heads/{0}:refs/heads/{0}", branch_name);

        remote.push(&[&refspec], Some(&mut push_opts))?;
        info!("Push complete");
        Ok(())
    }

    pub fn repo_path(&self) -> PathBuf {
        self.repo.workdir().unwrap_or(self.repo.path()).to_path_buf()
    }
}
