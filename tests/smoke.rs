use std::fs;

use tempfile::TempDir;

use png_pdf::cli::{Args, run};

/// Build a minimal valid RGB PNG programmatically.
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

    // Build raw scanline data (filter byte + pixel data per row)
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
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]); // PNG signature
    png.extend_from_slice(&make_chunk(b"IHDR", &ihdr_data));
    png.extend_from_slice(&make_chunk(b"IDAT", &compressed));
    png.extend_from_slice(&make_chunk(b"IEND", &[]));

    png
}

#[test]
fn test_smoke_end_to_end() {
    // Set up a temp dir with a single valid PNG
    let tmp = TempDir::new().unwrap();
    let input_dir = tmp.path().join("input");
    let output_dir = tmp.path().join("output");
    fs::create_dir_all(&input_dir).unwrap();

    let png_data = make_test_png();
    fs::write(input_dir.join("image.png"), &png_data).unwrap();

    // Run the full pipeline
    let args = Args {
        input_dir,
        output_dir: output_dir.clone(),
        dry_run: false,
        verbose: false,
        jobs: None,
        no_overwrite: false,
    };

    let exit_code = run(args).unwrap();

    // Assert: returns Ok(0)
    assert_eq!(
        exit_code, 0,
        "Expected exit code 0 for successful conversion"
    );

    // Assert: output dir contains one .pdf file
    let pdf_files: Vec<_> = fs::read_dir(&output_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "pdf"))
        .collect();
    assert_eq!(
        pdf_files.len(),
        1,
        "Expected exactly one PDF file in output directory"
    );

    // Assert: the PDF file starts with %PDF-
    let pdf_path = pdf_files[0].path();
    let pdf_contents = fs::read(&pdf_path).unwrap();
    assert!(
        pdf_contents.starts_with(b"%PDF-"),
        "Output file should be a valid PDF (starts with %PDF-)"
    );
}
