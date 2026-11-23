#pragma MIME application/split-second-textures
#pragma description Split/Second .textures (PS3, big-endian) - complete structure

// ---------------------------------------------------------------------------
// ImHex pattern for the PS3 flavour of Split/Second `.textures` files.
//
// Derived from the in-tree PS3 extractor:
//   src/lib.rs
//
// Layout summary (all file offsets are absolute, all integers big-endian):
//   0x00  char   magic[4]                 "TEXS" (PS3)
//   0x04  u32    version                  format version, expected 12
//   0x08  u32    platform                 platform flag, expected 1
//   0x0C  u32    table_size               header spans [0x10, 0x10 + table_size)
//   0x10  struct Ps3Header                record/PT/catalog table (header-relative)
//   0x10 + table_size ..                  per-catalog raw data (embedded slots)
//   ...   struct NameTable                optional, prefixed by "C2NM"
//
// Header-relative means: absolute_offset = 0x10 + stored_offset.
//
// NOTE on entries: each catalog's entry array is a *separate* contiguous block
// located at `BASE_OFFSET + entries_offset` (not inline in the catalog struct).
// In the verified files the entry arrays are laid out back-to-back in catalog
// order, so the pattern flattens them into one array (see `entries` below).
// ---------------------------------------------------------------------------

#include <std/core.pat>
#include <std/io.pat>

// All table offsets are stored relative to the fixed header base, not to 0.
#define BASE_OFFSET 0x10

// ---------------------------------------------------------------------------
// Record (36 bytes / 0x24). Reached through `records_offset` (header-relative).
//
// Unlike the PC record (0x40 bytes), the PS3 record packs format/mip/dimension
// into single bytes and stores dimensions as u16.
//
// `format` codes (console enum, not the DDS FourCC):
//   0x86 = DXT1   0x88 = DXT5   0xA5 = A8R8G8B8 (4 B/pixel)
//   any other non-ARGB code = 16-byte-block DXT variant (DXT3/4/5), same as
//   DXT5 for sizing purposes.
// `dimension` marker: 2 = 2D, 3 = 3D (volume; slice count lives at +0x18).
// `cubemap_flag`: 1 = cubemap (6 faces).
// ---------------------------------------------------------------------------
struct TextureRecord {
    u32 vftable_pointer;    // +0x00 runtime vtable, zero on disc
    u32 id;                 // +0x04 texture id, key into the name table
    u32 unknown_08;         // +0x08 unknown
    u8  format;             // +0x0C game format code (see table above)
    u8  mipmap_count;       // +0x0D number of mip levels, includes the top level
    u8  dimension;          // +0x0E 2 = 2D, 3 = 3D (volume)
    u8  cubemap_flag;       // +0x0F 1 = 6 faces
    u32 unknown_10;         // +0x10 unknown
    u16 width;              // +0x14 base width in pixels
    u16 height;             // +0x16 base height in pixels
    u16 depth;              // +0x18 slice count, only meaningful for 3D textures
    u8  unknown_1A[10];     // +0x1A unused/padding to fill the 0x24 record
};

// ---------------------------------------------------------------------------
// PT entry (8 bytes). Always zeroed on disc; files without streamtex entries
// have `pt_count == 0` and no PT table at all.
// ---------------------------------------------------------------------------
struct PtEntry {
    u32 unknown_00;
    u32 unknown_04;
};

// ---------------------------------------------------------------------------
// Catalog entry (12 bytes). Lives in the (separate) entry array of a catalog.
//
//   record_offset  header-relative -> the TextureRecord for this slot
//   pt_offset      header-relative -> the matching PT entry
//   data_offset    relative to the catalog's data section start (embedded), or
//                  an absolute offset into the sibling `.streamtex` file when
//                  the catalog is streamed (`data_size == 0`).
//
// Streamed (.streamtex) sibling layout (validated on airport_test_03):
//   DXT records sit at 128-byte aligned slots: align128(mip_chain_size) per
//   cubemap face. A8R8G8B8 records occupy a dedicated, larger allocation slot
//   (observed: 4 MiB for 1024x512, 130,560 for 128x128). The pixel data of a
//   record starts exactly at its slot start; the slot size only advances the
//   cursor to the next record.)
// ---------------------------------------------------------------------------
struct CatalogEntry {
    u32 record_offset [[comment("header-relative -> TextureRecord")]];
    u32 pt_offset     [[comment("header-relative -> PtEntry")]];
    u32 data_offset   [[comment("catalog-relative, or absolute .streamtex offset")]];
};

