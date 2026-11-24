# Split/Second PS3 Texture Unpacking Tools

Tools for extracting textures from the **console (PS3)** version of Split/Second.

> **Note for PC users:** this tool targets the PS3 format only. For the **PC** version of Split/Second, use [TEXembed](https://github.com/theUssa1n/TEXembed) instead — it opens `.textures` / `.streamtex`, previews, exports and replaces DDS textures.

## Tools

### 1. unpack-textures-and-streamtex.exe
Main tool for handling high-res textures stored in `.streamtex` files (PS3).

Select both the `.streamtex` file and its matching `.textures` file. The tool uses the metadata in `.textures` to reconstruct the raw pixels from `.streamtex` into valid DDS files with proper names.

Embedded (low-res) textures from `.textures` are written to an `embedded` subfolder; high-res `.streamtex` textures go into a `high-res` subfolder.

### 2. untexs.exe
Extracts low-res embedded textures directly from `.textures` files (PS3). It names them using the internal ID table, so duplicate names do not overwrite each other.

## Usage

> **Windows:** works on Windows without any issues — you can click through the file-picker dialogs (rfd) or run from the command line (PowerShell / Command Prompt).

**Interactive (file pickers):**
- Combined unpacking: pick the `.streamtex` file, then the matching `.textures` file, then an output folder.
- Embedded only: pick the `.textures` file(s), then an output folder.

**Command line:**
- Combined (Full): `./unpack-textures-and-streamtex.exe <streamtex> <textures> <outdir>`
- Embedded:        `./untexs.exe <textures> [<textures> ...] <outdir>`

## Formats & layout

- PS3 files are big-endian. Supported formats: `0x86` DXT1 (8 bytes/block), `0x88` DXT5 (16 bytes/block), `0xA5` A8R8G8B8 / BGRA (4 bytes/pixel), and any other non-ARGB code as a 16-byte-block DXT variant (DXT3/4/5).

For the full on-disc layout analysis (records, catalogs, embedded data, and the `streamtex` slot model), see `Structural_Comparison.txt`.
