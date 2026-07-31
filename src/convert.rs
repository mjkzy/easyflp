use crate::flp::{op, Event, Flp, Payload};
use crate::info;

pub struct Outcome {
    pub flp: Flp,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

pub const FL20_VERSION: &str = "20.8.4.2576";
pub const FL20_BUILD: u32 = 2576;

/* 20.8's registration blob is 50 bytes. newer versions write a 10-byte stand-in,
   and whether 20.8 accepts that is untested — so we ship the verified 50-byte one. */
const FL20_REGISTRATION: [u8; 50] = [
    0x70, 0x00, 0x62, 0x00, 0x63, 0x00, 0x6B, 0x00, 0x65, 0x00, 0x67, 0x00, 0x5F, 0x00, 0x5B,
    0x00, 0x5E, 0x00, 0x56, 0x00, 0x52, 0x00, 0x5D, 0x00, 0x57, 0x00, 0x58, 0x00, 0x5B, 0x00,
    0x61, 0x00, 0x3D, 0x00, 0x41, 0x00, 0x3B, 0x00, 0x40, 0x00, 0x3A, 0x00, 0x39, 0x00, 0x35,
    0x00, 0x39, 0x00, 0x00, 0x00,
];

/* opcodes 20.8 never writes. derived by byte-diffing the same project saved by 20.8.4 and 25.1.4,
   plus 0xAC (25's fixed-3-byte event). 0x68 is absent on purpose: it converts to 0x16 before
   deletion would apply. */
const POST_FL20_OPS: [u8; 19] = [
    0x29, 0x2A, 0x2B, 0x2C, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x67, 0xA5, 0xA6, 0xA7, 0xA9, 0xAA,
    0xAC, 0xD8, 0xF2, 0xF3,
];

/* every opcode observed in a 20.8 save, plus 20-era events our truth file happens not to use
   (0x95/0xCC insert colour+name, 0xC1 pattern name, 0xEF lane name, 0xD0 legacy notes).
   leftovers outside this set are reported, never deleted. */
const FL20_KNOWN_OPS: [u8; 91] = [
    0x00, 0x09, 0x0A, 0x0B, 0x11, 0x12, 0x14, 0x15, 0x16, 0x17, 0x1C, 0x1D, 0x1E, 0x1F, 0x20,
    0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x40, 0x41, 0x43, 0x45, 0x46, 0x47, 0x4A, 0x4B, 0x4C,
    0x50, 0x53, 0x55, 0x56, 0x59, 0x61, 0x62, 0x63, 0x64, 0x80, 0x83, 0x84, 0x85, 0x8A, 0x8B,
    0x8F, 0x90, 0x91, 0x92, 0x93, 0x95, 0x96, 0x9A, 0x9B, 0x9C, 0x9D, 0x9E, 0x9F, 0xA4, 0xC1,
    0xC2, 0xC3, 0xC4, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCE, 0xCF, 0xD0, 0xD1, 0xD4, 0xD5,
    0xD7, 0xDA, 0xDB, 0xDD, 0xE0, 0xE1, 0xE2, 0xE4, 0xE5, 0xE7, 0xE9, 0xEB, 0xEC, 0xED, 0xEE,
    0xF1,
];

/* 20.8 writes a fixed 4697-record 0xE1 table: a header record, then per strip the ten slot pairs
   and a fixed pid run whose tail shortens on high strips. 25 rebases targets to 0x7000 (addressing
   the "current" strip as strip 501) and drops the send/190 tail, so the table is rebuilt from
   scratch: source values where present, stock defaults elsewhere, unrecognised records appended
   after their strip's canonical run. */
fn rebuild_mixer_params(b: &[u8], rebased: &mut usize) -> Vec<u8> {
    use std::collections::BTreeMap;

    let mut existing: BTreeMap<(u16, u16, u8), i32> = BTreeMap::new();
    let mut extras: Vec<(u16, Vec<u8>)> = Vec::new();
    let mut verbatim: Vec<Vec<u8>> = Vec::new();

    for rec in b.chunks_exact(12) {
        let pid = rec[4];
        let tgt = u16::from_le_bytes([rec[6], rec[7]]);
        let val = i32::from_le_bytes(rec[8..12].try_into().unwrap());
        if tgt == 0x4000 && pid == 0 {
            continue;
        }
        let (base, strip) = if tgt >= 0x7000 {
            *rebased += 1;
            (0x7000u16, ((tgt - 0x7000) >> 6).min(126))
        } else if tgt >= 0x2000 {
            (0x2000u16, ((tgt - 0x2000) >> 6).min(126))
        } else {
            verbatim.push(rec.to_vec());
            continue;
        };
        let off = (tgt - base) & 0x3F;
        if existing.insert((strip, off, pid), val).is_some() || !canonical_pid(off, pid) {
            let nt = 0x2000 + strip * 0x40 + off;
            let mut r = rec.to_vec();
            r[6..8].copy_from_slice(&nt.to_le_bytes());
            extras.push((strip, r));
        }
    }

    let mut out = Vec::with_capacity(4697 * 12);
    let push = |pid: u8, group: u8, tgt: u16, val: i32, out: &mut Vec<u8>| {
        out.extend_from_slice(&[0, 0, 0, 0, pid, group]);
        out.extend_from_slice(&tgt.to_le_bytes());
        out.extend_from_slice(&val.to_le_bytes());
    };
    push(0, 0x00, 0x4000, 12800, &mut out);
    out.extend(verbatim.into_iter().flatten());

    for strip in 0u16..127 {
        let base = 0x2000 + strip * 0x40;
        let mut get = |off: u16, pid: u8, default: i32| {
            existing.remove(&(strip, off, pid)).unwrap_or(default)
        };
        for slot in 0u16..10 {
            let en = get(slot, 0, 1);
            let mix = get(slot, 1, 12800);
            push(0, 0x1F, base + slot, en, &mut out);
            push(1, 0x1F, base + slot, mix, &mut out);
        }
        for (pid, default) in strip_pid_run(strip) {
            let v = get(0, pid, default);
            push(pid, 0x1F, base, v, &mut out);
        }
        for (_, r) in extras.iter().filter(|(s, _)| *s == strip) {
            out.extend_from_slice(r);
        }
    }
    out
}

fn strip_pid_run(strip: u16) -> Vec<(u8, i32)> {
    let mut run = vec![
        (192u8, 12800i32),
        (193, 0),
        (194, 0),
        (208, 0),
        (209, 0),
        (210, 0),
        (216, 5777),
        (217, 33145),
        (218, 55825),
        (224, 17500),
        (225, 17500),
        (226, 17500),
    ];
    if strip <= 99 {
        run.extend([(164, 0), (165, 0), (166, 0), (167, 0), (168, 0)]);
    } else if strip <= 104 {
        run.push((168, 0));
    }
    run.push((190, 0));
    run
}

fn canonical_pid(off: u16, pid: u8) -> bool {
    if pid <= 1 {
        return off < 10;
    }
    off == 0
        && matches!(pid, 192 | 193 | 194 | 208..=210 | 216..=218 | 224..=226 | 164..=168 | 190)
}

pub fn to_fl20(src: &Flp) -> Result<Outcome, String> {
    let version = src.version().ok_or("file has no version event (0xC7)")?;
    let major = src
        .version_major()
        .ok_or_else(|| format!("cannot parse version \"{version}\""))?;
    if major <= 20 {
        return Err(format!("already v{version}, nothing to convert"));
    }

    let playlist_len = src
        .events
        .iter()
        .filter(|e| e.op == op::PLAYLIST)
        .filter_map(|e| e.blob())
        .map(|b| b.len())
        .find(|&l| l > 0)
        .unwrap_or(0);
    let clip_size = info::clip_record_size(major, playlist_len)
        .ok_or("playlist blob length matches no known clip record size")?;

    /* 0x68 replaces 0x16 in 25 at the same position in the channel param block, so an in-place
       rewrite lands the route where 20.8 expects it. a file carrying both per channel (some
       dual-write exports do this) keeps its 0x16 and drops the 0x68. */
    let mut channel_has_16 = vec![false];
    for ev in &src.events {
        match ev.op {
            op::CHANNEL_NEW => channel_has_16.push(false),
            op::CHANNEL_ROUTE => *channel_has_16.last_mut().unwrap() = true,
            _ => {}
        }
    }

    let mut notes = Vec::new();
    let mut warnings = Vec::new();
    let mut out: Vec<Event> = Vec::with_capacity(src.events.len());

    let mut chan_idx = 0usize;
    let mut deleted = 0usize;
    let mut routes_converted = 0usize;
    let mut routes_dropped = 0usize;
    let mut markers = 0usize;
    let mut d7_truncated = 0usize;
    let mut clips_converted = 0usize;
    let mut lanes_truncated = 0usize;
    let mut tables_expanded = 0usize;
    let mut params_rebased = 0usize;
    let mut e1_rebuilt = 0usize;
    let mut stretch_lost = 0usize;

    for ev in &src.events {
        match ev.op {
            o if POST_FL20_OPS.contains(&o) => deleted += 1,
            op::CHANNEL_NEW => {
                chan_idx += 1;
                out.push(ev.clone());
            }
            op::CHANNEL_ROUTE_FL25 => {
                if channel_has_16[chan_idx] {
                    routes_dropped += 1;
                } else {
                    let v = ev.value().unwrap_or(0).min(255) as u8;
                    out.push(Event { op: op::CHANNEL_ROUTE, payload: Payload::U8(v) });
                    routes_converted += 1;
                }
            }
            op::VERSION => {
                let mut b = FL20_VERSION.as_bytes().to_vec();
                b.push(0);
                out.push(Event { op: op::VERSION, payload: Payload::Blob(b) });
            }
            op::BUILD => out.push(Event { op: op::BUILD, payload: Payload::U32(FL20_BUILD) }),
            0xC8 => out.push(Event {
                op: 0xC8,
                payload: Payload::Blob(FL20_REGISTRATION.to_vec()),
            }),
            // saves carry out-of-range selected insert values (216 in both truth files); 126, the "current" strip, is the hand-verified one
            0x85 => out.push(Event {
                op: 0x85,
                payload: Payload::U32(ev.value().unwrap_or(126).min(126)),
            }),
            op::WRAPPER => {
                let mut b = ev.blob().ok_or("0xD5 without blob payload")?.to_vec();
                if b.len() >= 4 {
                    let marker = u32::from_le_bytes(b[0..4].try_into().unwrap());
                    if marker > 10 {
                        b[0..4].copy_from_slice(&10u32.to_le_bytes());
                        markers += 1;
                    }
                }
                out.push(Event { op: op::WRAPPER, payload: Payload::Blob(b) });
            }
            op::CHANNEL_DECO => {
                let b = ev.blob().ok_or("0xD7 without blob payload")?;
                if b.len() > 158 {
                    d7_truncated += 1;
                    out.push(Event {
                        op: op::CHANNEL_DECO,
                        payload: Payload::Blob(b[..158].to_vec()),
                    });
                } else {
                    out.push(ev.clone());
                }
            }
            op::PLAYLIST => {
                let b = ev.blob().ok_or("0xE9 without blob payload")?;
                if clip_size == 32 || b.is_empty() {
                    out.push(ev.clone());
                } else {
                    if b.len() % clip_size != 0 {
                        return Err(format!(
                            "playlist blob of {} bytes is not a multiple of {clip_size}",
                            b.len()
                        ));
                    }
                    let mut nb = Vec::with_capacity(b.len() / clip_size * 32);
                    for rec in b.chunks_exact(clip_size) {
                        nb.extend_from_slice(&rec[..32]);
                        clips_converted += 1;
                        if clip_size == 80 {
                            let scale =
                                f64::from_le_bytes(rec[64..72].try_into().unwrap());
                            if scale != 0.0 && (scale - 1.0).abs() > 1e-9 {
                                stretch_lost += 1;
                            }
                        }
                    }
                    out.push(Event { op: op::PLAYLIST, payload: Payload::Blob(nb) });
                }
            }
            op::LANE => {
                let b = ev.blob().ok_or("0xEE without blob payload")?;
                if b.len() > 66 {
                    lanes_truncated += 1;
                    out.push(Event { op: op::LANE, payload: Payload::Blob(b[..66].to_vec()) });
                } else {
                    out.push(ev.clone());
                }
            }
            op::ROUTE_TABLE => {
                let b = ev.blob().ok_or("0xEB without blob payload")?;
                if b.len() < 127 {
                    tables_expanded += 1;
                    let mut nb = b.to_vec();
                    nb.resize(127, 0);
                    out.push(Event { op: op::ROUTE_TABLE, payload: Payload::Blob(nb) });
                } else {
                    out.push(ev.clone());
                }
            }
            op::MIXER_PARAMS => {
                let b = ev.blob().ok_or("0xE1 without blob payload")?;
                if b.len() % 12 != 0 {
                    warnings.push(format!(
                        "mixer param blob of {} bytes is not a multiple of 12 — left unchanged",
                        b.len()
                    ));
                    out.push(ev.clone());
                } else {
                    let nb = rebuild_mixer_params(b, &mut params_rebased);
                    e1_rebuilt += 1;
                    out.push(Event { op: op::MIXER_PARAMS, payload: Payload::Blob(nb) });
                }
            }
            _ => out.push(ev.clone()),
        }
    }

    let push = |n: usize, msg: String, to: &mut Vec<String>| {
        if n > 0 {
            to.push(msg);
        }
    };
    push(1, format!("version {version} -> {FL20_VERSION}"), &mut notes);
    push(deleted, format!("deleted {deleted} post-v20 events"), &mut notes);
    push(
        routes_converted,
        format!("converted {routes_converted} channel routes 0x68 -> 0x16"),
        &mut notes,
    );
    push(
        routes_dropped,
        format!("dropped {routes_dropped} duplicate 0x68 routes (0x16 already present)"),
        &mut notes,
    );
    push(markers, format!("wrapper marker 12 -> 10 on {markers} plugins"), &mut notes);
    push(
        clips_converted,
        format!("rewrote {clips_converted} playlist clips {clip_size} -> 32 bytes"),
        &mut notes,
    );
    push(
        lanes_truncated,
        format!("truncated {lanes_truncated} lane records 70 -> 66 bytes"),
        &mut notes,
    );
    push(
        tables_expanded,
        format!("expanded {tables_expanded} mixer route tables to 127 bytes"),
        &mut notes,
    );
    push(
        e1_rebuilt,
        "rebuilt mixer param table to the v20 canonical 4697-record shape".into(),
        &mut notes,
    );
    push(
        params_rebased,
        format!("rebased {params_rebased} mixer param targets 0x7000 -> 0x2000"),
        &mut notes,
    );
    push(
        d7_truncated,
        format!("truncated {d7_truncated} channel blobs 0xD7 to 158 bytes"),
        &mut notes,
    );
    push(
        stretch_lost,
        format!("{stretch_lost} stretched audio clips lose their stretch (v20 has no per-clip scale)"),
        &mut warnings,
    );

    let leftovers: Vec<u8> = {
        let mut seen: Vec<u8> = out
            .iter()
            .map(|e| e.op)
            .filter(|o| !FL20_KNOWN_OPS.contains(o))
            .collect();
        seen.sort_unstable();
        seen.dedup();
        seen
    };
    if !leftovers.is_empty() {
        warnings.push(format!(
            "events kept that v20.8 is not known to write: {}",
            leftovers
                .iter()
                .map(|o| format!("0x{o:02X}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    Ok(Outcome {
        flp: Flp {
            format: src.format,
            n_channels: src.n_channels,
            ppq: src.ppq,
            header_raw: src.header_raw.clone(),
            events: out,
            trailing: src.trailing.clone(),
        },
        notes,
        warnings,
    })
}
