"""Self-authored structural PPT fixture; not a full application-authored deck."""
from pathlib import Path
import struct
import olefile

ROOT = Path(__file__).resolve().parents[1]
def atom(kind, payload=b"", container=False):
    return struct.pack("<HHI", 15 if container else 0, kind, len(payload)) + payload

stream = atom(1000, container=True) + atom(1006, container=True)
directory = len(stream)
stream += atom(6002, struct.pack("<III", 1 | (2 << 20), 0, 8))
def edit(previous, directory):
    return atom(4085, struct.pack("<7I", 0, 0, previous, directory, 1, 3, 0))
first = len(stream)
stream += edit(0, directory)
latest_slide = len(stream)
stream += atom(1006, container=True)
directory = len(stream)
stream += atom(6002, struct.pack("<II", 2 | (1 << 20), latest_slide))
latest = len(stream)
stream += edit(first, directory)
current = atom(4086, struct.pack("<II I H BB I", 20, 0xE391C05F, latest, 0, 3, 0, 0))

# Four 512-byte sectors: directory, FAT, mini-FAT, root mini-stream.
data = bytearray(512 * 5)
data[:8] = bytes.fromhex("d0cf11e0a1b11ae1")
def put16(offset, value): struct.pack_into("<H", data, offset, value)
def put32(offset, value): struct.pack_into("<I", data, offset, value)
for offset, value in [(24, 0x3e), (26, 3), (28, 0xfffe), (30, 9), (32, 6)]:
    put16(offset, value)
for offset, value in [(44, 1), (48, 0), (56, 4096), (60, 2), (64, 1), (68, 0xfffffffe)]:
    put32(offset, value)
for offset in range(76, 512, 4): put32(offset, 0xffffffff)
put32(76, 1)
def entry(index, name, kind, start, size, child=0xffffffff, right=0xffffffff, color=1):
    base = 512 + index * 128
    encoded = (name + "\0").encode("utf-16le")
    data[base:base+len(encoded)] = encoded
    put16(base+64, len(encoded))
    data[base+66:base+68] = bytes([kind, color])
    for off, value in [(68, 0xffffffff), (72, right), (76, child), (116, start)]:
        put32(base+off, value)
    struct.pack_into("<Q", data, base+120, size)
mini_count = (len(stream)+63)//64
entry(0, "Root Entry", 5, 3, (mini_count+1)*64, child=1)
entry(1, "Current User", 2, mini_count, len(current), right=2)
entry(2, "PowerPoint Document", 2, 0, len(stream), color=0)
for sector in (1, 2):
    for off in range((sector+1)*512, (sector+2)*512, 4): put32(off, 0xffffffff)
for index, value in enumerate([0xfffffffe, 0xfffffffd, 0xfffffffe, 0xfffffffe]):
    put32(1024+index*4, value)
for index in range(mini_count):
    put32(1536+index*4, index+1 if index+1 < mini_count else 0xfffffffe)
put32(1536+mini_count*4, 0xfffffffe)
data[2048:2048+len(stream)] = stream
data[2048+mini_count*64:2048+mini_count*64+len(current)] = current
path = ROOT / "tests/fixtures/ppt-indexed-structure.ppt"
path.write_bytes(data)
with olefile.OleFileIO(path, raise_defects=olefile.DEFECT_INCORRECT) as ole:
    assert ole.openstream("PowerPoint Document").read() == stream
    assert ole.openstream("Current User").read() == current
    assert len(ole.listdir()) == 2
print(f"Verified independent OLE stream bytes: {path} ({len(data)} bytes)")
