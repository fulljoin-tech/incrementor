use std::io;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitOperationError {
    #[error("Git working directory is dirty")]
    Dirty,
    #[error("Unknown git error: {0}")]
    Unknown(#[from] io::Error),
}

/// Minimal git functionality used to tag, commit and check dirty repo
pub(crate) struct Git {
    allow_dirty: bool,
    path: Option<PathBuf>,
}

impl Git {
    /// Returns a new instance
    pub fn new(allow_dirty: bool) -> Result<Self, GitOperationError> {
        Ok(Git {
            allow_dirty,
            path: None,
        })
    }

    /// New at path
    #[cfg(test)]
    pub fn new_with_path(path: &Path, allow_dirty: bool) -> Result<Self, GitOperationError> {
        Ok(Git {
            allow_dirty,
            path: Some(path.to_path_buf()),
        })
    }

    /// Returns true if dirty
    pub fn is_dirty(&self) -> bool {
        self.is_dirty_check().is_err()
    }

    /// Tags the latest commit on the current branch
    pub fn tag(&self, tag: &str, message: &str) -> Result<(), GitOperationError> {
        self.is_dirty_check()?;
        self.create_git_cmd()
            .args(vec!["tag", "-a", tag, "-m", message])
            .output()?;
        Ok(())
    }

    /// Commit all with message
    pub fn commit(&self, message: &str) -> Result<(), GitOperationError> {
        let _res = self
            .create_git_cmd()
            .args(vec!["commit", "-am", message])
            .output()?;
        Ok(())
    }

    /// Create the git command with default arguments.
    fn create_git_cmd(&self) -> Command {
        let mut cmd = Command::new("git");
        if let Some(path) = &self.path {
            cmd.args(["-C", path.to_str().unwrap()]);
        }
        cmd
    }

    /// Returns 'true' when the git working directory is dirty (has changes)
    fn is_dirty_check(&self) -> Result<(), GitOperationError> {
        if self.allow_dirty {
            return Ok(());
        }
        let output = self
            .create_git_cmd()
            .args(["status", "--porcelain"])
            .output()?;

        // Check the command's standard output; if it's empty, there are no changes
        if output.stdout.is_empty() {
            Ok(()) // No changes, working directory is clean
        } else {
            Err(GitOperationError::Dirty) // Changes detected, working directory is dirty
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

        let mut index = repo.index()?;
        create_file_in_repo(repo_path, "TEST", "test")?;
        index.add_path(Path::new("TEST"))?;
        index.write()?;

        let git = Git::new_with_path(repo_path, true)?;
        git.commit("test commit")?;

        // Verify the commit exist
        let commit = repo.head()?.peel_to_commit()?;
        assert_eq!(commit.message(), Some("test commit\n"));

        git.tag("0.2.0", "v0.2.0")?;
        let tags = repo.tag_names(None)?;
        assert!(
            tags.iter().any(|name| name == Some("0.2.0")),
            "The tag was not found in the list."
        );

        Ok(())
    }
}
