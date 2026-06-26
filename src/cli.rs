use std::path::PathBuf;

use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};

/// Convert PNG files to PDF, preserving directory structure.
#[derive(Parser, Debug)]
#[command(name = "png-pdf", about = "Batch convert PNG files to PDF")]
pub struct Args {
    /// Input directory containing PNG files
    pub input_dir: PathBuf,

    /// Output directory for generated PDF files
    pub output_dir: PathBuf,

    /// Dry run: list files that would be converted without actually converting
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Verbose output: show details of each file processed
    #[arg(short, long)]
    pub verbose: bool,

    /// Number of parallel jobs (defaults to number of CPU cores)
    #[arg(short, long)]
    pub jobs: Option<usize>,

    /// Do not overwrite existing output files
    #[arg(long)]
    pub no_overwrite: bool,
}

/// Orchestrates the full pipeline: discovery -> conversion -> summary.
/// Returns the process exit code (0 = success, 1 = some failures).
pub fn run(args: Args) -> anyhow::Result<i32> {
    // Discover PNG files
    let jobs = crate::discovery::discover_jobs(&args.input_dir, &args.output_dir)?;

    // If no files found, report and exit
    if jobs.is_empty() {
        eprintln!("No PNG files found in {}", args.input_dir.display());
        return Ok(0);
    }

    eprintln!("Found {} PNG files", jobs.len());

    // Dry run: just list file paths
    if args.dry_run {
        for job in &jobs {
            println!("{}", job.relative_path.display());
        }
        return Ok(0);
    }

    // Create output directory
    std::fs::create_dir_all(&args.output_dir)?;

    // Create progress bar
    let pb = ProgressBar::new(jobs.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .expect("Invalid progress bar template")
            .progress_chars("=> "),
    );

    // Run batch conversion
    let summary = crate::converter::convert_batch(
        &jobs,
        args.no_overwrite,
        args.jobs,
        Some(&pb),
        args.verbose,
    );

    pb.finish_and_clear();

    // Print summary to stderr
    eprintln!(
        "Converted {}/{} files ({} failed, {} skipped) in {:.1}s",
        summary.succeeded,
        summary.total,
        summary.failed,
        summary.skipped,
        summary.elapsed.as_secs_f64()
    );

    // If verbose and there are failures, print each failure detail in the summary
    if args.verbose && !summary.failures.is_empty() {
        eprintln!("\nFailure details:");
        for failure in &summary.failures {
            if let crate::converter::Outcome::Failed { error_message } = &failure.outcome {
                eprintln!(
                    "  FAILED: {} - {}",
                    failure.relative_path.display(),
                    error_message
                );
            }
        }
    }

    // Exit code
    if summary.failed == 0 { Ok(0) } else { Ok(1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // Helper: create a minimal valid PNG for tests (same approach as converter tests)
    fn make_test_png() -> Vec<u8> {
        use flate2::Compression;
        use flate2::write::ZlibEncoder;
        use std::io::Write;

        let width: u32 = 4;
        let height: u32 = 4;
        let num_channels: u32 = 3; // RGB

        // IHDR data
        let mut ihdr_data = Vec::new();
        ihdr_data.extend_from_slice(&width.to_be_bytes());
        ihdr_data.extend_from_slice(&height.to_be_bytes());
        ihdr_data.push(8); // bit_depth
        ihdr_data.push(2); // color_type = RGB
        ihdr_data.push(0); // compression
        ihdr_data.push(0); // filter
        ihdr_data.push(0); // interlace

        // Build raw scanline data
        let mut raw_data = Vec::new();
        for _ in 0..height {
            raw_data.push(0u8); // filter byte: None
            raw_data.extend(std::iter::repeat_n(128u8, (width * num_channels) as usize));
        }

        // Compress with zlib
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&raw_data).unwrap();
        let compressed = encoder.finish().unwrap();

        // CRC-32 computation
        fn crc32(data: &[u8]) -> u32 {
            let mut crc: u32 = 0xFFFF_FFFF;
            for &byte in data {
                crc ^= byte as u32;
                for _ in 0..8 {
                    if crc & 1 != 0 {
                        crc = (crc >> 1) ^ 0xEDB8_8320;
                    } else {
                        crc >>= 1;
                    }
                }
            }
            !crc
        }

        fn make_chunk(chunk_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
            let mut chunk = Vec::new();
            chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
            chunk.extend_from_slice(chunk_type);
            chunk.extend_from_slice(data);
            let mut crc_input = Vec::new();
            crc_input.extend_from_slice(chunk_type);
            crc_input.extend_from_slice(data);
            chunk.extend_from_slice(&crc32(&crc_input).to_be_bytes());
            chunk
        }

        // Assemble PNG
        let mut png = Vec::new();
        png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        png.extend_from_slice(&make_chunk(b"IHDR", &ihdr_data));
        png.extend_from_slice(&make_chunk(b"IDAT", &compressed));
        png.extend_from_slice(&make_chunk(b"IEND", &[]));

        png
    }

    #[test]
    fn test_dry_run_no_output_files() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("input");
        let output = tmp.path().join("output");
        fs::create_dir_all(&input).unwrap();

        // Write valid PNG files
        let png_data = make_test_png();
        fs::write(input.join("a.png"), &png_data).unwrap();
        fs::write(input.join("b.png"), &png_data).unwrap();

        let args = Args {
            input_dir: input,
            output_dir: output.clone(),
            dry_run: true,
            verbose: false,
            jobs: None,
            no_overwrite: false,
        };

        let code = run(args).unwrap();
        assert_eq!(code, 0);

        // No output directory or files should be created
        assert!(
            !output.exists(),
            "Output directory should not be created during dry run"
        );
    }

    #[test]
    fn test_exit_code_zero_on_success() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("input");
        let output = tmp.path().join("output");
        fs::create_dir_all(&input).unwrap();

        // Write valid PNG files
        let png_data = make_test_png();
        fs::write(input.join("a.png"), &png_data).unwrap();
        fs::write(input.join("b.png"), &png_data).unwrap();

        let args = Args {
            input_dir: input,
            output_dir: output.clone(),
            dry_run: false,
            verbose: false,
            jobs: None,
            no_overwrite: false,
        };

        let code = run(args).unwrap();
        assert_eq!(code, 0);

        // Verify PDFs were created
        assert!(output.join("a.pdf").exists());
        assert!(output.join("b.pdf").exists());
    }

    #[test]
    fn test_exit_code_one_on_any_failure() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("input");
        let output = tmp.path().join("output");
        fs::create_dir_all(&input).unwrap();

        // Write one valid PNG and one corrupt file
        let png_data = make_test_png();
        fs::write(input.join("good.png"), &png_data).unwrap();
        fs::write(input.join("bad.png"), b"this is not a PNG").unwrap();

        let args = Args {
            input_dir: input,
            output_dir: output,
            dry_run: false,
            verbose: false,
            jobs: None,
            no_overwrite: false,
        };

        let code = run(args).unwrap();
        assert_eq!(code, 1);
    }

    #[test]
    fn test_creates_output_dir() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("input");
        let output = tmp.path().join("deeply/nested/output");
        fs::create_dir_all(&input).unwrap();

        // Write a valid PNG file
        let png_data = make_test_png();
        fs::write(input.join("test.png"), &png_data).unwrap();

        let args = Args {
            input_dir: input,
            output_dir: output.clone(),
            dry_run: false,
            verbose: false,
            jobs: None,
            no_overwrite: false,
        };

        assert!(!output.exists(), "Output dir should not exist before run");
        let code = run(args).unwrap();
        assert_eq!(code, 0);
        assert!(output.exists(), "Output dir should be created by run");
        assert!(output.join("test.pdf").exists());
    }
}
