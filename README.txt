# Split/Second PS3 Texture Unpacking Tools

extracting textures from the PS3 (big-endian) version of Split/Second. These
tools only accept PS3 `.textures` files ("TEXS" magic); PC ("SXET") files are
rejected with a clear error.

### 1. unpack-textures-and-streamtex.exe
main tool for handling High-Res textures from `.streamtex` files.

- select both the `.streamtex` and its matching `.textures` file. The tool uses
  the metadata in `.textures` to reconstruct the raw pixels from `.streamtex`
  into valid DDS files with proper names. The embedded (low-res) textures from
  the `.textures` file go into an `embedded` subfolder and the high-res
  `.streamtex` textures into a `high-res` subfolder.

### 2. untexs.exe
This tool is for extracting low-res embedded textures directly from `.textures`
files. It names them correctly using the internal ID table.

1.  Run the tool you need.
2.  **For Combined Unpacking:** 
    - First, select the `.streamtex` file.
    - Second, select the matching `.textures` file.
    - Third, pick an output folder.
3.  **For Embedded Only:** Just pick the `.textures` file(s) and an output folder.

**CLI Usage:**
- Combined (Full): `./unpack-textures-and-streamtex.exe <streamtex> <textures> <outdir>`
- Embedded: `./untexs.exe <textures> [<textures> ...] <outdir>`

-------------------
utilities specifically designed for extracting textures
from the PS3 version of Split/Second. They handle both the standard `.textures`
files and the high-resolution `.streamtex` files.

1. unpack-textures-and-streamtex.exe
main tool you'll probably want to use. It
will unpack both files simultaneously, putting the textures from the
`.textures` file into a subdirectory called `embedded`, and textures from
`.streamtex` go into `high-res`. 

2. untexs.exe
unpack all the embedded textures from a `.textures` file and
name them according to their names found at the end of the file. I know some
files have duplicate names which might cause overwrites, but this tool
handles them by using the internal IDs and proper offsets to keep everything
unique.

-- also included a `Structural_Comparison.txt` file which explains how these
files are structured, especially why the PS3 version is unique compared
to PC (like the Big Endian byte order and the raw data in PS3 streamtex
files).

Simply run the program you want. 

- 'unpack-textures-and-streamtex.exe', a file picker will pop up so you
can pick your `.textures` file, then another one for the `.streamtex` file
and finally one for the output directory.

- 'untexs.exe', a file picker will pop up so you can pick your `.textures` files and an output
directory.

They can also be run command line from PowerShell or Command Prompt if you
prefer that. In that case, you can pass the paths as arguments like this:
`./unpack-textures-and-streamtex.exe Musclecar_01.streamtex Musclecar_01.textures "outdir"`
`./untexs.exe Musclecar_01.textures "outdir"`
This tool is used for PS3 files only. For the High-Res extraction, you must
select both the .textures and .streamtex files.
