use std::path::PathBuf;

use anyhow::bail;
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};

use crate::discovery::ConversionJob;

/// Color type of a PNG image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorType {
    Grayscale,
    Rgb,
    GrayscaleAlpha,
    Rgba,
}

/// Parsed PNG metadata and concatenated IDAT data.
#[derive(Debug)]
pub struct PngInfo {
    pub width: u32,
    pub height: u32,
    pub color_type: ColorType,
    pub bit_depth: u8,
    pub num_channels: u8,
    pub idat_data: Vec<u8>,
}

/// Parse a PNG file from raw bytes, extracting IHDR metadata and concatenated IDAT stream.
pub fn parse_png(data: &[u8]) -> anyhow::Result<PngInfo> {
    const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

    // Validate PNG signature
    if data.len() < 8 || data[..8] != PNG_SIGNATURE {
        bail!("Invalid PNG signature");
    }

    let mut offset: usize = 8;

    // Parse first chunk — must be IHDR
    if data.len() < offset + 8 {
        bail!("Truncated PNG: missing IHDR chunk header");
    }
    let ihdr_length = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]) as usize;
    let ihdr_type = &data[offset + 4..offset + 8];
    offset += 8;

    if ihdr_type != b"IHDR" {
        bail!("First chunk is not IHDR");
    }
    if ihdr_length != 13 {
        bail!("IHDR chunk has invalid length: {}", ihdr_length);
    }
    if data.len() < offset + 13 {
        bail!("Truncated PNG: incomplete IHDR data");
    }

    let width = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]);
    let height = u32::from_be_bytes([
        data[offset + 4],
        data[offset + 5],
        data[offset + 6],
        data[offset + 7],
    ]);
    let bit_depth = data[offset + 8];
    let color_type_byte = data[offset + 9];
    let compression = data[offset + 10];
    let filter = data[offset + 11];
    let interlace = data[offset + 12];
    offset += 13;

    if compression != 0 {
        bail!("Unsupported compression method: {}", compression);
    }
    if filter != 0 {
        bail!("Unsupported filter method: {}", filter);
    }
    if interlace != 0 {
        bail!("Interlaced PNGs not supported");
    }

    let color_type = match color_type_byte {
        0 => ColorType::Grayscale,
        2 => ColorType::Rgb,
        4 => ColorType::GrayscaleAlpha,
        6 => ColorType::Rgba,
        _ => bail!("Unsupported color type: {}", color_type_byte),
    };

    let num_channels = match color_type {
        ColorType::Grayscale => 1,
        ColorType::Rgb => 3,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Rgba => 4,
    };

    // Skip IHDR CRC (4 bytes)
    if data.len() < offset + 4 {
        bail!("Truncated PNG: missing IHDR CRC");
    }
    offset += 4;

    // Iterate remaining chunks, collecting IDAT data
    let mut idat_data = Vec::new();

    loop {
        if data.len() < offset + 8 {
            bail!("Truncated PNG: incomplete chunk header");
        }

        let chunk_length = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        let chunk_type = &data[offset + 4..offset + 8];
        offset += 8;

        if chunk_type == b"IDAT" {
            if data.len() < offset + chunk_length {
                bail!("Truncated PNG: incomplete IDAT data");
            }
            idat_data.extend_from_slice(&data[offset..offset + chunk_length]);
            offset += chunk_length;
            // Skip CRC
            if data.len() < offset + 4 {
                bail!("Truncated PNG: missing IDAT CRC");
            }
            offset += 4;
        } else if chunk_type == b"IEND" {
            break;
        } else {
            // Skip unknown chunk data + CRC
            if data.len() < offset + chunk_length + 4 {
                bail!("Truncated PNG: incomplete chunk");
            }
            offset += chunk_length + 4;
        }
    }

    if idat_data.is_empty() {
        bail!("No IDAT chunks found");
    }

    Ok(PngInfo {
        width,
        height,
        color_type,
        bit_depth,
        num_channels,
        idat_data,
    })
}

/// Outcome of a single file conversion.
#[derive(Debug)]
pub enum Outcome {
    Success,
    Skipped,
    Failed { error_message: String },
}

/// Result of converting a single PNG file to PDF.
#[derive(Debug)]
pub struct ConversionResult {
    pub relative_path: PathBuf,
    pub outcome: Outcome,
}

