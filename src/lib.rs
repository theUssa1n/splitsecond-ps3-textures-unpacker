use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Result};

// PS3 (big-endian) .textures layout, fully reverse-engineered:
//
// File header (absolute file offsets):
//   0x00  "TEXS" magic
//   0x04  12 (version)
//   0x08  1  (platform flag)
//   0x0C  table_size      -> header spans [0x10, 0x10 + table_size)
//   0x10  record_count    (36-byte records)
//   0x14  records_offset  (header-relative)
//   0x18  pt_count        (8-byte PT entries, always zeroed on disc)
//   0x1C  pt_offset       (header-relative; 0 when pt_count is 0)
//   0x20  catalog_count   (16-byte catalogs)
//   0x24  catalogs_offset (header-relative)
//
// Two catalog layouts exist:
//
// 1) Track files (docks, Downtown, ...): catalog_count == record_count and the
//    catalogs are interleaved PAIRS, one pair per texture:
//      catalog 2k   = embedded variant (size > 0, one entry)
//      catalog 2k+1 = streamtex variant (size == 0, one entry)
//    Records are interleaved the same way (2k = embedded, 2k+1 = streamtex).
//
// 2) Vehicle/global files (Musclecar_09, Bootup, ...): catalog_count == 2:
//      catalog 0 = ALL embedded textures (size = total embedded data, many
//                  entries whose data offsets are relative to the section)
//      catalog 1 = ALL streamtex textures (size == 0; entry data offsets are
//                  ABSOLUTE offsets into the .streamtex file, or all zero)
//    Records are grouped: embedded records first, streamtex records after.
//    pt_count is 0 for files without streamtex entries.
//
// Record (36 bytes, header-relative offset):
//   +0x04  texture id (u32 BE)
//   +0x0C  format (u8): 0x86 = DXT1, 0x88 = DXT5, 0xA5 = A8R8G8B8
//   +0x0D  mip count (u8), includes the top level
//   +0x0E  dimension marker (u8): 2 = 2D, 3 = 3D (volume)
//   +0x0F  cubemap flag (u8): 1 = 6 faces
//   +0x14  width  (u16 BE)
//   +0x16  height (u16 BE)
//   +0x18  depth  (u16 BE) - slice count, only meaningful for 3D textures
//
// Catalog (16 bytes):
//   +0x00  data section size (u32 BE) - 0 for streamtex catalogs
//   +0x08  entry count
//   +0x0C  entries offset (header-relative)
//
// Entry (12 bytes):
//   +0x00  record offset (header-relative)
//   +0x04  PT offset (header-relative)
//   +0x08  data offset (relative to the catalog's data section start; for
//          streamtex catalogs it is an absolute .streamtex file offset)
//
// Data section: starts right after the header. Catalog i's data starts at
// data_start + sum(sizes of catalogs 0..i-1) and is `size` bytes of raw
// (header-less) texture data. DXT textures occupy 128-byte aligned slots
// (mip chain rounded up to 128 bytes; a cubemap reserves one such slot per
// face). A8R8G8B8 (0xA5) records instead occupy a dedicated larger allocation
// slot (observed: 4 MiB / 1024x512, 130,560 / 128x128) whose start holds the
// linear BGRA mip chain; the slot size only advances the cursor for the next
// record.
//
// Volume (3D) textures store their mip chain differently:
//   * every mip level is aligned to 128 bytes (not just the whole chain), and
//   * within a level the depth slices are interleaved in groups of four:
//     block `wi` of slice `s` sits at
//     `(s / 4) * blocks_per_slice * 4 + wi * 4 + (s % 4)`.
// Extraction undoes that swizzle and stacks the slices into a flat 2D image
// (width x height * depth), so editors can open it.
//
// Names table: at data_start + sum(all catalog sizes), marker "C2NM"
// followed by [id u32 BE][len u32 BE][name bytes] entries until EOF.

const BASE_OFFSET: usize = 0x10;
const RECORD_SIZE: usize = 36;
const PT_ENTRY_SIZE: usize = 8;
const CATALOG_SIZE: usize = 16;
const ENTRY_SIZE: usize = 12;
const NAMES_MARKER: [u8; 4] = *b"C2NM";