// ---------------------------------------------------------------------------
// Catalog (16 bytes).
//
//   data_size     0 => streamed catalog: pixel data lives in the sibling
//                 `.streamtex`, not inline. Non-zero => size in bytes of this
//                 catalog's raw data section (placed right after the header).
//   entry_count   number of 12-byte entries in the catalog's entry array.
//   entries_offset offset of the entry array, header-relative.
// ---------------------------------------------------------------------------
struct Catalog {
    u32 data_size      [[comment("0 = streamed (.streamtex sidecar)")]];
    u32 unknown_04;
    u32 entry_count;
    u32 entries_offset;
};

// ---------------------------------------------------------------------------
// Fixed 0x18-byte header table at BASE_OFFSET (0x10). Each offset is
// header-relative, i.e. absolute = 0x10 + offset.
// ---------------------------------------------------------------------------
struct Ps3Header {
    u32 record_count;       // abs +0x10
    u32 records_offset;     // abs +0x14, header-relative
    u32 pt_count;           // abs +0x18
    u32 pt_offset;          // abs +0x1C, header-relative (0 when pt_count == 0)
    u32 catalog_count;      // abs +0x20
    u32 catalogs_offset;    // abs +0x24, header-relative
};

// ---------------------------------------------------------------------------
// Name table record, optional. Prefix `C2NM` is followed by repeated
// `[u32 id][u32 len][name bytes]` entries until EOF.
// ---------------------------------------------------------------------------
struct Name {
    u32 id;
    u32 len;
    char value[len];
};

// ===========================================================================
// Root
// ===========================================================================

char magic[4] @ $;
if (magic == "TEXS") {
    // PS3 builds are big-endian.
    std::core::set_endian(std::mem::Endian::Big);
} else if (magic == "SXET") {
    // PC builds are little-endian; this pattern targets the PS3 layout only.
    std::core::set_endian(std::mem::Endian::Little);
    std::error("This is a PC (\"SXET\", little-endian) file; this pattern covers the PS3 \"TEXS\" layout.");
} else {
    std::error("Magic is neither \"TEXS\" nor \"SXET\"; not a .textures file?");
}

u32 version  @ $ [[comment("expected 12")]];
u32 platform @ $ [[comment("expected 1")]];
u32 table_size @ $ [[comment("header region size: [0x10, 0x10 + table_size)")]];

// The fixed header table starts at BASE_OFFSET (0x10).
Ps3Header header @ BASE_OFFSET;

// Records / PT / catalogs, each anchored at its header-relative offset.
TextureRecord records[header.record_count] @ (BASE_OFFSET + header.records_offset);
PtEntry       pt[header.pt_count]            @ (header.pt_count > 0 ? (BASE_OFFSET + header.pt_offset) : 0);
Catalog       catalogs[header.catalog_count] @ (BASE_OFFSET + header.catalogs_offset);

// ---------------------------------------------------------------------------
// Aggregate helpers (defined after the globals they read).
// ---------------------------------------------------------------------------

fn total_entries() {
    u64 n = 0;
    u32 c = 0;
    while (c < header.catalog_count) {
        n = n + catalogs[c].entry_count;
        c = c + 1;
    }
    return n;
};

fn total_data_size() {
    u64 n = 0;
    u32 c = 0;
    while (c < header.catalog_count) {
        n = n + catalogs[c].data_size;
        c = c + 1;
    }
    return n;
};

// ---------------------------------------------------------------------------
// Catalog entries (12 bytes each), flattened in catalog order.
//
// Each catalog's entry array is a contiguous block at
// `BASE_OFFSET + entries_offset`; in the verified files these blocks are
// arranged back-to-back, so a single linear array is correct here.
// ---------------------------------------------------------------------------
u64 entries_base = (header.catalog_count > 0) ? (BASE_OFFSET + catalogs[0].entries_offset) : 0;
CatalogEntry entries[total_entries()] @ entries_base;

// ---------------------------------------------------------------------------
// Raw data band: the embedded sections begin right after the table and are
// packed sequentially (catalog 0, then 1, ...). Each streamed catalog has
// data_size == 0 and contributes nothing here.
// ---------------------------------------------------------------------------
u64 data_start = BASE_OFFSET + table_size;
u8 raw_data[total_data_size()] @ data_start;

// The name table (if any) sits right after the last data section.
u64 names_offset = data_start + total_data_size();

// ---------------------------------------------------------------------------
// Optional name table. Only present when the marker "C2NM" sits before EOF.
// Files without a name table simply end after the data.
// ---------------------------------------------------------------------------
struct NameTable {
    if (names_offset + 4 <= sizeof($)) {
        char name_table_marker[4];
        if (name_table_marker != "C2NM") {
            std::error("Name-table marker is not \"C2NM\"");
        }
        Name names[while($ < sizeof($))];
    }
};

NameTable name_table @ names_offset;
