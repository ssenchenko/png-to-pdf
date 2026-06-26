#!/usr/bin/env python3
"""Benchmark PyMuPDF PNG-to-PDF conversion for comparison with png-pdf."""

import os
import sys
import time

try:
    import fitz
except ImportError:
    print("PyMuPDF not installed. Run: pip install pymupdf")
    sys.exit(1)


def main():
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <input-dir> <output-dir>")
        sys.exit(1)

    input_dir = sys.argv[1]
    output_dir = sys.argv[2]

    if not os.path.isdir(input_dir):
        print(f"Error: {input_dir} is not a directory")
        sys.exit(1)

    os.makedirs(output_dir, exist_ok=True)

    files = [f for f in sorted(os.listdir(input_dir)) if f.lower().endswith(".png")]
    print(f"Converting {len(files)} files with PyMuPDF {fitz.__version__}...")

    start = time.perf_counter()
    for f in files:
        input_path = os.path.join(input_dir, f)
        output_path = os.path.join(output_dir, os.path.splitext(f)[0] + ".pdf")

        img = fitz.open(input_path)
        pdf_bytes = img.convert_to_pdf()
        img.close()

        with open(output_path, "wb") as out:
            out.write(pdf_bytes)

    elapsed = time.perf_counter() - start

    output_size = sum(
        os.path.getsize(os.path.join(output_dir, f)) for f in os.listdir(output_dir)
    )
    print(f"Done in {elapsed:.3f}s")
    print(f"Files converted: {len(files)}")
    print(f"Output size: {output_size / 1024 / 1024:.1f} MB")


if __name__ == "__main__":
    main()
