use std::path::{Path, PathBuf};

use crate::{convert, flp, info, package};

pub struct LoadedProject {
    pub path: PathBuf,
    pub zip_entry: Option<String>,
    pub file_size: usize,
    pub flp: flp::Flp,
    pub info: info::ProjectInfo,
    pub roundtrip_ok: bool,
}

pub struct ConvertDone {
    pub out: PathBuf,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn load(path: &Path) -> Result<LoadedProject, String> {
    let is_zip = path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false);
    let (zip_entry, bytes) = if is_zip {
        let (name, bytes) = package::read_flp_from_zip(path)?;
        (Some(name), bytes)
    } else {
        (None, std::fs::read(path).map_err(|e| e.to_string())?)
    };

    let parsed = flp::parse(&bytes)?;
    let roundtrip_ok = flp::serialize(&parsed) == bytes;
    let project_info = info::extract(&parsed);
    Ok(LoadedProject {
        path: path.to_path_buf(),
        zip_entry,
        file_size: bytes.len(),
        flp: parsed,
        info: project_info,
        roundtrip_ok,
    })
}

pub fn convert_and_write(l: &LoadedProject) -> Result<ConvertDone, String> {
    if !l.roundtrip_ok {
        return Err("parser cannot reproduce this file byte-exact — conversion disabled for safety".into());
    }
    let outcome = convert::to_fl20(&l.flp)?;
    let bytes = flp::serialize(&outcome.flp);
    let stem = l
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    let ext = if l.zip_entry.is_some() { "zip" } else { "flp" };
    let out = l.path.with_file_name(format!("{stem}_easy.{ext}"));

    if let Some(entry) = &l.zip_entry {
        package::write_zip_with_flp(&l.path, entry, &bytes, &out)?;
    } else {
        std::fs::write(&out, &bytes).map_err(|e| e.to_string())?;
    }
    Ok(ConvertDone { out, notes: outcome.notes, warnings: outcome.warnings })
}