/// Generate a PDF document from parsed PNG metadata and IDAT data.
///
/// The PDF embeds the PNG image data using FlateDecode with PNG predictor (15),
/// mapping 1 pixel = 1 point for page dimensions.
pub fn write_pdf(info: &PngInfo) -> Vec<u8> {
    let catalog_id = Ref::new(1);
    let page_tree_id = Ref::new(2);
    let page_id = Ref::new(3);
    let image_id = Ref::new(4);
    let content_id = Ref::new(5);

    let mut pdf = Pdf::new();

    // Catalog
    pdf.catalog(catalog_id).pages(page_tree_id);

    // Page tree
    pdf.pages(page_tree_id).kids([page_id]).count(1);

    // Page
    let width_f = info.width as f32;
    let height_f = info.height as f32;
    let mut page = pdf.page(page_id);
    page.parent(page_tree_id)
        .media_box(Rect::new(0.0, 0.0, width_f, height_f))
        .contents(content_id);
    page.resources().x_objects().pair(Name(b"Im1"), image_id);
    page.finish();

    // Image XObject
    let mut image = pdf.image_xobject(image_id, &info.idat_data);
    image.width(info.width as i32);
    image.height(info.height as i32);
    image.bits_per_component(info.bit_depth as i32);

    match info.color_type {
        ColorType::Grayscale | ColorType::GrayscaleAlpha => {
            image.color_space().device_gray();
        }
        ColorType::Rgb | ColorType::Rgba => {
            image.color_space().device_rgb();
        }
    }

    image.filter(Filter::FlateDecode);
    image
        .decode_parms()
        .predictor(pdf_writer::types::Predictor::PngOptimum)
        .colors(info.num_channels as i32)
        .bits_per_component(info.bit_depth as i32)
        .columns(info.width as i32);
    image.finish();

    // Content stream: draw image at full page size
    let mut content = Content::new();
    content.save_state();
    content.transform([width_f, 0.0, 0.0, height_f, 0.0, 0.0]);
    content.x_object(Name(b"Im1"));
    content.restore_state();
    let content_data = content.finish();
    pdf.stream(content_id, &content_data);

    pdf.finish()
}