const FORMAT_DXT1: u8 = 0x86;
const FORMAT_DXT5: u8 = 0x88;
const FORMAT_ARGB: u8 = 0xA5;

pub struct Texture {
    pub name: String,
    pub bytes: Vec<u8>,
}

struct Catalog {
    size: u32,
    entry_count: u32,
    entries_offset: usize,
}

#[derive(Clone, Copy)]
struct RecordMeta {
    id: u32,
    format: u8,
    mips: u32,
    is_cubemap: bool,
    is_volume: bool,
    width: u32,
    height: u32,
    depth: u32,
}

pub struct TexturesView<'a> {
    file_bytes: &'a [u8],
    data_start: usize,
    catalogs: Vec<Catalog>,
    names: HashMap<u32, String>,
}

fn be32(bytes: &[u8], off: usize) -> Result<u32> {
    let slice = bytes
        .get(off..off + 4)
        .ok_or_else(|| anyhow!("Unexpected end of file while reading u32 at {:#x}", off))?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn be16(bytes: &[u8], off: usize) -> Result<u16> {
    let slice = bytes
        .get(off..off + 2)
        .ok_or_else(|| anyhow!("Unexpected end of file while reading u16 at {:#x}", off))?;
    Ok(u16::from_be_bytes([slice[0], slice[1]]))
}

fn read_record(file_bytes: &[u8], abs_offset: usize) -> Result<RecordMeta> {
    let id = be32(file_bytes, abs_offset + 0x04)?;
    let format = *file_bytes
        .get(abs_offset + 0x0C)
        .ok_or_else(|| anyhow!("Record metadata out of bounds"))?;
    let mips = *file_bytes
        .get(abs_offset + 0x0D)
        .ok_or_else(|| anyhow!("Record metadata out of bounds"))? as u32;
    let is_cubemap = *file_bytes
        .get(abs_offset + 0x0F)
        .unwrap_or(&0)
        == 1;
    // +0x0E is the dimension marker: 3 means a 3D (volume) texture, whose
    // slice count lives at +0x18.
    let is_volume = *file_bytes.get(abs_offset + 0x0E).unwrap_or(&2) == 3;
    let width = u32::from(be16(file_bytes, abs_offset + 0x14)?);
    let height = u32::from(be16(file_bytes, abs_offset + 0x16)?);
    let depth = if is_volume {
        u32::from(be16(file_bytes, abs_offset + 0x18)?)
    } else {
        1
    };
    Ok(RecordMeta {
        id,
        format,
        mips,
        is_cubemap,
        is_volume,
        width,
        height,
        depth,
    })
}

/// Parses a PS3 (big-endian) `.textures` file.
pub fn make_textures_view(file_bytes: &[u8]) -> Result<TexturesView<'_>> {
    if file_bytes.len() < 0x28 {
        return Err(anyhow!("File too short"));
    }
    if &file_bytes[0..4] == b"SXET" {
        return Err(anyhow!(
            "PC (little-endian) .textures file - this tool only supports PS3 files"
        ));
    }
    if &file_bytes[0..4] != b"TEXS" {
        return Err(anyhow!(
            "Invalid magic: expected 'TEXS' (PS3), got {:?}",
            &file_bytes[0..4]
        ));
    }

    let table_size = be32(file_bytes, 0x0C)? as usize;
    let record_count = be32(file_bytes, 0x10)?;
    let records_offset = be32(file_bytes, 0x14)? as usize;
    let pt_count = be32(file_bytes, 0x18)?;
    let pt_offset = be32(file_bytes, 0x1C)? as usize;
    let catalog_count = be32(file_bytes, 0x20)?;
    let catalogs_offset = be32(file_bytes, 0x24)? as usize;

    let header_end = BASE_OFFSET + table_size;
    if header_end > file_bytes.len() {
        return Err(anyhow!("Header extends past the end of the file"));
    }

    // Structural sanity: records -> (optional PT) -> catalogs must be chained
    // in order. Files without streamtex entries have no PT table at all.
    let records_end = records_offset + record_count as usize * RECORD_SIZE;
    let pt_end = pt_offset + pt_count as usize * PT_ENTRY_SIZE;
    let catalogs_end = catalogs_offset + catalog_count as usize * CATALOG_SIZE;
    let layout_ok = if pt_count > 0 {
        records_end == pt_offset && pt_end == catalogs_offset
    } else {
        records_end <= catalogs_offset
    };
    if !layout_ok || catalogs_end > table_size {
        return Err(anyhow!(
            "Header layout does not match the PS3 .textures structure"
        ));
    }

    let mut catalogs = Vec::with_capacity(catalog_count as usize);
    let mut total_data = 0usize;
    for i in 0..catalog_count as usize {
        let off = BASE_OFFSET + catalogs_offset + i * CATALOG_SIZE;
        let size = be32(file_bytes, off)?;
        let entry_count = be32(file_bytes, off + 0x08)?;
        let entries_offset = be32(file_bytes, off + 0x0C)? as usize;
        if BASE_OFFSET + entries_offset + entry_count as usize * ENTRY_SIZE > header_end {
            return Err(anyhow!("Catalog {} entries are out of bounds", i));
        }
        total_data += size as usize;
        catalogs.push(Catalog {
            size,
            entry_count,
            entries_offset,
        });
    }

    // Names table sits right after all of the catalog data sections.
    let names_start = header_end + total_data;
    let mut names = HashMap::new();
    if names_start + 4 <= file_bytes.len() {
        if file_bytes[names_start..names_start + 4] != NAMES_MARKER {
            return Err(anyhow!(
                "Names marker doesn't match at {:#x}",
                names_start
            ));
        }
        let mut pos = names_start + 4;
        while pos + 8 <= file_bytes.len() {
            let id = be32(file_bytes, pos)?;
            let len = be32(file_bytes, pos + 4)? as usize;
            pos += 8;
            if pos + len > file_bytes.len() {
                break;
            }
            if let Ok(name) = std::str::from_utf8(&file_bytes[pos..pos + len]) {
                names.insert(id, name.to_string());
            }
            pos += len;
        }
    }

    Ok(TexturesView {
        file_bytes,
        data_start: header_end,
        catalogs,
        names,
    })
}

