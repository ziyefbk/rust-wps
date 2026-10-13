"""Reproduce the self-authored binary DOC parser regression fixtures.

These minimal containers exercise this repository's DOC reader; they are not
reference documents produced or validated by Microsoft Word or LibreOffice.
Use --check to verify committed fixture bytes without writing anything.
"""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path
import struct


def put_u16(data: bytearray | memoryview, offset: int, value: int) -> None:
    struct.pack_into("<H", data, offset, value)


def put_u32(data: bytearray | memoryview, offset: int, value: int) -> None:
    struct.pack_into("<I", data, offset, value)


def make_doc(raw: str, papx_end_cp: int | None = None) -> bytes:
    """One UTF-16 piece, one PAPX run, one section, no additional stories."""
    free, end, fat = 0xFFFFFFFF, 0xFFFFFFFE, 0xFFFFFFFD
    # Header; directory sector 0; FAT sector 1; 8 sectors each for WordDocument
    # and 0Table. Streams use regular FAT chains, avoiding a mini-stream.
    data = bytearray(19 * 512)
    data[:8] = bytes.fromhex("D0CF11E0A1B11AE1")
    for offset, value in [(0x18, 0x3E), (0x1A, 3), (0x1C, 0xFFFE), (0x1E, 9), (0x20, 6)]:
        put_u16(data, offset, value)
    for offset, value in [(0x2C, 1), (0x30, 0), (0x38, 4096), (0x3C, end), (0x44, end), (0x4C, 1)]:
        put_u32(data, offset, value)
    for index in range(1, 109):
        put_u32(data, 0x4C + 4 * index, free)

    def directory_entry(index: int, name: str, kind: int, start: int, size: int,
                        child: int = free, left: int = free, color: int = 1) -> None:
        offset = 512 + 128 * index
        encoded = name.encode("utf-16-le")
        data[offset:offset + len(encoded)] = encoded
        put_u16(data, offset + 0x40, len(encoded) + 2)
        data[offset + 0x42] = kind
        data[offset + 0x43] = color
        for relative, value in [(0x44, left), (0x48, free), (0x4C, child), (0x74, start), (0x78, size)]:
            put_u32(data, offset + relative, value)

    directory_entry(0, "Root Entry", 5, end, 0, child=1)
    directory_entry(1, "WordDocument", 2, 2, 4096, left=2)
    directory_entry(2, "0Table", 2, 10, 4096, color=0)
    for index in range(128):
        put_u32(data, 1024 + 4 * index, free)
    put_u32(data, 1024, end)
    put_u32(data, 1028, fat)
    for start in [2, 10]:
        for index in range(start, start + 8):
            put_u32(data, 1024 + 4 * index, index + 1 if index < start + 7 else end)

    word = memoryview(data)[1536:5632]
    table = memoryview(data)[5632:9728]
    encoded = raw.encode("utf-16-le")
    text_len = len(encoded) // 2
    if papx_end_cp is None:
        papx_end_cp = text_len
    assert 0 < papx_end_cp <= text_len
    assert len(encoded) < 1024
    for offset, value in [(0, 0xA5EC), (2, 0xC1), (6, 0x409), (0x0A, 4),
                          (0x0C, 0xBF), (0x20, 14), (0x3E, 22), (0x98, 93)]:
        put_u16(word, offset, value)
    for offset, value in [(0x18, 1024), (0x1C, 1024 + len(encoded)), (0x40, 4096),
                          (0x4C, text_len), (0x1A2, 64), (0x1A6, 21),
                          (0x102, 100), (0x106, 12), (0xCA, 128), (0xCE, 20)]:
        put_u32(word, offset, value)
    # CLX: a single uncompressed PCD covers all main text.
    table[64] = 2
    for offset, value in [(65, 16), (69, 0), (73, text_len), (79, 1024)]:
        put_u32(table, offset, value)
    # PlcfBtePapx points to FKP page 4; PlcfSed has only the final sentinel.
    for offset, value in [(100, 1024), (104, 1024 + len(encoded)), (108, 4),
                          (128, 0), (132, text_len)]:
        put_u32(table, offset, value)
    word[1024:1024 + len(encoded)] = encoded
    put_u32(word, 2048, 1024)
    put_u32(word, 2052, 1024 + 2 * papx_end_cp)
    word[2559] = 1
    return bytes(data)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    destination = Path(__file__).resolve().parents[1] / "tests" / "fixtures" / "doc-pagination"
    fixtures = {
        "blank.doc": make_doc("hello.\r\r\x0C\rworld.\r"),
        "partial.doc": make_doc("A\rB\x0CC\r", papx_end_cp=2),
    }
    if not args.check:
        destination.mkdir(parents=True, exist_ok=True)
    for name, data in fixtures.items():
        path = destination / name
        if args.check:
            if path.read_bytes() != data:
                raise SystemExit(f"Fixture differs from generator: {path}")
        else:
            path.write_bytes(data)
        print(f"{name}: {len(data)} bytes, sha256 {hashlib.sha256(data).hexdigest()}")


if __name__ == "__main__":
    main()
