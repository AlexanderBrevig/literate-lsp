use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Represents a virtual document written to disk
#[derive(Debug, Clone)]
pub struct DiskVirtualDoc {
    pub file_path: PathBuf,
    pub content: String,
    pub language: String,
}

impl DiskVirtualDoc {
    /// Write a virtual document to disk in the configured output directory
    ///
    /// # Arguments
    /// * `project_root` - The root directory of the project
    /// * `output_dir` - Path relative to project_root where files should be written (e.g., "./src")
    /// * `markdown_filename` - Name of the original markdown file (e.g., "book.typ")
    /// * `language` - Programming language of the code block
    /// * `extension` - File extension for this language (e.g., "rs", "py")
    /// * `content` - The concatenated virtual document content
    ///
    /// # Returns
    /// A DiskVirtualDoc with absolute path and file:// URI
    pub fn write_to_disk(
        project_root: &Path,
        output_dir: &str,
        markdown_filename: &str,
        language: &str,
        extension: &str,
        content: String,
    ) -> Result<Self> {
        // Get the basename without extension
        let basename = markdown_filename
            .rsplit_once('.')
            .map(|(name, _)| name)
            .unwrap_or(markdown_filename);

        // Construct output directory path
        let output_path = if output_dir.starts_with('/') {
            // Absolute path
            PathBuf::from(output_dir)
        } else if output_dir.starts_with("./") || output_dir.starts_with("../") {
            // Relative path
            project_root.join(output_dir)
        } else {
            // Treat as relative
            project_root.join(output_dir)
        };

        // Create output directory if it doesn't exist
        std::fs::create_dir_all(&output_path)?;

        // Generate filename: {basename}.{extension}
        let filename = format!("{}.{}", basename, extension);
        let file_path = output_path.join(&filename);

        // Write file to disk
        debug!(
            "[DiskVirtualDoc] Writing {} code to {}",
            language,
            file_path.display()
        );
        std::fs::write(&file_path, &content)?;

        Ok(DiskVirtualDoc {
            file_path,
            content,
            language: language.to_string(),
        })
    }

    /// Convert file path to file:// URI
    pub fn to_uri(&self) -> String {
        format!("file://{}", self.file_path.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_write_to_disk() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        let disk_doc = DiskVirtualDoc::write_to_disk(
            project_root,
            "./src",
            "book.typ",
            "rust",
            "rs",
            "fn main() {}".to_string(),
        )
        .unwrap();

        assert!(disk_doc.file_path.exists());
        assert_eq!(disk_doc.language, "rust");
        assert_eq!(
            std::fs::read_to_string(&disk_doc.file_path).unwrap(),
            "fn main() {}"
        );
    }

    #[test]
    fn test_to_uri() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        let disk_doc = DiskVirtualDoc::write_to_disk(
            project_root,
            "./src",
            "example.md",
            "python",
            "py",
            "print('hello')".to_string(),
        )
        .unwrap();

        let uri = disk_doc.to_uri();
        assert!(uri.starts_with("file://"));
        assert!(uri.contains("example.py"));
    }

    #[test]
    fn test_creates_output_directory() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path();

        DiskVirtualDoc::write_to_disk(
            project_root,
            "./src",
            "test.md",
            "go",
            "go",
            "package main".to_string(),
        )
        .unwrap();

        assert!(project_root.join("src").exists());
    }
}