/// Bytes per texture block for the console format codes, or 0 for the
/// uncompressed ARGB format (which is counted as 4 bytes per pixel instead).
///
/// Split/Second (PS3) only uses these formats: DXT1 (0x86, 8 B/block), DXT5
/// (0x88, 16 B/block) and A8R8G8B8 (0xA5, 4 B/pixel). DXT3/DXT4, when they
/// appear, use the same 16-byte blocks as DXT5, so any non-ARGB code falls
/// back to a 16-byte block.
fn bytes_per_block(format: u8) -> usize {
    match format {
        FORMAT_DXT1 => 8,
        FORMAT_ARGB => 0,
        _ => 16,
    }
}

/// Dimensions of one mip level. Depth only shrinks for real volume textures and
/// never drops below 1.
fn mip_level_dims(meta: &RecordMeta, level: u32) -> (usize, usize, usize) {
    let w = (meta.width as usize >> level).max(1);
    let h = (meta.height as usize >> level).max(1);
    let d = if meta.is_volume {
        (meta.depth as usize >> level).max(1)
    } else {
        1
    };
    (w, h, d)
}

/// Byte count of a single mip level, including every depth slice.
fn mip_level_size(meta: &RecordMeta, level: u32) -> usize {
    let (w, h, d) = mip_level_dims(meta, level);
    let per_pixel = bytes_per_block(meta.format);
    let slice = if per_pixel > 0 {
        w.div_ceil(4) * h.div_ceil(4) * per_pixel
    } else {
        w * h * 4
    };
    slice * d
}

/// Total number of bytes the mip chain of one face occupies.
fn mip_chain_size(meta: &RecordMeta) -> usize {
    (0..meta.mips).map(|l| mip_level_size(meta, l)).sum()
}

