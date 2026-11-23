use unpack_ss_utils::{
    extract_streamtex_textures, make_textures_view,
};

/// Builds a minimal PS3 .textures with two sequential streamtex entries. The
/// cubemap has two 4x4 DXT1 mips; the stream data is packed back-to-back with
/// no padding, so each face is just the concatenated mip chain (16 bytes) and
/// the plane follows all six face slots.
fn make_textures() -> Vec<u8> {
    // Header scalars occupy [0x10, 0x28). Records start right after, at header
    // offset 40, so they never overwrite the header fields.
    let records_offset = 40usize;
    let catalogs_offset = records_offset + 2 * 36; // 112
    let entries_offset = catalogs_offset + 16;      // 128

    let mut t = vec![0u8; 176];
    t[0..4].copy_from_slice(b"TEXS");
    t[4..8].copy_from_slice(&12u32.to_be_bytes());   // version
    t[8..12].copy_from_slice(&1u32.to_be_bytes());   // platform
    t[12..16].copy_from_slice(&160u32.to_be_bytes()); // table_size
    t[16..20].copy_from_slice(&2u32.to_be_bytes());   // record_count
    t[20..24].copy_from_slice(&(records_offset as u32).to_be_bytes());
    t[24..28].copy_from_slice(&0u32.to_be_bytes());   // pt_count
    t[28..32].copy_from_slice(&0u32.to_be_bytes());   // pt_offset
    t[32..36].copy_from_slice(&1u32.to_be_bytes());   // catalog_count
    t[36..40].copy_from_slice(&(catalogs_offset as u32).to_be_bytes());

    // record 0 (cubemap)
    let r0 = 0x10 + records_offset;
    t[r0 + 4..r0 + 8].copy_from_slice(&1u32.to_be_bytes());
    t[r0 + 0x0C] = 0x86; // DXT1
    t[r0 + 0x0D] = 2;    // mips
    t[r0 + 0x0E] = 2;    // 2D
    t[r0 + 0x0F] = 1;    // cubemap
    t[r0 + 0x14..r0 + 0x16].copy_from_slice(&4u16.to_be_bytes());
    t[r0 + 0x16..r0 + 0x18].copy_from_slice(&4u16.to_be_bytes());
    t[r0 + 0x18..r0 + 0x1A].copy_from_slice(&1u16.to_be_bytes());

    // record 1 (plane)
    let r1 = r0 + 36;
    t[r1 + 4..r1 + 8].copy_from_slice(&2u32.to_be_bytes());
    t[r1 + 0x0C] = 0x86;
    t[r1 + 0x0D] = 1;
    t[r1 + 0x0E] = 2;
    t[r1 + 0x0F] = 0;
    t[r1 + 0x14..r1 + 0x16].copy_from_slice(&4u16.to_be_bytes());
    t[r1 + 0x16..r1 + 0x18].copy_from_slice(&4u16.to_be_bytes());
    t[r1 + 0x18..r1 + 0x1A].copy_from_slice(&1u16.to_be_bytes());

    // catalog: streamtex (size 0), 2 entries
    let c = 0x10 + catalogs_offset;
    t[c + 0..c + 4].copy_from_slice(&0u32.to_be_bytes());    // size
    t[c + 4..c + 8].copy_from_slice(&0u32.to_be_bytes());    // pad
    t[c + 8..c + 12].copy_from_slice(&2u32.to_be_bytes());   // entry_count
    t[c + 12..c + 16].copy_from_slice(&(entries_offset as u32).to_be_bytes());

    // entries (record_offset, pt_offset, data_offset)
    let e0 = 0x10 + entries_offset + 0 * 12;
    let e1 = 0x10 + entries_offset + 1 * 12;
    t[e0..e0 + 4].copy_from_slice(&(records_offset as u32).to_be_bytes());
    t[e0 + 8..e0 + 12].copy_from_slice(&0u32.to_be_bytes());
    t[e1..e1 + 4].copy_from_slice(&((records_offset + 36) as u32).to_be_bytes());
    t[e1 + 8..e1 + 12].copy_from_slice(&0u32.to_be_bytes());

    t
}

#[test]
fn streamtex_cubemap_sequential_face_slots() {
    let t = make_textures();
    let view = make_textures_view(&t).expect("parse");

    // stream: each cubemap face is the 16-byte mip chain (mip0 = 8 bytes,
    // mip1 = 8 bytes); six faces = 96 bytes, then the plane starts at 96.
    let mut s = vec![0u8; 96 + 16];
    for f in 0..6 {
        s[f * 16] = (f + 1) as u8;          // face f, mip0
        s[f * 16 + 8] = (f + 11) as u8;     // face f, mip1
    }
    s[96] = 0xEE;

    let out = extract_streamtex_textures(&view, &s).expect("extract");
    assert_eq!(out.len(), 2);

    // Each face contributes both mip payloads contiguously.
    let cube = &out[0].bytes;
    assert!(cube.len() >= 128);
    let payload = &cube[128..];
    let mut expected = Vec::new();
    for f in 0..6 {
        expected.extend_from_slice(&s[f * 16..f * 16 + 8]);
        expected.extend_from_slice(&s[f * 16 + 8..f * 16 + 16]);
    }
    assert_eq!(payload, &expected[..], "mips must be the raw per-face chain");

    // The plane follows the cubemap's six face slots.
    let plane = &out[1].bytes;
    assert_eq!(&plane[128..], &s[96..104]);
}
