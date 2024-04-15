#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use git2::build::CheckoutBuilder;
use git2::{IndexAddOption, Repository, StatusOptions};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitOperationError {
    #[error("Git working directory is dirty")]
    Dirty,
    #[error("Unknown git error: {0}")]
    Unknown(#[from] git2::Error),
}

/// Minimal git functionality used to tag, commit and check dirty repo
pub(crate) struct Git {
    allow_dirty: bool,
    repo: Repository,
}

impl Git {
    /// Returns a new instance
    pub fn new(allow_dirty: bool) -> Result<Self, GitOperationError> {
        let repo = Repository::discover(".")?;
        Ok(Git { repo, allow_dirty })
    }

    /// New at path
    #[cfg(test)]
    pub fn new_with_path(path: &Path, allow_dirty: bool) -> Result<Self, GitOperationError> {
        let repo = Repository::discover(path)?;
        Ok(Git { repo, allow_dirty })
    }

    /// Returns true if dirty
    pub fn is_dirty(&self) -> bool {
        self.is_dirty_check().is_err()
    }

    /// Tags the latest commit on the current branch
    pub fn tag(&self, tag: &str, message: &str) -> Result<(), GitOperationError> {
        self.is_dirty_check()?;

        // Tag the latest commit of the current branch
        let obj = self
            .repo
            .head()?
            .resolve()?
            .peel(git2::ObjectType::Commit)?;
        self.repo
            .tag(tag, &obj, &self.repo.signature()?, message, false)?;
        Ok(())
    }

    /// Commit all with message
    pub fn commit(&self, message: &str) -> Result<(), GitOperationError> {
        let mut index = self.repo.index()?;
        // TODO: here we add all the changes, maybe only add the files changed by the incrementor command?
        index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
        let oid = index.write_tree()?;

        let tree = self.repo.find_tree(oid)?;
        let sig = self.repo.signature()?;
        let parent_commit = self.find_last_commit()?;
        self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent_commit])?;
        // Make sure the tree is written to disk
        tree.write()?;
        Ok(())
    }

    /// Find the last commit of the current branch
    fn find_last_commit(&self) -> Result<git2::Commit, git2::Error> {
        let obj = self
            .repo
            .head()?
            .resolve()?
            .peel(git2::ObjectType::Commit)?;
        obj.into_commit()
            .map_err(|_| git2::Error::from_str("Couldn't find commit"))
    }

    /// Rollback any changes made
    pub fn rollback(&self, files: Vec<&PathBuf>) -> Result<(), GitOperationError> {
        let mut b = CheckoutBuilder::new();
        for path in files {
            b.path(path);
        }

        Ok(self.repo.checkout_head(Some(&mut b.force()))?)
    }

    /// Returns 'true' when the git working directory is dirty (has changes)
    fn is_dirty_check(&self) -> Result<(), GitOperationError> {
        if self.allow_dirty {
            return Ok(());
        }
        let mut opts = StatusOptions::default();
        if self.repo.statuses(Some(&mut opts))?.is_empty() {
            Ok(())
        } else {
            Err(GitOperationError::Dirty)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use eyre::Result;
    use git2::Repository;
    use tempfile::tempdir;

    use crate::git_operations::Git;

    fn create_file_in_repo(repo_path: &Path, file_name: &str, contents: &str) -> Result<()> {
        let file_path = repo_path.join(file_name);
        fs::write(file_path, contents)?;
        Ok(())
    }

    #[test]
    fn test_if_dirty() -> Result<()> {
        let dir = tempdir().unwrap();
        let repo = Repository::init(dir.path())?;
        let repo_path = repo.path().parent().unwrap();
        create_file_in_repo(repo_path, "incrementor.toml", "current_version = '0.1.0'")?;
        create_file_in_repo(repo_path, "VERSION", "0.1.0")?;

        // Add files to the index
        let mut index = repo.index()?;
        index.add_path(Path::new("incrementor.toml"))?;
        index.add_path(Path::new("VERSION"))?;
        index.write()?;

        let git = Git::new_with_path(repo_path, false)?;
        assert!(git.is_dirty());

        let git = Git::new_with_path(repo_path, true)?;
        assert!(!git.is_dirty());

        Ok(())
    }

    #[test]
    fn test_commit_and_tag() -> Result<()> {
        let dir = tempdir().unwrap();
        let repo = Repository::init(dir.path())?;
        let repo_path = repo.path().parent().unwrap();
        create_file_in_repo(repo_path, "incrementor.toml", "current_version = '0.1.0'")?;
        create_file_in_repo(repo_path, "VERSION", "0.1.0")?;

        // Add files to the repo and commit them
        let mut index = repo.index()?;
        index.add_path(Path::new("incrementor.toml"))?;
        index.add_path(Path::new("VERSION"))?;
        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = repo.signature()?;
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Adding VERSION and incrementor.toml",
            &tree,
            &[],
        )?;

        let git = Git::new_with_path(repo_path, false)?;
        git.commit("test commit")?;

        // Verify the commit exist
        let commit = repo.head()?.peel_to_commit()?;
        assert_eq!(commit.message(), Some("test commit"));

        git.tag("0.2.0", "v0.2.0")?;
        let tags = repo.tag_names(None)?;
        assert!(
            tags.iter().any(|name| name == Some("0.2.0")),
            "The tag was not found in the list."
        );

        Ok(())
    }
}
