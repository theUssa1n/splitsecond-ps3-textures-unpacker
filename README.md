# Split/Second Texture Unpacking Tools (Universal)

Tools for extracting textures from both the **PC** and **console (PS3)** versions of Split/Second.

## Tools

### 1. unpack-textures-and-streamtex.exe
Main tool for handling high-res textures stored in `.streamtex` files.

- **Console (PS3) version:** select both the `.streamtex` file and its matching `.textures` file. The tool uses the metadata in `.textures` to reconstruct the raw pixels from `.streamtex` into valid DDS files with proper names.
- **PC version:**
  - If you select both files, it extracts everything with proper names.
  - If you ONLY select the `.streamtex` file (and skip the `.textures` part), it still extracts the textures because PC files have internal headers; since the names are stored in the `.textures` file, it falls back to numeric names (`0.dds`, `1.dds`, …).

Embedded (low-res) textures from `.textures` are written to an `embedded` subfolder; high-res `.streamtex` textures go into a `high-res` subfolder.

### 2. untexs.exe
Extracts low-res embedded textures directly from `.textures` files. It names them using the internal ID table, so duplicate names do not overwrite each other.

## Usage

> **Windows:** both methods work on Windows without any issues — you can click through the file-picker dialogs (rfd) or run from the command line (PowerShell / Command Prompt).

**Interactive (file pickers):**
- Combined unpacking: pick the `.streamtex` file, then the matching `.textures` file, then an output folder.
- Embedded only: pick the `.textures` file(s), then an output folder.

**Command line:**
- Combined (Full):      `./unpack-textures-and-streamtex.exe <streamtex> <textures> <outdir>`
- Combined (Blind PC): `./unpack-textures-and-streamtex.exe <streamtex> <outdir>`
- Embedded:             `./untexs.exe <textures> [<textures> ...] <outdir>`

## Formats & layout

- PS3 files are big-endian. Supported formats: `0x86` DXT1 (8 bytes/block), `0x88` DXT5 (16 bytes/block), `0xA5` A8R8G8B8 / BGRA (4 bytes/pixel), and any other non-ARGB code as a 16-byte-block DXT variant (DXT3/4/5).
- PC files are little-endian DDS payloads.

For the full on-disc layout analysis (records, catalogs, embedded data, and the `streamtex` slot model), see `Structural_Comparison.txt`.
