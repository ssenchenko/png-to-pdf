#!/usr/bin/env python3
"""Compare png-pdf and PyMuPDF output quality and file sizes.

Requires both output directories to already exist (run the converters first).
"""

import os
import sys

try:
    import fitz
except ImportError:
    print("PyMuPDF not installed. Run: pip install pymupdf")
    sys.exit(1)


def main():
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <png-pdf-output-dir> <pymupdf-output-dir>")
        print()
        print("Run both converters first:")
        print("  png-pdf <input> <png-pdf-output>")
        print("  python bench/bench_pymupdf.py <input> <pymupdf-output>")
        sys.exit(1)

    rust_dir = sys.argv[1]
    pymupdf_dir = sys.argv[2]

    if not os.path.isdir(rust_dir):
        print(f"Error: {rust_dir} not found. Run png-pdf first.")
        sys.exit(1)
    if not os.path.isdir(pymupdf_dir):
        print(f"Error: {pymupdf_dir} not found. Run bench_pymupdf.py first.")
        sys.exit(1)

    rust_files = sorted(f for f in os.listdir(rust_dir) if f.endswith(".pdf"))
    pymupdf_files = sorted(f for f in os.listdir(pymupdf_dir) if f.endswith(".pdf"))

    common = sorted(set(rust_files) & set(pymupdf_files))
    print(f"Comparing {len(common)} common PDF files...\n")

    print(
        f"{'File':<55} {'png-pdf dims':<16} {'PyMuPDF dims':<16} {'png-pdf KB':<12} {'PyMuPDF KB':<12}"
    )
    print("-" * 111)

    rust_total = 0
    pymupdf_total = 0
    image_match_count = 0

    for f in common[:20]:  # show first 20
        rust_path = os.path.join(rust_dir, f)
        pymupdf_path = os.path.join(pymupdf_dir, f)

        rust_size = os.path.getsize(rust_path) / 1024
        pymupdf_size = os.path.getsize(pymupdf_path) / 1024
        rust_total += rust_size
        pymupdf_total += pymupdf_size

        # Get page dimensions
        doc_r = fitz.open(rust_path)
        rect_r = doc_r[0].rect
        doc_r.close()

        doc_p = fitz.open(pymupdf_path)
        rect_p = doc_p[0].rect
        doc_p.close()

        dims_r = f"{rect_r.width:.0f}x{rect_r.height:.0f}"
        dims_p = f"{rect_p.width:.0f}x{rect_p.height:.0f}"

        print(
            f"{f[:53]:<55} {dims_r:<16} {dims_p:<16} {rust_size:<12.0f} {pymupdf_size:<12.0f}"
        )

    # Compare image data for a sample file
    print("\n--- Image data comparison (first 5 files) ---\n")
    for f in common[:5]:
        rust_path = os.path.join(rust_dir, f)
        pymupdf_path = os.path.join(pymupdf_dir, f)

        doc_r = fitz.open(rust_path)
        imgs_r = doc_r[0].get_images(full=True)
        img_r = doc_r.extract_image(imgs_r[0][0]) if imgs_r else None
        doc_r.close()

        doc_p = fitz.open(pymupdf_path)
        imgs_p = doc_p[0].get_images(full=True)
        img_p = doc_p.extract_image(imgs_p[0][0]) if imgs_p else None
        doc_p.close()

        if img_r and img_p:
            match = img_r["image"] == img_p["image"]
            size_r = len(img_r["image"])
            size_p = len(img_p["image"])
            if match:
                image_match_count += 1
            print(
                f"  {f[:50]}: {'IDENTICAL' if match else 'DIFFERENT'} "
                f"(png-pdf: {size_r} bytes, PyMuPDF: {size_p} bytes)"
            )

    # Summary
    rust_total_all = sum(
        os.path.getsize(os.path.join(rust_dir, f)) for f in rust_files
    )
    pymupdf_total_all = sum(
        os.path.getsize(os.path.join(pymupdf_dir, f)) for f in pymupdf_files
    )

    print(f"\n--- Summary ---\n")
    print(f"  png-pdf total output:  {rust_total_all / 1024 / 1024:.1f} MB ({len(rust_files)} files)")
    print(f"  PyMuPDF total output:  {pymupdf_total_all / 1024 / 1024:.1f} MB ({len(pymupdf_files)} files)")
    print(f"  Size ratio:            png-pdf is {rust_total_all / pymupdf_total_all:.2f}x larger")
    print(f"  Image data:            {'All identical' if image_match_count == 5 else f'{image_match_count}/5 matched'} (lossless both)")
    print(f"  Page sizing:           png-pdf uses 1px=1pt, PyMuPDF uses 96 DPI")


if __name__ == "__main__":
    main()
