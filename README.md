# png-pdf

Batch convert PNG files to PDF. Fast, lossless, parallel.

Each PNG produces a single-page PDF where the page dimensions match the source image (1 pixel = 1 point). Directory structure is preserved in the output.

## Usage

```bash
png-pdf <input-dir> <output-dir>
```

### Options

| Flag | Description |
|------|-------------|
| `-n`, `--dry-run` | List files that would be converted without converting |
| `-v`, `--verbose` | Show per-file status during conversion |
| `-j`, `--jobs <N>` | Number of parallel threads (default: all cores) |
| `--no-overwrite` | Skip files that already exist in output |

### Examples

```bash
# Convert all PNGs in a folder
png-pdf ./scans ./output

# Preview what would be converted
png-pdf --dry-run ./scans ./output

# Verbose output with 4 threads
png-pdf -v -j 4 ./scans ./output
```

## Installation

```bash
cargo install --path .
```

Or run directly:

```bash
cargo run --release -- <input-dir> <output-dir>
```

## Benchmarks

Tested on 91 HVAC schematics (97 MB total, up to 14292×5905 pixels), Apple Silicon M5 Pro:

| Metric | png-pdf | PyMuPDF |
|--------|---------|---------|
| **Speed** | **0.03s** | 12.3s |
| **Speedup** | **~400x** | baseline |
| **Image quality** | Bit-perfect (lossless) | Bit-perfect (lossless) |
| **Output size** | 97 MB | 51 MB |
| **Parallelism** | All cores (rayon) | Single-threaded |
| **Page sizing** | 1px = 1pt | 96 DPI |

**Quality is identical** — both embed lossless PNG data. The extracted image bytes are byte-for-byte the same.

**Speed difference:** png-pdf passes raw compressed IDAT bytes directly into the PDF (zero decode/re-encode), parallelized across all cores. PyMuPDF decodes and re-encodes in single-threaded Python.

**File size difference:** Not image quality — the PNG data is identical. PyMuPDF produces more compact PDF structure (compressed object streams, less metadata overhead). png-pdf wraps each IDAT stream with minimal PDF boilerplate.

## Limitations

- PNG only (no JPEG, TIFF, BMP)
- RGBA and GrayscaleAlpha PNGs are rejected (raw pass-through cannot handle alpha channels correctly in PDF)
- Interlaced PNGs are rejected (PDF FlateDecode with PNG predictor requires non-interlaced scanlines)
- No multi-page merge — each PNG becomes its own PDF
