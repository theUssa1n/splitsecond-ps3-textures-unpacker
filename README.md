# Split/Second PS3 Texture Unpacking Tools

Tools for extracting textures from the PS3 (big-endian) version of Split/Second.
They only accept PS3 `.textures` files (`TEXS` magic); PC (`SXET`) files are
rejected with a clear error.

## Tools

### 1. unpack-textures-and-streamtex
Main tool for high-res textures stored in `.streamtex` files.

- Select both the `.streamtex` file and its matching `.textures` file. The tool
  uses the metadata in `.textures` to reconstruct the raw pixels from
  `.streamtex` into valid DDS files with proper names.
- Embedded (low-res) textures from `.textures` are written to an `embedded`
  subfolder; high-res `.streamtex` textures go to a `high-res` subfolder.

### 2. untexs
Extracts low-res embedded textures directly from `.textures` files. It names
them using the internal ID table, so duplicate names do not overwrite each
other.

## Usage

**Interactive (file pickers):**
- Combined unpacking: pick the `.streamtex` file, then the matching
  `.textures` file, then an output folder.
- Embedded only: pick `.textures` file(s), then an output folder.

**Command line:**
- Combined (Full): `./unpack-textures-and-streamtex <streamtex> <textures> <outdir>`
- Embedded:        `./untexs <textures> [<textures> ...] <outdir>`

## Formats & layout

PS3 files are big-endian. Supported texture formats:
- `0x86` DXT1 (8 bytes/block)
- `0x88` DXT5 (16 bytes/block)
- `0xA5` A8R8G8B8 / BGRA (4 bytes/pixel)
- any other non-ARGB code: 16-byte-block DXT variant (DXT3/4/5)

For the full on-disc layout (records, catalogs, embedded data, volume
swizzle, and the sequential `.streamtex` slot model), see
`Structural_Comparison.txt` and the header comment in `src/lib.rs`.

## Building

```
cargo build --release
```

The two binaries are produced under `target/release/`:
`unpack-textures-and-streamtex` and `untexs`.

## License

MIT — see `LICENSE`.
