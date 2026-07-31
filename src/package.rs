use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub fn read_flp_from_zip(path: &Path) -> Result<(String, Vec<u8>), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut found: Option<(usize, String)> = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if name.to_ascii_lowercase().ends_with(".flp") && !name.starts_with("__MACOSX") {
            found = Some((i, name));
            break;
        }
    }
    let (idx, name) = found.ok_or("zip contains no .flp file")?;

    let mut entry = archive.by_index(idx).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok((name, bytes))
}

pub fn write_zip_with_flp(
    src: &Path,
    flp_name: &str,
    flp_bytes: &[u8],
    out: &Path,
) -> Result<(), String> {
    let mut archive =
        ZipArchive::new(File::open(src).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut writer = ZipWriter::new(File::create(out).map_err(|e| e.to_string())?);

    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i).map_err(|e| e.to_string())?;
        if entry.name() == flp_name {
            continue;
        }
        writer.raw_copy_file(entry).map_err(|e| e.to_string())?;
    }

    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    writer.start_file(flp_name, options).map_err(|e| e.to_string())?;
    writer.write_all(flp_bytes).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(())
}
