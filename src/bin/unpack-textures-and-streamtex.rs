use std::fs;
use std::path::PathBuf;
use anyhow::{anyhow, Result};
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageLevel};
use unpack_ss_utils::{
    make_textures_view, extract_embedded_textures, extract_streamtex_textures,
    dump_texture_bundle,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let (streamtex_path, textures_path, output_base_path): (PathBuf, PathBuf, PathBuf) = if args.len() == 3 {
        // CLI: streamtex output
        (PathBuf::from(&args[1]), PathBuf::from(""), PathBuf::from(&args[2]))
    } else if args.len() >= 4 {
        // CLI: streamtex textures output
        (PathBuf::from(&args[1]), PathBuf::from(&args[2]), PathBuf::from(&args[3]))
    } else {
        let stream = match FileDialog::new()
            .set_title("Select PS3 .streamtex file")
            .add_filter("Streamtex", &["streamtex"])
            .pick_file()
        {
            Some(p) => p,
            None => return,
        };

        let tex = match FileDialog::new()
            .set_title("Select PS3 .textures file (required for console streamtex)")
            .add_filter("Textures", &["textures"])
            .pick_file()
        {
            Some(p) => p,
            None => return,
        };

        let out = match FileDialog::new()
            .set_title("Select Output Folder")
            .pick_folder()
        {
            Some(p) => p,
            None => return,
        };
        (stream, tex, out)
    };

    println!("Processing streamtex: {}...", streamtex_path.display());

    match process_combined(streamtex_path, textures_path, output_base_path.clone()) {
        Ok((embedded_count, stream_count)) => {
            println!(
                "Extracted {} embedded and {} high-res textures to: {}",
                embedded_count,
                stream_count,
                output_base_path.display()
            );
            MessageDialog::new()
                .set_level(MessageLevel::Info)
                .set_buttons(MessageButtons::Ok)
                .set_title("Process Finished")
                .set_description(&format!(
                    "Extracted {} embedded and {} high-res textures.\nOutput: {}",
                    embedded_count,
                    stream_count,
                    output_base_path.display()
                ))
                .show();
        },
        Err(e) => {
            eprintln!("Error: {}", e);
            MessageDialog::new()
                .set_level(MessageLevel::Error)
                .set_buttons(MessageButtons::Ok)
                .set_title("Unpack Error")
                .set_description(&format!("Failed to unpack:\n{}", e))
                .show();
        }
    }
}

fn process_combined(
    streamtex_path: PathBuf,
    textures_path: PathBuf,
    output_dir: PathBuf,
) -> Result<(usize, usize)> {
    if textures_path.to_string_lossy().is_empty() || !textures_path.exists() {
        return Err(anyhow!(
            "The matching PS3 .textures file is required for console files"
        ));
    }

    let tex_bytes = fs::read(&textures_path)?;
    if tex_bytes.len() < 4 {
        return Err(anyhow!("Textures file too short"));
    }
    if &tex_bytes[0..4] == b"SXET" {
        return Err(anyhow!(
            "PC .textures file - this tool only supports PS3 (big-endian) files"
        ));
    }
    if &tex_bytes[0..4] != b"TEXS" {
        return Err(anyhow!("Invalid textures magic: {:?}", &tex_bytes[0..4]));
    }

    let view = make_textures_view(&tex_bytes)?;

    // 1. Embedded textures (low-res mip chains inside the .textures file).
    let embedded_bundle = extract_embedded_textures(&view)?;
    let embedded_count = embedded_bundle.len();
    dump_texture_bundle(embedded_bundle, output_dir.join("embedded"))?;

    // 2. High-res streamtex (raw data inside the .streamtex file). The
    // .streamtex file is only required when the .textures file references
    // streamtex entries.
    let stream_bundle = if streamtex_path.exists() {
        let stream_bytes = fs::read(&streamtex_path)?;
        extract_streamtex_textures(&view, &stream_bytes)?
    } else {
        extract_streamtex_textures(&view, &[])?
    };
    let stream_count = stream_bundle.len();
    dump_texture_bundle(stream_bundle, output_dir.join("high-res"))?;

    Ok((embedded_count, stream_count))
}
