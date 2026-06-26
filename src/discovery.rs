use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Represents a single file to be converted from PNG to PDF.
#[derive(Debug, Clone)]
pub struct ConversionJob {
    /// Absolute path to the input PNG file.
    pub input_path: PathBuf,
    /// Absolute path where the output PDF should be written.
    pub output_path: PathBuf,
    /// Path relative to the input directory (used for display and output structure).
    pub relative_path: PathBuf,
}

/// Recursively discovers PNG files in `input_dir` and builds conversion jobs
/// with output paths rooted at `output_dir`.
///
/// Filters out hidden files/directories (names starting with `.`) and non-PNG files.
/// PNG extension matching is case-insensitive.
/// Results are sorted by relative_path for deterministic output.
pub fn discover_jobs(input_dir: &Path, output_dir: &Path) -> anyhow::Result<Vec<ConversionJob>> {
    if !input_dir.exists() {
        anyhow::bail!("Input directory does not exist: {}", input_dir.display());
    }
    if !input_dir.is_dir() {
        anyhow::bail!("Input path is not a directory: {}", input_dir.display());
    }

    let mut jobs = Vec::new();

    let walker = WalkDir::new(input_dir).into_iter();

    for entry in walker.filter_entry(|e| {
        // Skip hidden entries (name starts with '.')
        e.file_name()
            .to_str()
            .map(|name| !name.starts_with('.'))
            .unwrap_or(false)
    }) {
        let entry = entry?;

        // Skip non-files (directories, symlinks, etc.)
        if !entry.file_type().is_file() {
            continue;
        }

        // Match .png extension case-insensitively
        let path = entry.path();
        let is_png = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"));

        if !is_png {
            continue;
        }

        // Compute relative path by stripping input_dir prefix
        let relative_path = path.strip_prefix(input_dir)?.to_path_buf();

        // Build output path: output_dir/relative_path with .pdf extension
        let output_path = output_dir.join(&relative_path).with_extension("pdf");

        jobs.push(ConversionJob {
            input_path: path.to_path_buf(),
            output_path,
            relative_path,
        });
    }

    // Sort by relative_path for deterministic output
    jobs.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    Ok(jobs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_finds_png_files() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("input");
        fs::create_dir_all(&input).unwrap();
        fs::write(input.join("a.png"), b"").unwrap();
        fs::write(input.join("b.png"), b"").unwrap();

        let output = tmp.path().join("output");
        let jobs = discover_jobs(&input, &output).unwrap();
        assert_eq!(jobs.len(), 2);
    }

    #[test]
    fn test_recursive_traversal() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("input");
        fs::create_dir_all(input.join("sub1/sub2")).unwrap();
        fs::write(input.join("top.png"), b"").unwrap();
        fs::write(input.join("sub1/mid.png"), b"").unwrap();
        fs::write(input.join("sub1/sub2/deep.png"), b"").unwrap();

        let output = tmp.path().join("output");
        let jobs = discover_jobs(&input, &output).unwrap();
        assert_eq!(jobs.len(), 3);
    }

    #[test]
    fn test_case_insensitive_extension() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("input");
        fs::create_dir_all(&input).unwrap();
        fs::write(input.join("upper.PNG"), b"").unwrap();
        fs::write(input.join("mixed.Png"), b"").unwrap();
        fs::write(input.join("weird.pNg"), b"").unwrap();

        let output = tmp.path().join("output");
        let jobs = discover_jobs(&input, &output).unwrap();
        assert_eq!(jobs.len(), 3);
    }

    #[test]
    fn test_skips_non_png() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("input");
        fs::create_dir_all(&input).unwrap();
        fs::write(input.join("photo.jpg"), b"").unwrap();
        fs::write(input.join("notes.txt"), b"").unwrap();
        fs::write(input.join("valid.png"), b"").unwrap();

        let output = tmp.path().join("output");
        let jobs = discover_jobs(&input, &output).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].relative_path, PathBuf::from("valid.png"));
    }

    #[test]
    fn test_skips_hidden_files() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("input");
        fs::create_dir_all(input.join(".hidden")).unwrap();
        fs::write(input.join(".hidden/file.png"), b"").unwrap();
        fs::write(input.join(".secret.png"), b"").unwrap();
        fs::write(input.join("visible.png"), b"").unwrap();

        let output = tmp.path().join("output");
        let jobs = discover_jobs(&input, &output).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].relative_path, PathBuf::from("visible.png"));
    }

    #[test]
    fn test_output_path_mapping() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("input");
        fs::create_dir_all(input.join("subdir")).unwrap();
        fs::write(input.join("subdir/image.png"), b"").unwrap();

        let output = tmp.path().join("output");
        let jobs = discover_jobs(&input, &output).unwrap();
        assert_eq!(jobs.len(), 1);

        let job = &jobs[0];
        assert_eq!(job.relative_path, PathBuf::from("subdir/image.png"));
        assert_eq!(job.output_path, output.join("subdir/image.pdf"));
        assert_eq!(job.input_path, input.join("subdir/image.png"));
    }

    #[test]
    fn test_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("input");
        fs::create_dir_all(&input).unwrap();

        let output = tmp.path().join("output");
        let jobs = discover_jobs(&input, &output).unwrap();
        assert!(jobs.is_empty());
    }

    #[test]
    fn test_nonexistent_dir_returns_error() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("does_not_exist");
        let output = tmp.path().join("output");

        let result = discover_jobs(&input, &output);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("does not exist"),
            "Expected 'does not exist' in error message, got: {err_msg}"
        );
    }

    #[test]
    fn test_file_as_input_returns_error() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("a_file.txt");
        fs::write(&input, b"not a directory").unwrap();
        let output = tmp.path().join("output");

        let result = discover_jobs(&input, &output);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not a directory"),
            "Expected 'not a directory' in error message, got: {err_msg}"
        );
    }
}