/// Convert a single PNG file to PDF.
///
/// Reads the input PNG, parses it, generates a PDF, creates output directories
/// if needed, and writes the PDF. Returns a ConversionResult indicating success,
/// skip, or failure (never panics).
pub fn convert_single(job: &ConversionJob, no_overwrite: bool) -> ConversionResult {
    let relative_path = job.relative_path.clone();

    // Check no-overwrite condition
    if no_overwrite && job.output_path.exists() {
        return ConversionResult {
            relative_path,
            outcome: Outcome::Skipped,
        };
    }

    // Read input file
    let bytes = match std::fs::read(&job.input_path) {
        Ok(b) => b,
        Err(e) => {
            return ConversionResult {
                relative_path,
                outcome: Outcome::Failed {
                    error_message: format!("Failed to read input file: {}", e),
                },
            };
        }
    };

    // Parse PNG
    let info = match parse_png(&bytes) {
        Ok(i) => i,
        Err(e) => {
            return ConversionResult {
                relative_path,
                outcome: Outcome::Failed {
                    error_message: format!("Failed to parse PNG: {}", e),
                },
            };
        }
    };

    // Generate PDF
    let pdf_bytes = write_pdf(&info);

    // Create output parent directories
    if let Some(parent) = job.output_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return ConversionResult {
            relative_path,
            outcome: Outcome::Failed {
                error_message: format!("Failed to create output directories: {}", e),
            },
        };
    }

    // Write PDF file
    if let Err(e) = std::fs::write(&job.output_path, pdf_bytes) {
        return ConversionResult {
            relative_path,
            outcome: Outcome::Failed {
                error_message: format!("Failed to write output file: {}", e),
            },
        };
    }

    ConversionResult {
        relative_path,
        outcome: Outcome::Success,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

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

    /// Construct a minimal valid PNG programmatically.
    ///
    /// Generates scanline data (filter byte 0 + zero pixel bytes per row),
    /// compresses with zlib, and wraps in proper PNG structure.
    fn make_minimal_png(width: u32, height: u32, color_type: u8, interlaced: bool) -> Vec<u8> {
        let num_channels: u32 = match color_type {
            0 => 1, // Grayscale
            2 => 3, // RGB
            4 => 2, // GrayscaleAlpha
            6 => 4, // RGBA
            _ => panic!("Unsupported color_type for test helper"),
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
        ihdr_data.push(if interlaced { 1 } else { 0 });

        // Build raw scanline data: for each row, filter byte (0=None) + pixel bytes (all zeros)
        let bytes_per_row = 1 + (width * num_channels) as usize; // 1 filter byte + pixel data
        let mut raw_data = Vec::with_capacity(bytes_per_row * height as usize);
        for _ in 0..height {
            raw_data.push(0u8); // filter byte: None
            raw_data.extend(std::iter::repeat(0u8).take((width * num_channels) as usize));
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

    #[test]
    fn test_parses_valid_rgb_png() {
        let data = make_minimal_png(16, 16, 2, false);
        let info = parse_png(&data).expect("should parse valid RGB PNG");
        assert_eq!(info.width, 16);
        assert_eq!(info.height, 16);
        assert_eq!(info.color_type, ColorType::Rgb);
        assert_eq!(info.bit_depth, 8);
        assert_eq!(info.num_channels, 3);
        assert!(!info.idat_data.is_empty());
    }

    #[test]
    fn test_parses_rgba_png() {
        let data = make_minimal_png(8, 8, 6, false);
        let info = parse_png(&data).expect("should parse valid RGBA PNG");
        assert_eq!(info.width, 8);
        assert_eq!(info.height, 8);
        assert_eq!(info.color_type, ColorType::Rgba);
        assert_eq!(info.num_channels, 4);
        assert!(!info.idat_data.is_empty());
    }

    #[test]
    fn test_parses_grayscale_png() {
        let data = make_minimal_png(4, 4, 0, false);
        let info = parse_png(&data).expect("should parse valid grayscale PNG");
        assert_eq!(info.width, 4);
        assert_eq!(info.height, 4);
        assert_eq!(info.color_type, ColorType::Grayscale);
        assert_eq!(info.num_channels, 1);
        assert!(!info.idat_data.is_empty());
    }

    #[test]
    fn test_concatenates_multiple_idat_chunks() {
        // Build a PNG with two separate IDAT chunks
        let num_channels: u32 = 3; // RGB
        let width: u32 = 4;
        let height: u32 = 4;

        // IHDR
        let mut ihdr_data = Vec::new();
        ihdr_data.extend_from_slice(&width.to_be_bytes());
        ihdr_data.extend_from_slice(&height.to_be_bytes());
        ihdr_data.push(8); // bit_depth
        ihdr_data.push(2); // color_type = RGB
        ihdr_data.push(0); // compression
        ihdr_data.push(0); // filter
        ihdr_data.push(0); // interlace

        // Generate raw scanline data and compress
        let mut raw_data = Vec::new();
        for _ in 0..height {
            raw_data.push(0u8);
            raw_data.extend(std::iter::repeat(0u8).take((width * num_channels) as usize));
        }
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&raw_data).unwrap();
        let compressed = encoder.finish().unwrap();

        // Split compressed data into two chunks
        let mid = compressed.len() / 2;
        let chunk1_data = &compressed[..mid];
        let chunk2_data = &compressed[mid..];

        // Assemble PNG with two IDAT chunks
        let mut png = Vec::new();
        png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        png.extend_from_slice(&make_chunk(b"IHDR", &ihdr_data));
        png.extend_from_slice(&make_chunk(b"IDAT", chunk1_data));
        png.extend_from_slice(&make_chunk(b"IDAT", chunk2_data));
        png.extend_from_slice(&make_chunk(b"IEND", &[]));

        let info = parse_png(&png).expect("should parse PNG with multiple IDAT chunks");
        assert_eq!(info.idat_data, compressed);
    }

    #[test]
    fn test_rejects_interlaced_png() {
        let data = make_minimal_png(8, 8, 2, true);
        let err = parse_png(&data).unwrap_err();
        assert!(
            err.to_string().contains("Interlaced"),
            "Error should mention interlacing, got: {}",
            err
        );
    }

    #[test]
    fn test_rejects_invalid_png() {
        let data = vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let result = parse_png(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_rejects_truncated_png() {
        let valid = make_minimal_png(8, 8, 2, false);
        // Take only the first 20 bytes (signature + partial IHDR)
        let truncated = &valid[..20];
        let result = parse_png(truncated);
        assert!(result.is_err());
    }

    #[test]
    fn test_rejects_missing_idat() {
        // PNG with IHDR + IEND but no IDAT
        let mut ihdr_data = Vec::new();
        ihdr_data.extend_from_slice(&8u32.to_be_bytes()); // width
        ihdr_data.extend_from_slice(&8u32.to_be_bytes()); // height
        ihdr_data.push(8); // bit_depth
        ihdr_data.push(2); // color_type = RGB
        ihdr_data.push(0); // compression
        ihdr_data.push(0); // filter
        ihdr_data.push(0); // interlace

        let mut png = Vec::new();
        png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        png.extend_from_slice(&make_chunk(b"IHDR", &ihdr_data));
        png.extend_from_slice(&make_chunk(b"IEND", &[]));

        let err = parse_png(&png).unwrap_err();
        assert!(
            err.to_string().contains("No IDAT chunks found"),
            "Error should mention missing IDAT, got: {}",
            err
        );
    }

    #[test]
    fn test_produces_valid_pdf() {
        let png_data = make_minimal_png(16, 16, 2, false);
        let info = parse_png(&png_data).expect("should parse test PNG");
        let pdf_bytes = write_pdf(&info);
        assert!(
            pdf_bytes.starts_with(b"%PDF-"),
            "PDF output should start with %PDF- header"
        );
    }

    #[test]
    fn test_page_dimensions_match() {
        let png_data = make_minimal_png(100, 200, 2, false);
        let info = parse_png(&png_data).expect("should parse test PNG");
        let pdf_bytes = write_pdf(&info);
        let pdf_str = String::from_utf8_lossy(&pdf_bytes);
        // MediaBox should contain the width (100) and height (200)
        assert!(
            pdf_str.contains("100") && pdf_str.contains("200"),
            "PDF should contain page dimensions 100 and 200 in MediaBox"
        );
        // More specifically, look for the MediaBox pattern
        assert!(
            pdf_str.contains("/MediaBox [0 0 100 200]"),
            "PDF should have MediaBox [0 0 100 200], got: {}",
            pdf_str
                .lines()
                .find(|l| l.contains("MediaBox"))
                .unwrap_or("(no MediaBox found)")
        );
    }

    #[test]
    fn test_creates_output_directories() {
        use crate::discovery::ConversionJob;
        use std::path::PathBuf;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let input_dir = tmp.path().join("input");
        std::fs::create_dir_all(&input_dir).unwrap();

        // Write a valid PNG file
        let png_data = make_minimal_png(8, 8, 2, false);
        let input_path = input_dir.join("test.png");
        std::fs::write(&input_path, &png_data).unwrap();

        // Output in a non-existent subdirectory
        let output_path = tmp.path().join("output/sub1/sub2/test.pdf");

        let job = ConversionJob {
            input_path,
            output_path: output_path.clone(),
            relative_path: PathBuf::from("sub1/sub2/test.pdf"),
        };

        let result = convert_single(&job, false);
        assert!(
            matches!(result.outcome, Outcome::Success),
            "Expected Success, got: {:?}",
            result.outcome
        );
        assert!(output_path.exists(), "Output PDF file should exist");
    }

    #[test]
    fn test_skips_existing_when_no_overwrite() {
        use crate::discovery::ConversionJob;
        use std::path::PathBuf;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();

        // Write a valid PNG file
        let png_data = make_minimal_png(8, 8, 2, false);
        let input_path = tmp.path().join("test.png");
        std::fs::write(&input_path, &png_data).unwrap();

        // Create existing output file
        let output_path = tmp.path().join("test.pdf");
        std::fs::write(&output_path, b"existing content").unwrap();

        let job = ConversionJob {
            input_path,
            output_path,
            relative_path: PathBuf::from("test.pdf"),
        };

        let result = convert_single(&job, true);
        assert!(
            matches!(result.outcome, Outcome::Skipped),
            "Expected Skipped, got: {:?}",
            result.outcome
        );
    }

    #[test]
    fn test_overwrites_existing_by_default() {
        use crate::discovery::ConversionJob;
        use std::path::PathBuf;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();

        // Write a valid PNG file
        let png_data = make_minimal_png(8, 8, 2, false);
        let input_path = tmp.path().join("test.png");
        std::fs::write(&input_path, &png_data).unwrap();

        // Create existing output file with dummy content
        let output_path = tmp.path().join("test.pdf");
        std::fs::write(&output_path, b"dummy content").unwrap();

        let job = ConversionJob {
            input_path,
            output_path: output_path.clone(),
            relative_path: PathBuf::from("test.pdf"),
        };

        let result = convert_single(&job, false);
        assert!(
            matches!(result.outcome, Outcome::Success),
            "Expected Success, got: {:?}",
            result.outcome
        );

        // Verify the file content changed (should now be a valid PDF)
        let content = std::fs::read(&output_path).unwrap();
        assert!(
            content.starts_with(b"%PDF-"),
            "Output file should now contain a valid PDF"
        );
    }

    #[test]
    fn test_returns_failed_on_bad_input() {
        use crate::discovery::ConversionJob;
        use std::path::PathBuf;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();

        // Write a non-PNG file with .png extension
        let input_path = tmp.path().join("bad.png");
        std::fs::write(&input_path, b"this is not a PNG file").unwrap();

        let output_path = tmp.path().join("bad.pdf");

        let job = ConversionJob {
            input_path,
            output_path,
            relative_path: PathBuf::from("bad.pdf"),
        };

        let result = convert_single(&job, false);
        assert!(
            matches!(result.outcome, Outcome::Failed { .. }),
            "Expected Failed, got: {:?}",
            result.outcome
        );
    }
}
