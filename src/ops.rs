use std::path::{Path, PathBuf};

use crate::{convert10, convert200, flp, info, package};

pub struct LoadedProject {
    pub path: PathBuf,
    pub zip_entry: Option<String>,
    pub file_size: usize,
    pub flp: flp::Flp,
    pub info: info::ProjectInfo,
    pub roundtrip_ok: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Target {
    Fl20,
    Fl10,
}

impl Target {
    pub fn suffix(self) -> &'static str {
        match self {
            Target::Fl20 => "_easy",
            Target::Fl10 => "_easy10",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Target::Fl20 => "v20",
            Target::Fl10 => "v10",
        }
    }

    pub fn applicable(self, major: u32, minor: u32) -> bool {
        match self {
            Target::Fl20 => major > 20 || (major == 20 && minor > 0),
            Target::Fl10 => major > 10,
        }
    }
}

pub struct ConvertDone {
    pub target: Target,
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

pub fn convert_and_write(l: &LoadedProject, target: Target) -> Result<ConvertDone, String> {
    if !l.roundtrip_ok {
        return Err("parser cannot reproduce this file byte-exact — conversion disabled for safety".into());
    }
    let outcome = match target {
        Target::Fl20 => convert200::to_fl200(&l.flp)?,
        Target::Fl10 => convert10::to_fl10(&l.flp)?,
    };
    let bytes = flp::serialize(&outcome.flp);
    let stem = l
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    let ext = if l.zip_entry.is_some() {
        "zip".to_string()
    } else {
        l.path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .filter(|e| e == "fst")
            .unwrap_or_else(|| "flp".into())
    };
    let out = l.path.with_file_name(format!("{stem}{}.{ext}", target.suffix()));

    if let Some(entry) = &l.zip_entry {
        package::write_zip_with_flp(&l.path, entry, &bytes, &out)?;
    } else {
        std::fs::write(&out, &bytes).map_err(|e| e.to_string())?;
    }
    Ok(ConvertDone { target, out, notes: outcome.notes, warnings: outcome.warnings })
}
