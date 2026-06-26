//! Shared test utilities for integration tests.
//!
//! Provides helpers for generating minimal valid PNGs, validating PDFs,
//! and setting up temporary test directories.

use std::io::Write;
use std::path::Path;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use tempfile::TempDir;

/// CRC-32 computation (ISO 3309 / ITU-T V.42 polynomial used by PNG).
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

/// Build a PNG chunk: length (4 BE) + type (4) + data + CRC (4 BE over type+data).
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

/// Build a minimal valid PNG with the given dimensions and color type.
///
/// Generates scanline data (filter byte 0 + zero pixel bytes per row),
/// compresses with zlib, and wraps in proper PNG structure.
///
/// # Arguments
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `color_type` - PNG color type: 0=Grayscale, 2=RGB, 4=GrayscaleAlpha, 6=RGBA
pub fn make_png(width: u32, height: u32, color_type: u8) -> Vec<u8> {
    let num_channels: u32 = match color_type {
        0 => 1, // Grayscale
        2 => 3, // RGB
        4 => 2, // GrayscaleAlpha
        6 => 4, // RGBA
        _ => panic!("Unsupported color_type for test helper: {}", color_type),
    };
    let bit_depth: u8 = 8;

    // Build IHDR data (13 bytes)
    let mut ihdr_data = Vec::new();
    ihdr_data.extend_from_slice(&width.to_be_bytes());
    ihdr_data.extend_from_slice(&height.to_be_bytes());
    ihdr_data.push(bit_depth);
    ihdr_data.push(color_type);
    ihdr_data.push(0); // compression method
    ihdr_data.push(0); // filter method
    ihdr_data.push(0); // interlace = none

    // Build raw scanline data: for each row, filter byte (0=None) + pixel bytes
    let row_bytes = (width * num_channels) as usize;
    let mut raw_data = Vec::with_capacity((1 + row_bytes) * height as usize);
    for _ in 0..height {
        raw_data.push(0u8); // filter byte: None
        raw_data.extend(std::iter::repeat_n(128u8, row_bytes));
    }

    // Compress with zlib
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&raw_data).unwrap();
    let compressed = encoder.finish().unwrap();

    // Assemble PNG
    let mut png = Vec::new();
    // PNG signature
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    // IHDR chunk
    png.extend_from_slice(&make_chunk(b"IHDR", &ihdr_data));
    // IDAT chunk
    png.extend_from_slice(&make_chunk(b"IDAT", &compressed));
    // IEND chunk
    png.extend_from_slice(&make_chunk(b"IEND", &[]));

    png
}

/// Build a minimal valid RGB PNG (color_type = 2).
pub fn make_rgb_png(width: u32, height: u32) -> Vec<u8> {
    make_png(width, height, 2)
}

/// Build a minimal valid RGBA PNG (color_type = 6).
pub fn make_rgba_png(width: u32, height: u32) -> Vec<u8> {
    make_png(width, height, 6)
}

/// Build a minimal valid Grayscale PNG (color_type = 0).
pub fn make_grayscale_png(width: u32, height: u32) -> Vec<u8> {
    make_png(width, height, 0)
}

/// Assert that the given bytes represent a valid PDF document.
///
/// Checks:
/// - Starts with the `%PDF-` magic header
/// - Is non-empty (beyond just the header)
pub fn assert_valid_pdf(data: &[u8]) {
    assert!(!data.is_empty(), "PDF data should not be empty");
    assert!(
        data.starts_with(b"%PDF-"),
        "PDF should start with %PDF- header, got first bytes: {:?}",
        &data[..std::cmp::min(10, data.len())]
    );
}

/// Create a temporary directory populated with the given files.
///
/// Each entry in `files` is a `(relative_path, content)` pair.
/// Parent directories are created as needed.
///
/// Returns the `TempDir` handle — the directory is cleaned up when dropped.
///
/// Note: The returned TempDir may have a name starting with `.tmp` which
/// is filtered by the discovery module's hidden-file logic. When using this
/// as an input directory for `cli::run`, create an `input` subdirectory inside
/// the TempDir instead.
#[allow(dead_code)]
pub fn setup_test_dir(files: &[(&str, &[u8])]) -> TempDir {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    for (path, content) in files {
        let full_path = tmp.path().join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("Failed to create dir {}: {}", parent.display(), e));
        }
        std::fs::write(&full_path, content)
            .unwrap_or_else(|e| panic!("Failed to write {}: {}", full_path.display(), e));
    }
    tmp
}

/// Count PDF files in a directory (non-recursive).
#[allow(dead_code)]
pub fn count_pdfs_in_dir(dir: &Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "pdf"))
        .count()
}

/// Recursively count PDF files in a directory tree.
pub fn count_pdfs_recursive(dir: &Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    let mut count = 0;
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "pdf") {
            count += 1;
        }
    }
    count
}
