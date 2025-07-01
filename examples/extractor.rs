use std::{fs, path::Path};

use cfb::CompoundFile;
use clap::Parser;
use tiny_msg::{MsgError, MsgReader};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input .msg file
    file: String,

    /// Output directory
    #[arg(short, long)]
    out_dir: String,
}

fn main() -> Result<(), MsgError> {
    let args = Args::parse();
    let out_dir = Path::new(&args.out_dir);
    if !out_dir.is_dir() {
        fs::create_dir_all(out_dir)?;
    }
    let file = std::fs::File::open(&args.file)?;
    let mut compound_file = CompoundFile::open(file).unwrap();
    extract_attachments_recursively(&mut compound_file, Path::new("/"), out_dir)?;
    Ok(())
}

fn extract_attachments_recursively(
    cfb: &mut CompoundFile<std::fs::File>,
    path: &Path,
    out_dir: &Path,
) -> Result<(), MsgError> {
    let mut msg = MsgReader::new(cfb, path);

    // Extract simple attachments
    for attachment in msg.attachments()? {
        let mut full_path = out_dir.join(&attachment.name);
        if full_path.exists() {
            // Avoid overwriting files with the same name
            let mut counter = 1;
            loop {
                let mut new_name = full_path.file_stem().unwrap().to_str().unwrap().to_string();
                new_name.push_str(&format!(" ({counter})",));
                if let Some(ext) = full_path.extension() {
                    new_name.push('.');
                    new_name.push_str(ext.to_str().unwrap());
                }
                full_path.set_file_name(new_name);
                if !full_path.exists() {
                    break;
                }
                counter += 1;
            }
        }
        fs::write(&full_path, &attachment.data)?;
        println!("Saved attachment to {}", full_path.to_str().unwrap());
    }

    // Recurse into embedded messages
    for embedded_path in msg.embedded_messages()? {
        let mut msg_reader = MsgReader::new(cfb, &embedded_path);
        let subject = msg_reader
            .pr_subject()
            .unwrap_or_else(|_| "Untitled".to_string());
        let new_out_dir = out_dir.join(subject);
        if !new_out_dir.is_dir() {
            fs::create_dir_all(&new_out_dir)?;
        }
        extract_attachments_recursively(cfb, &embedded_path, &new_out_dir)?;
    }

    Ok(())
}