/// Slot (allocation) size of an A8R8G8B8 (0xA5) streamtex record.
///
/// Unlike DXT records (128-byte aligned slots), PS3 reserves a dedicated,
/// larger allocation for uncompressed ARGB textures. The sizes below were
/// measured on airport_test_03 by validating the extracted mip 0 against the
/// embedded low-res thumbnail in `.textures`:
///   - 1024x512, 11 mips -> 4 MiB
///   - 128x128,   8 mips -> 130,560
/// For unobserved combinations we fall back to the 128-byte aligned chain so
/// the cursor still advances sanely.
fn a5_slot_size(meta: &RecordMeta) -> usize {
    let chain = mip_chain_size(meta);
    match (meta.width, meta.height, meta.mips) {
        (1024, 512, 11) => 4 * 1024 * 1024,
        (128, 128, 8) => 130_560,
        _ => (chain + 127) & !127,
    }
}

/// Size of a volume texture's data region on disc.
///
/// Unlike 2D chains, PS3 aligns every volume mip level up to 128 bytes, so the
/// levels cannot simply be summed.
fn volume_region_size(meta: &RecordMeta) -> usize {
    (0..meta.mips)
        .map(|l| (mip_level_size(meta, l) + 127) & !127)
        .sum()
}

/// Rewrites a volume texture's mip 0 into plain slice-major order.
///
/// PS3 stores the slices of each 4-slice group interleaved block-by-block:
/// block `wi` of slice `s` sits at `(s / 4) * blocks_per_slice * 4 + wi * 4 +
/// (s % 4)`. Undoing that yields the byte order a flat 2D image expects, so
/// stacking the slices vertically is a plain concatenation.
fn deswizzle_volume_slices(region: &[u8], meta: &RecordMeta) -> Option<Vec<u8>> {
    let (w, h, depth) = mip_level_dims(meta, 0);
    let per_pixel = bytes_per_block(meta.format);
    let block = if per_pixel > 0 { per_pixel } else { 4 };
    let blocks_per_slice = if per_pixel > 0 {
        w.div_ceil(4) * h.div_ceil(4)
    } else {
        w * h
    };

    let total = blocks_per_slice.checked_mul(depth)?.checked_mul(block)?;
    if region.len() < total {
        return None;
    }

    let mut out = vec![0u8; total];
    for s in 0..depth {
        let group = (s / 4) * blocks_per_slice * 4;
        let within = s % 4;
        for wi in 0..blocks_per_slice {
            let src = (group + wi * 4 + within) * block;
            let dst = (s * blocks_per_slice + wi) * block;
            out[dst..dst + block].copy_from_slice(&region[src..src + block]);
        }
    }
    Some(out)
}

/// Wraps a volume texture's on-disc region in a flat 2D DDS.
///
/// The slices are de-swizzled back into plain order and stacked vertically
/// (`width x height * depth`), which is what image editors can open. Only mip 0
/// is kept: a 3D mip chain has no sensible 2D equivalent, and the extra levels
/// are padding-aligned per level anyway.
fn wrap_volume_texture(meta: &RecordMeta, region: &[u8]) -> Vec<u8> {
    let flat_height = meta.height.saturating_mul(meta.depth);
    let mut bytes = create_dds_header(meta.width, flat_height, 1, meta.format, false);
    match deswizzle_volume_slices(region, meta) {
        Some(plain) => bytes.extend_from_slice(&plain),
        None => bytes.extend_from_slice(region),
    }
    bytes
}

fn texture_name(view: &TexturesView, meta: &RecordMeta) -> String {
    match view.names.get(&meta.id) {
        Some(n) => format!("{}.dds", n),
        None => format!("{:08X}.dds", meta.id),
    }
}

