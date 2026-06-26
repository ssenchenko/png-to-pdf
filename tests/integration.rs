//! Integration tests for the full PNG-to-PDF conversion pipeline.
//!
//! These tests exercise the complete flow from CLI args through discovery,
//! conversion, and output file generation.

mod common;

use std::fs;

use png_pdf_converter::cli::{Args, run};
use tempfile::TempDir;

use common::{
    assert_valid_pdf, count_pdfs_recursive, make_grayscale_png, make_rgb_png, make_rgba_png,
};

/// Test: RGB and grayscale PNGs produce valid PDFs; RGBA is rejected (alpha not supported).
#[test]
fn test_full_pipeline_small_batch() {
    let rgb_png = make_rgb_png(16, 16);
    let gray_png = make_grayscale_png(16, 16);

    let tmp = TempDir::new().unwrap();
    let input_dir = tmp.path().join("input");
    let output_dir = tmp.path().join("output");
    fs::create_dir_all(&input_dir).unwrap();

    fs::write(input_dir.join("image_rgb.png"), &rgb_png).unwrap();
    fs::write(input_dir.join("image_gray.png"), &gray_png).unwrap();

    let args = Args {
        input_dir: input_dir.clone(),
        output_dir: output_dir.clone(),
        dry_run: false,
        verbose: false,
        jobs: Some(2),
        no_overwrite: false,
    };

    let exit_code = run(args).unwrap();
    assert_eq!(
        exit_code, 0,
        "Expected exit code 0 for all-successful batch"
    );

    // Verify 2 PDFs were created
    let pdf_count = count_pdfs_recursive(&output_dir);
    assert_eq!(pdf_count, 2, "Expected 2 PDFs in output directory");

    // Verify each PDF is valid
    for name in &["image_rgb.pdf", "image_gray.pdf"] {
        let pdf_path = output_dir.join(name);
        assert!(pdf_path.exists(), "Expected {} to exist", name);
        let pdf_data = fs::read(&pdf_path).unwrap();
        assert_valid_pdf(&pdf_data);
    }
}

/// Test: RGBA PNGs are rejected with a failure (alpha not supported for raw pass-through).
#[test]
fn test_rgba_png_rejected() {
    let rgba_png = make_rgba_png(16, 16);

    let tmp = TempDir::new().unwrap();
    let input_dir = tmp.path().join("input");
    let output_dir = tmp.path().join("output");
    fs::create_dir_all(&input_dir).unwrap();

    fs::write(input_dir.join("alpha.png"), &rgba_png).unwrap();

    let args = Args {
        input_dir: input_dir.clone(),
        output_dir: output_dir.clone(),
        dry_run: false,
        verbose: false,
        jobs: Some(1),
        no_overwrite: false,
    };

    let exit_code = run(args).unwrap();
    assert_eq!(exit_code, 1, "RGBA PNG should cause failure exit code");
    assert!(!output_dir.join("alpha.pdf").exists(), "No PDF should be created for RGBA");
}

/// Test: PNGs in nested directories produce output that mirrors the directory structure.
#[test]
fn test_full_pipeline_nested_dirs() {
    let png_data = make_rgb_png(8, 8);

    let tmp = TempDir::new().unwrap();
    let input_dir = tmp.path().join("input");
    let output_dir = tmp.path().join("output");

    fs::create_dir_all(input_dir.join("sub/deep")).unwrap();
    fs::write(input_dir.join("a.png"), &png_data).unwrap();
    fs::write(input_dir.join("sub/b.png"), &png_data).unwrap();
    fs::write(input_dir.join("sub/deep/c.png"), &png_data).unwrap();

    let args = Args {
        input_dir: input_dir.clone(),
        output_dir: output_dir.clone(),
        dry_run: false,
        verbose: false,
        jobs: Some(1),
        no_overwrite: false,
    };

    let exit_code = run(args).unwrap();
    assert_eq!(exit_code, 0);

    // Verify output mirrors directory structure
    assert!(
        output_dir.join("a.pdf").exists(),
        "Expected a.pdf at root of output"
    );
    assert!(output_dir.join("sub/b.pdf").exists(), "Expected sub/b.pdf");
    assert!(
        output_dir.join("sub/deep/c.pdf").exists(),
        "Expected sub/deep/c.pdf"
    );

    // Verify total count
    let pdf_count = count_pdfs_recursive(&output_dir);
    assert_eq!(pdf_count, 3);
}

