use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{anyhow, Result};
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageLevel};
use unpack_ss_utils::{make_textures_view, extract_embedded_textures, dump_texture_bundle};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let (texture_paths, output_base_path): (Vec<PathBuf>, PathBuf) = if args.len() >= 3 {
        let out = PathBuf::from(&args[args.len()-1]);
        let paths = args[1..args.len()-1].iter()
            .map(Path::new)
            .filter(|p| p.is_file())
            .map(|p| p.to_path_buf())
            .collect();
        (paths, out)
    } else if args.len() == 2 {
        let paths = vec![PathBuf::from(&args[1])];
        let out = match FileDialog::new()
            .set_title("Select Output Folder")
            .pick_folder()
        {
            Some(p) => p,
            None => return,
        };
        (paths, out)
    } else {
        let paths = match FileDialog::new()
            .set_title("Select PS3 .textures files")
            .add_filter("Split/Second PS3 textures", &["textures"])
            .pick_files()
        {
            Some(paths) => paths,
            None => return,
        };
        let out = match FileDialog::new()
            .set_title("Select Output Folder")
            .pick_folder()
        {
            Some(p) => p,
            None => return,
        };
        (paths, out)
    };

    let mut success_count = 0;
    let mut fail_count = 0;

    for texture_path in &texture_paths {
        println!("Processing: {}...", texture_path.display());

        match unpack_textures_only(texture_path.clone(), output_base_path.clone()) {
            Ok(count) => {
                println!("Extracted {} textures to: {}", count, output_base_path.display());
                success_count += 1;
            },
            Err(e) => {
                eprintln!("Error unpacking {}: {}", texture_path.display(), e);
                fail_count += 1;
                MessageDialog::new()
                    .set_level(MessageLevel::Error)
                    .set_buttons(MessageButtons::Ok)
                    .set_title("Unpack Error")
                    .set_description(&format!("Failed to unpack {}:\n{}", texture_path.display(), e))
                    .show();
            }
        }
    }

    let final_msg = if fail_count == 0 {
        format!("Successfully processed {} files.", success_count)
    } else {
        format!("Finished with {} successes and {} failures.", success_count, fail_count)
    };

    MessageDialog::new()
        .set_level(if fail_count == 0 { MessageLevel::Info } else { MessageLevel::Warning })
        .set_buttons(MessageButtons::Ok)
        .set_title("Process Finished")
        .set_description(&final_msg)
        .show();
}

pub fn unpack_textures_only(textures_path: PathBuf, output_directory_path: PathBuf) -> Result<usize> {
    let textures_bytes = fs::read(&textures_path)?;

    if textures_bytes.len() < 4 {
        return Err(anyhow!("File too short"));
    }
    if &textures_bytes[0..4] == b"SXET" {
        return Err(anyhow!("PC .textures file - this tool only supports PS3 (big-endian) files"));
    }
    if &textures_bytes[0..4] != b"TEXS" {
        return Err(anyhow!("Invalid magic: {:?}", &textures_bytes[0..4]));
    }

    let view = make_textures_view(&textures_bytes)?;
    let bundle = extract_embedded_textures(&view)?;
    let count = bundle.len();
    dump_texture_bundle(bundle, output_directory_path)?;
    Ok(count)
}