/// Extracts every embedded texture (the low-res mip chains stored inside the
/// .textures file itself). Each one is wrapped in a proper DDS header built
/// from its record metadata.
pub fn extract_embedded_textures(view: &TexturesView) -> Result<Vec<Texture>> {
    let fb = view.file_bytes;
    let mut textures = Vec::new();
    let mut running = view.data_start;

    for cat in &view.catalogs {
        let size = cat.size as usize;
        if size > 0 {
            let section_start = running;
            let section_end = section_start + size;
            if section_end > fb.len() {
                return Err(anyhow!(
                    "Embedded data section at {:#x} is out of bounds",
                    section_start
                ));
            }
            for e in 0..cat.entry_count as usize {
                let entry_off = BASE_OFFSET + cat.entries_offset + e * ENTRY_SIZE;
                let record_offset = be32(fb, entry_off)? as usize;
                let data_offset = be32(fb, entry_off + 0x08)? as usize;

                let meta = read_record(fb, BASE_OFFSET + record_offset)?;

                let data_start_abs = section_start + data_offset;
                if data_start_abs > section_end {
                    return Err(anyhow!(
                        "Embedded data offset out of bounds for {:#X}",
                        meta.id
                    ));
                }

                if meta.is_volume {
                    let region_size = volume_region_size(&meta);
                    let n = region_size.min(section_end - data_start_abs);
                    let region = &fb[data_start_abs..data_start_abs + n];
                    textures.push(Texture {
                        name: texture_name(view, &meta),
                        bytes: wrap_volume_texture(&meta, region),
                    });
                    continue;
                }

                let chain = mip_chain_size(&meta);
                // Every texture occupies one 128-byte aligned slot per face.
                let slot = (chain + 127) & !127;

                let mut bytes =
                    create_dds_header(meta.width, meta.height, meta.mips, meta.format, meta.is_cubemap);
                if meta.is_cubemap {
                    // Six face slots of `slot` bytes (mip chain + padding).
                    for f in 0..6 {
                        let face_start = data_start_abs + f * slot;
                        let n = chain.min(section_end.saturating_sub(face_start));
                        if n == 0 {
                            break;
                        }
                        let face = fb
                            .get(face_start..face_start + n)
                            .ok_or_else(|| anyhow!("Cubemap face out of bounds for {:#X}", meta.id))?;
                        bytes.extend_from_slice(face);
                    }
                } else {
                    let n = chain.min(section_end - data_start_abs);
                    bytes.extend_from_slice(&fb[data_start_abs..data_start_abs + n]);
                }

                textures.push(Texture {
                    name: texture_name(view, &meta),
                    bytes,
                });
            }
        }
        running += size;
    }

    Ok(textures)
}