/// Test: Only PNG files are converted; non-PNG and hidden files are ignored.
#[test]
fn test_full_pipeline_mixed_files() {
    let png_data = make_rgb_png(8, 8);

    let tmp = TempDir::new().unwrap();
    let input_dir = tmp.path().join("input");
    let output_dir = tmp.path().join("output");
    fs::create_dir_all(&input_dir).unwrap();

    fs::write(input_dir.join("photo1.png"), &png_data).unwrap();
    fs::write(input_dir.join("photo2.png"), &png_data).unwrap();
    fs::write(input_dir.join("file.jpg"), b"fake jpeg data").unwrap();
    fs::write(input_dir.join("notes.txt"), b"some notes").unwrap();
    fs::write(input_dir.join(".hidden.png"), &png_data).unwrap();

    let args = Args {
        input_dir: input_dir.clone(),
        output_dir: output_dir.clone(),
        dry_run: false,
        verbose: false,
        jobs: None,
        no_overwrite: false,
    };

    let exit_code = run(args).unwrap();
    assert_eq!(exit_code, 0);

    // Only 2 PDFs should be created (the two non-hidden .png files)
    let pdf_count = count_pdfs_recursive(&output_dir);
    assert_eq!(
        pdf_count, 2,
        "Expected only 2 PDFs (non-hidden PNGs), got {}",
        pdf_count
    );

    // Verify the correct ones exist
    assert!(output_dir.join("photo1.pdf").exists());
    assert!(output_dir.join("photo2.pdf").exists());

    // Verify non-PNGs were not converted
    assert!(!output_dir.join("file.pdf").exists());
    assert!(!output_dir.join("notes.pdf").exists());
    assert!(!output_dir.join(".hidden.pdf").exists());
}

/// Test: Mix of valid and invalid PNGs — valid ones succeed, invalid one fails, exit code is 1.
#[test]
fn test_full_pipeline_with_failures() {
    let good_png = make_rgb_png(8, 8);

    let tmp = TempDir::new().unwrap();
    let input_dir = tmp.path().join("input");
    let output_dir = tmp.path().join("output");
    fs::create_dir_all(&input_dir).unwrap();

    fs::write(input_dir.join("good1.png"), &good_png).unwrap();
    fs::write(input_dir.join("good2.png"), &good_png).unwrap();
    fs::write(
        input_dir.join("bad.png"),
        b"this is not a valid PNG file at all",
    )
    .unwrap();

    let args = Args {
        input_dir: input_dir.clone(),
        output_dir: output_dir.clone(),
        dry_run: false,
        verbose: true,
        jobs: Some(1),
        no_overwrite: false,
    };

    let exit_code = run(args).unwrap();
    assert_eq!(
        exit_code, 1,
        "Expected exit code 1 when some conversions fail"
    );

    // The 2 valid PNGs should have produced PDFs
    assert!(output_dir.join("good1.pdf").exists());
    assert!(output_dir.join("good2.pdf").exists());

    // The bad PNG should not have produced a PDF
    assert!(!output_dir.join("bad.pdf").exists());

    // Total PDF count should be 2
    let pdf_count = count_pdfs_recursive(&output_dir);
    assert_eq!(pdf_count, 2);
}

/// Test: A large-dimension PNG (10000x5000) produces a PDF with matching page dimensions.
#[test]
fn test_large_image_dimensions() {
    let large_png = make_rgb_png(10000, 5000);

    let tmp = TempDir::new().unwrap();
    let input_dir = tmp.path().join("input");
    let output_dir = tmp.path().join("output");
    fs::create_dir_all(&input_dir).unwrap();

    fs::write(input_dir.join("large.png"), &large_png).unwrap();

    let args = Args {
        input_dir: input_dir.clone(),
        output_dir: output_dir.clone(),
        dry_run: false,
        verbose: false,
        jobs: Some(1),
        no_overwrite: false,
    };

    let exit_code = run(args).unwrap();
    assert_eq!(exit_code, 0);

    let pdf_path = output_dir.join("large.pdf");
    assert!(pdf_path.exists(), "Expected large.pdf to be created");

    let pdf_data = fs::read(&pdf_path).unwrap();
    assert_valid_pdf(&pdf_data);

    // The PDF should contain the dimensions as strings (in the MediaBox)
    let pdf_str = String::from_utf8_lossy(&pdf_data);
    assert!(
        pdf_str.contains("10000"),
        "PDF should contain width 10000 in its structure"
    );
    assert!(
        pdf_str.contains("5000"),
        "PDF should contain height 5000 in its structure"
    );
}

/// Test: The output PDF has the expected internal structure (FlateDecode filter, Predictor).
#[test]
fn test_output_pdf_structure() {
    let png_data = make_rgb_png(32, 32);

    let tmp = TempDir::new().unwrap();
    let input_dir = tmp.path().join("input");
    let output_dir = tmp.path().join("output");
    fs::create_dir_all(&input_dir).unwrap();

    fs::write(input_dir.join("test.png"), &png_data).unwrap();

    let args = Args {
        input_dir: input_dir.clone(),
        output_dir: output_dir.clone(),
        dry_run: false,
        verbose: false,
        jobs: Some(1),
        no_overwrite: false,
    };

    let exit_code = run(args).unwrap();
    assert_eq!(exit_code, 0);

    let pdf_path = output_dir.join("test.pdf");
    let pdf_data = fs::read(&pdf_path).unwrap();
    assert_valid_pdf(&pdf_data);

    let pdf_str = String::from_utf8_lossy(&pdf_data);

    // Verify the PDF uses FlateDecode filter (PNG data passed through without re-compression)
    assert!(
        pdf_str.contains("FlateDecode"),
        "PDF should use FlateDecode filter for the image stream"
    );

    // Verify the PDF uses PNG predictor in DecodeParms
    // Predictor 15 = PNG optimum predictor
    assert!(
        pdf_str.contains("Predictor"),
        "PDF should contain Predictor in DecodeParms for PNG-style filtering"
    );
}
