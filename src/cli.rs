use std::path::PathBuf;

use easyflp::{info, ops};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("info") | Some("--info") => cmd_info(args.get(1).map(PathBuf::from)),
        Some("convert") | Some("--convert") => {
            let flag = |names: &[&str]| args.iter().skip(1).any(|a| names.contains(&a.as_str()));
            let target = if flag(&["--fl10", "--v10"]) {
                ops::Target::Fl10
            } else if flag(&["--fl21", "--v21"]) {
                ops::Target::Fl21
            } else if flag(&["--fl24", "--v24"]) {
                ops::Target::Fl24
            } else {
                ops::Target::Fl20
            };
            let path = args.iter().skip(1).find(|a| !a.starts_with("--")).map(PathBuf::from);
            cmd_convert(path, target)
        }
        Some("gui") => cmd_gui(args.get(1)),
        Some(p) if PathBuf::from(p).exists() => cmd_gui(args.first()),
        _ => {
            usage();
            2
        }
    };
    std::process::exit(code);
}

fn usage() {
    eprintln!("easyflp-cli {}", env!("CARGO_PKG_VERSION"));
    eprintln!();
    eprintln!("usage:");
    eprintln!("  easyflp-cli info <file.flp|file.fst|file.zip>     print project information");
    eprintln!("  easyflp-cli convert <file.flp|file.fst|file.zip>  write <name>_easy (v20.0.5) next to the input");
    eprintln!("  easyflp-cli convert --fl24 <file>                 write <name>_easy24 (v24.2.2) from a v25/v26 save");
    eprintln!("  easyflp-cli convert --fl21 <file>                 write <name>_easy21 (v21.2.3) from a v24/v25/v26 save");
    eprintln!("  easyflp-cli convert --fl10 <file>                 write <name>_easy10 (v10.0.9) — experimental");
    eprintln!("  easyflp-cli gui [file]                            launch the graphical app (easyflp)");
}

fn cmd_gui(path: Option<&String>) -> i32 {
    let name = if cfg!(windows) { "easyflp.exe" } else { "easyflp" };
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.exists());
    let Some(exe) = exe else {
        eprintln!("{name} not found next to this executable");
        return 1;
    };
    let mut cmd = std::process::Command::new(exe);
    if let Some(p) = path {
        cmd.arg(p);
    }
    match cmd.spawn() {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn cmd_convert(path: Option<PathBuf>, target: ops::Target) -> i32 {
    let Some(path) = path else {
        usage();
        return 2;
    };
    let loaded = match ops::load(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    match ops::convert_and_write(&loaded, target) {
        Ok(done) => {
            println!("wrote {}", done.out.display());
            for n in &done.notes {
                println!("- {n}");
            }
            for w in &done.warnings {
                println!("! {w}");
            }
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn cmd_info(path: Option<PathBuf>) -> i32 {
    let Some(path) = path else {
        usage();
        return 2;
    };
    let l = match ops::load(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let i = &l.info;

    println!();
    if let Some(entry) = &l.zip_entry {
        println!("zip entry  {entry}");
    }
    if !l.flp.is_project() {
        println!("kind       {}", l.flp.kind_name());
    }
    println!(
        "version    {}{}",
        i.version,
        i.build.map(|b| format!("  build {b}")).unwrap_or_default()
    );
    if !i.title.is_empty() {
        println!("title      {}", i.title);
    }
    println!("tempo      {:.3} bpm", i.tempo);
    println!("time sig   {}/{}", i.timesig.0, i.timesig.1);
    println!("ppq        {}", i.ppq);
    println!("events     {}", i.event_count);
    println!("routing    {}", i.route_style);
    println!(
        "playlist   {} pattern clips, {} audio clips on {} lanes",
        i.pattern_clips, i.audio_clips, i.lanes_used
    );
    if !i.channels.is_empty() {
        println!("channels   {}", i.channels.len());
        for c in &i.channels {
            let mut line = format!("  {:>3}  {:<11} {}", c.iid, info::kind_name(c.kind), c.name);
            if !c.detail.is_empty() && c.detail != c.name {
                line.push_str(&format!("  -  {}", c.detail));
            }
            println!("{line}");
        }
    }
    let used: Vec<_> = i.patterns.iter().filter(|p| p.notes > 0 || p.name.is_some()).collect();
    if !used.is_empty() {
        println!("patterns   {}", used.len());
        for p in used {
            println!(
                "  {:>3}  {:<24} {} notes",
                p.id,
                p.name.clone().unwrap_or_else(|| format!("Pattern {}", p.id)),
                p.notes
            );
        }
    }
    if !i.effects.is_empty() {
        println!("effects    {}", i.effects.len());
        for e in &i.effects {
            println!("  insert {:>3}  slot {}  {}", e.insert, e.slot, e.name);
        }
    }
    if !l.roundtrip_ok {
        println!("roundtrip  FAILED - conversion is disabled for this file");
    }
    0
}