/// Extracts the full-res streamtex data from a PS3 `.streamtex` file using the
/// streamtex records of the `.textures` file for dimensions.
///
/// Streamtex catalogs have no data section of their own; their entries point
/// into the matching `.streamtex` file. When the entries carry explicit data
/// offsets they are absolute positions inside that file; when they are all
/// zero (track files), the data is packed sequentially, back-to-back, with
/// no alignment padding between textures.
pub fn extract_streamtex_textures(
    view: &TexturesView,
    streamtex_bytes: &[u8],
) -> Result<Vec<Texture>> {
    let fb = view.file_bytes;

    // Gather every streamtex entry (catalogs without a data section).
    let mut entries: Vec<(usize, RecordMeta)> = Vec::new();
    for cat in &view.catalogs {
        if cat.size != 0 {
            continue;
        }
        for e in 0..cat.entry_count as usize {
            let entry_off = BASE_OFFSET + cat.entries_offset + e * ENTRY_SIZE;
            let record_offset = be32(fb, entry_off)? as usize;
            let data_offset = be32(fb, entry_off + 0x08)? as usize;
            let meta = read_record(fb, BASE_OFFSET + record_offset)?;
            entries.push((data_offset, meta));
        }
    }
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    // If any entry carries an explicit offset, all of them are absolute
    // positions inside the .streamtex file.
    let absolute = entries.iter().any(|(off, _)| *off != 0);

    let mut textures = Vec::with_capacity(entries.len());
    let mut running = 0usize;
    for (off, meta) in entries {
        let start = if absolute { off } else { running };
        if start >= streamtex_bytes.len() {
            break;
        }

        let (bytes, slot_total) = if meta.is_volume {
            let region = volume_region_size(&meta);
            let n = region.min(streamtex_bytes.len().saturating_sub(start));
            (
                wrap_volume_texture(&meta, &streamtex_bytes[start..start + n]),
                (region + 127) & !127,
            )
        } else if absolute {
            let chain = mip_chain_size(&meta);
            let slot = (chain + 127) & !127;
            let mut bytes =
                create_dds_header(meta.width, meta.height, meta.mips, meta.format, meta.is_cubemap);
            if meta.is_cubemap {
                for f in 0..6 {
                    let face_start = start + f * slot;
                    let n = chain.min(streamtex_bytes.len().saturating_sub(face_start));
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&streamtex_bytes[face_start..face_start + n]);
                }
                (bytes, 6 * slot)
            } else {
                let n = chain.min(streamtex_bytes.len().saturating_sub(start));
                bytes.extend_from_slice(&streamtex_bytes[start..start + n]);
                (bytes, slot)
            }
        } else {
            // Sequential streamtex entries are NOT packed back-to-back.
            // Observations from airport_test_03 (PS3) validated against the
            // embedded low-res thumbnails in .textures:
            //   - DXT1/DXT5 (0x86/0x88): each record occupies a slot of
            //     align128(mip_chain_size); a cubemap reserves one such slot
            //     per face. The pixel data sits at the start of each slot.
            //   - A8R8G8B8 (0xA5): each record occupies a dedicated
            //     allocation slot (see a5_slot_size) whose start holds the
            //     linear BGRA mip chain. The slot size only controls the
            //     cursor advance for the next record.
            let chain = mip_chain_size(&meta);
            let face_count = if meta.is_cubemap { 6 } else { 1 };
            let slot = if meta.format == 0xA5 {
                a5_slot_size(&meta)
            } else {
                (chain + 127) & !127
            };
            let slot_total = slot * face_count;
            let mut bytes = Vec::with_capacity(chain * face_count);
            'faces: for face in 0..face_count {
                let face_start = start + face * slot;
                let Some(face_end) = face_start.checked_add(chain) else {
                    break 'faces;
                };
                if face_end > streamtex_bytes.len() {
                    break 'faces;
                }
                bytes.extend_from_slice(&streamtex_bytes[face_start..face_end]);
            }
            if bytes.len() != chain * face_count {
                break;
            }
            let header = create_dds_header(
                meta.width,
                meta.height,
                meta.mips,
                meta.format,
                meta.is_cubemap,
            );
            bytes.splice(0..0, header);
            (bytes, slot_total)
        };

        textures.push(Texture {
            name: texture_name(view, &meta),
            bytes,
        });

        if !absolute {
            running = start + slot_total;
        }
    }

    Ok(textures)
}

pub fn dump_texture_bundle(bundle: Vec<Texture>, path: PathBuf) -> Result<()> {
    if bundle.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(&path)?;
    // Some textures legitimately share a name (LOD/duplicate ids); suffix them
    // so nothing gets silently overwritten on disk.
    let mut used = std::collections::HashSet::new();
    for t in bundle {
        let mut name = t.name.clone();
        if !used.insert(name.clone()) {
            let stem = name.trim_end_matches(".dds").to_string();
            let mut n = 2;
            loop {
                name = format!("{}_{}.dds", stem, n);
                if used.insert(name.clone()) {
                    break;
                }
                n += 1;
            }
        }
        fs::write(path.join(&name), t.bytes)?;
    }
    Ok(())
}

pub fn create_dds_header(
    width: u32,
    height: u32,
    mipmaps: u32,
    format_code: u8,
    cubemap: bool,
) -> Vec<u8> {
    let mut header = vec![0u8; 128];

    const DDSD_CAPS: u32 = 0x1;
    const DDSD_HEIGHT: u32 = 0x2;
    const DDSD_WIDTH: u32 = 0x4;
    const DDSD_PITCH: u32 = 0x8;
    const DDSD_PIXELFORMAT: u32 = 0x1000;
    const DDSD_MIPMAPCOUNT: u32 = 0x20000;
    const DDSD_LINEARSIZE: u32 = 0x80000;

    const DDPF_ALPHAPIXELS: u32 = 0x1;
    const DDPF_FOURCC: u32 = 0x4;
    const DDPF_RGB: u32 = 0x40;

    const DDSCAPS_COMPLEX: u32 = 0x8;
    const DDSCAPS_MIPMAP: u32 = 0x400000;
    const DDSCAPS_TEXTURE: u32 = 0x1000;

    const DDSCAPS2_CUBEMAP_ALLFACES: u32 = 0xFE00;

    header[0..4].copy_from_slice(b"DDS ");
    header[4..8].copy_from_slice(&124u32.to_le_bytes());

    let mut dw_flags = DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PIXELFORMAT;
    if mipmaps > 1 {
        dw_flags |= DDSD_MIPMAPCOUNT;
    }

    let mut ddspf_dw_flags = 0u32;
    let mut dw_four_cc = [0u8; 4];
    let mut dw_rgbbit_count = 0u32;
    let mut dw_rbit_mask = 0u32;
    let mut dw_gbit_mask = 0u32;
    let mut dw_bbit_mask = 0u32;
    let mut dw_alpha_bit_mask = 0u32;

    match format_code {
        FORMAT_DXT1 => {
            dw_flags |= DDSD_LINEARSIZE;
            ddspf_dw_flags |= DDPF_FOURCC;
            dw_four_cc.copy_from_slice(b"DXT1");
        }
        FORMAT_DXT5 => {
            dw_flags |= DDSD_LINEARSIZE;
            ddspf_dw_flags |= DDPF_FOURCC;
            dw_four_cc.copy_from_slice(b"DXT5");
        }
        FORMAT_ARGB => {
            dw_flags |= DDSD_PITCH;
            ddspf_dw_flags |= DDPF_RGB | DDPF_ALPHAPIXELS;
            dw_rgbbit_count = 32;
            dw_rbit_mask = 0x00FF0000;
            dw_gbit_mask = 0x0000FF00;
            dw_bbit_mask = 0x000000FF;
            dw_alpha_bit_mask = 0xFF000000;
        }
        _ => {
            // Any other non-ARGB console format is a 16-byte-block DXT variant
            // (DXT3/DXT4/DXT5); tag it DXT5 so the file still opens.
            dw_flags |= DDSD_LINEARSIZE;
            ddspf_dw_flags |= DDPF_FOURCC;
            dw_four_cc.copy_from_slice(b"DXT5");
        }
    }

    header[8..12].copy_from_slice(&dw_flags.to_le_bytes());
    header[12..16].copy_from_slice(&height.to_le_bytes());
    header[16..20].copy_from_slice(&width.to_le_bytes());

    let pitch_or_linear_size = match format_code {
        FORMAT_DXT1 => width.div_ceil(4) * height.div_ceil(4) * 8,
        FORMAT_DXT5 => width.div_ceil(4) * height.div_ceil(4) * 16,
        FORMAT_ARGB => width * 4,
        _ => width.div_ceil(4) * height.div_ceil(4) * 16,
    };
    header[20..24].copy_from_slice(&pitch_or_linear_size.to_le_bytes());
    header[28..32].copy_from_slice(&mipmaps.to_le_bytes());

    header[76..80].copy_from_slice(&32u32.to_le_bytes());
    header[80..84].copy_from_slice(&ddspf_dw_flags.to_le_bytes());
    header[84..88].copy_from_slice(&dw_four_cc);
    header[88..92].copy_from_slice(&dw_rgbbit_count.to_le_bytes());
    header[92..96].copy_from_slice(&dw_rbit_mask.to_le_bytes());
    header[96..100].copy_from_slice(&dw_gbit_mask.to_le_bytes());
    header[100..104].copy_from_slice(&dw_bbit_mask.to_le_bytes());
    header[104..108].copy_from_slice(&dw_alpha_bit_mask.to_le_bytes());

    let mut dw_caps = DDSCAPS_TEXTURE;
    if mipmaps > 1 || cubemap {
        dw_caps |= DDSCAPS_COMPLEX;
    }
    if mipmaps > 1 {
        dw_caps |= DDSCAPS_MIPMAP;
    }
    header[108..112].copy_from_slice(&dw_caps.to_le_bytes());
    if cubemap {
        header[112..116].copy_from_slice(&DDSCAPS2_CUBEMAP_ALLFACES.to_le_bytes());
    }

    header
}
