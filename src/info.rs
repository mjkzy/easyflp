use std::collections::{BTreeMap, HashSet};

use crate::flp::{self, op, Flp, Payload};

pub struct ChannelInfo {
    pub iid: u32,
    pub kind: u8,
    pub name: String,
    pub detail: String,
}

pub struct PatternInfo {
    pub id: u32,
    pub name: Option<String>,
    pub notes: usize,
}

pub struct EffectInfo {
    pub insert: usize,
    pub slot: usize,
    pub name: String,
}

pub struct ProjectInfo {
    pub version: String,
    pub major: u32,
    pub build: Option<u32>,
    pub title: String,
    pub tempo: f64,
    pub timesig: (u32, u32),
    pub ppq: u16,
    pub event_count: usize,
    pub channels: Vec<ChannelInfo>,
    pub patterns: Vec<PatternInfo>,
    pub pattern_clips: usize,
    pub audio_clips: usize,
    pub lanes_used: usize,
    pub clip_record_size: Option<usize>,
    pub effects: Vec<EffectInfo>,
    pub wrapper_marker: Option<u32>,
    pub lane_record_size: Option<usize>,
    pub route_table_size: Option<usize>,
    pub route_style: &'static str,
    pub mixer_param_base: Option<u16>,
    pub op_histogram: Vec<(u8, &'static str, usize)>,
}

pub fn kind_name(kind: u8) -> &'static str {
    match kind {
        0 => "sampler",
        2 => "plugin",
        3 => "layer",
        4 => "audio clip",
        5 => "automation",
        _ => "unknown",
    }
}

pub fn clip_record_size(major: u32, playlist_len: usize) -> Option<usize> {
    let claimed: usize = if major >= 25 {
        80
    } else if major >= 21 {
        60
    } else {
        32
    };
    if playlist_len == 0 || playlist_len % claimed == 0 {
        return Some(claimed);
    }
    [80usize, 60, 32].iter().copied().find(|s| playlist_len % s == 0)
}

fn wrapper_name(blob: &[u8]) -> Option<String> {
    if blob.len() < 4 {
        return None;
    }
    let mut pos = 4usize;
    while pos + 12 <= blob.len() {
        let id = u32::from_le_bytes(blob[pos..pos + 4].try_into().unwrap());
        let len = u64::from_le_bytes(blob[pos + 4..pos + 12].try_into().unwrap()) as usize;
        if len > blob.len() - pos - 12 {
            return None;
        }
        if id == 54 {
            return Some(String::from_utf8_lossy(&blob[pos + 12..pos + 12 + len]).into_owned());
        }
        pos += 12 + len;
    }
    None
}

pub fn extract(flp: &Flp) -> ProjectInfo {
    let mut info = ProjectInfo {
        version: flp.version().unwrap_or_else(|| "unknown".into()),
        major: flp.version_major().unwrap_or(0),
        build: None,
        title: String::new(),
        tempo: 0.0,
        timesig: (4, 4),
        ppq: flp.ppq,
        event_count: flp.events.len(),
        channels: Vec::new(),
        patterns: Vec::new(),
        pattern_clips: 0,
        audio_clips: 0,
        lanes_used: 0,
        clip_record_size: None,
        effects: Vec::new(),
        wrapper_marker: None,
        lane_record_size: None,
        route_table_size: None,
        route_style: "none",
        mixer_param_base: None,
        op_histogram: Vec::new(),
    };

    let mut patterns: BTreeMap<u32, PatternInfo> = BTreeMap::new();
    let mut current_pattern: Option<u32> = None;
    let mut lanes: HashSet<u16> = HashSet::new();
    let mut has16 = false;
    let mut has68 = false;

    let mut in_mixer = false;
    let mut insert_idx: isize = -1;
    let mut slot_idx = 0usize;
    let mut slot_internal: Option<String> = None;
    let mut slot_display: Option<String> = None;
    let mut slot_wrapper: Option<String> = None;

    struct PendingChannel {
        iid: u32,
        kind: u8,
        name: Option<String>,
        internal: Option<String>,
        wrapper: Option<String>,
        sample: Option<String>,
    }
    let mut chan: Option<PendingChannel> = None;
    let flush_chan = |c: PendingChannel, out: &mut Vec<ChannelInfo>| {
        let name = c
            .name
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| c.wrapper.clone())
            .or_else(|| c.internal.clone())
            .unwrap_or_default();
        let detail = match c.kind {
            0 | 4 => c
                .sample
                .as_deref()
                .map(|p| p.rsplit(['\\', '/']).next().unwrap_or(p).to_string())
                .unwrap_or_default(),
            2 => c.wrapper.or(c.internal).unwrap_or_default(),
            _ => String::new(),
        };
        out.push(ChannelInfo { iid: c.iid, kind: c.kind, name, detail });
    };

    for ev in &flp.events {
        match ev.op {
            op::BUILD => info.build = ev.value(),
            op::TEMPO => info.tempo = ev.value().unwrap_or(0) as f64 / 1000.0,
            op::TIMESIG_NUM => info.timesig.0 = ev.value().unwrap_or(4),
            op::TIMESIG_DEN => info.timesig.1 = ev.value().unwrap_or(4),
            op::TITLE => {
                if let Some(b) = ev.blob() {
                    info.title = flp::utf16z(b);
                }
            }
            op::CHANNEL_NEW => {
                if let Some(c) = chan.take() {
                    flush_chan(c, &mut info.channels);
                }
                chan = Some(PendingChannel {
                    iid: ev.value().unwrap_or(0),
                    kind: 0,
                    name: None,
                    internal: None,
                    wrapper: None,
                    sample: None,
                });
            }
            op::CHANNEL_KIND => {
                if let Some(c) = chan.as_mut() {
                    c.kind = ev.value().unwrap_or(0) as u8;
                }
            }
            op::CHANNEL_ROUTE => has16 = true,
            op::CHANNEL_ROUTE_FL25 => has68 = true,
            op::SAMPLE_PATH => {
                if let (Some(c), Some(b)) = (chan.as_mut(), ev.blob()) {
                    c.sample = Some(flp::utf16z(b));
                }
            }
            op::PLUGIN_INTERNAL_NAME => {
                if let Some(b) = ev.blob() {
                    let s = flp::utf16z(b);
                    if in_mixer {
                        slot_internal = Some(s).filter(|s| !s.is_empty());
                    } else if let Some(c) = chan.as_mut() {
                        c.internal = Some(s).filter(|s| !s.is_empty());
                    }
                }
            }
            op::NAME => {
                if let Some(b) = ev.blob() {
                    let s = flp::utf16z(b);
                    if in_mixer {
                        slot_display = Some(s).filter(|s| !s.is_empty());
                    } else if let Some(c) = chan.as_mut() {
                        c.name = Some(s);
                    }
                }
            }
            op::WRAPPER => {
                if let Some(b) = ev.blob() {
                    if info.wrapper_marker.is_none() && b.len() >= 4 {
                        info.wrapper_marker =
                            Some(u32::from_le_bytes(b[0..4].try_into().unwrap()));
                    }
                    let name = wrapper_name(b);
                    if in_mixer {
                        slot_wrapper = name;
                    } else if let Some(c) = chan.as_mut() {
                        c.wrapper = name;
                    }
                }
            }
            op::PATTERN_NEW => {
                let id = ev.value().unwrap_or(0);
                current_pattern = Some(id);
                patterns
                    .entry(id)
                    .or_insert(PatternInfo { id, name: None, notes: 0 });
            }
            op::PATTERN_NAME => {
                if let (Some(id), Some(b)) = (current_pattern, ev.blob()) {
                    if let Some(p) = patterns.get_mut(&id) {
                        p.name = Some(flp::utf16z(b));
                    }
                }
            }
            op::NOTES => {
                if let (Some(id), Some(b)) = (current_pattern, ev.blob()) {
                    if let Some(p) = patterns.get_mut(&id) {
                        p.notes += b.len() / 24;
                    }
                }
            }
            op::PLAYLIST => {
                if let Some(b) = ev.blob() {
                    let size = clip_record_size(info.major, b.len());
                    info.clip_record_size = info.clip_record_size.or(size);
                    if let Some(s) = size {
                        for rec in b.chunks_exact(s) {
                            let item = u16::from_le_bytes([rec[6], rec[7]]);
                            if item >= 20480 {
                                info.pattern_clips += 1;
                            } else {
                                info.audio_clips += 1;
                            }
                            lanes.insert(u16::from_le_bytes([rec[12], rec[13]]));
                        }
                    }
                }
            }
            op::LANE => {
                if info.lane_record_size.is_none() {
                    info.lane_record_size = ev.blob().map(|b| b.len());
                }
            }
            op::ROUTE_TABLE => {
                if info.route_table_size.is_none() {
                    info.route_table_size = ev.blob().map(|b| b.len());
                }
            }
            op::INSERT_FLAGS => {
                if let Some(c) = chan.take() {
                    flush_chan(c, &mut info.channels);
                }
                in_mixer = true;
                insert_idx += 1;
                slot_idx = 0;
                slot_internal = None;
                slot_display = None;
                slot_wrapper = None;
            }
            op::SLOT_CLOSE => {
                if in_mixer {
                    if slot_internal.is_some() || slot_wrapper.is_some() {
                        info.effects.push(EffectInfo {
                            insert: insert_idx.max(0) as usize,
                            slot: slot_idx,
                            name: slot_wrapper
                                .take()
                                .or(slot_display.take())
                                .or(slot_internal.take())
                                .unwrap_or_default(),
                        });
                    }
                    slot_internal = None;
                    slot_display = None;
                    slot_wrapper = None;
                    slot_idx += 1;
                }
            }
            op::MIXER_PARAMS => {
                if let Some(b) = ev.blob() {
                    for rec in b.chunks_exact(12) {
                        let tgt = u16::from_le_bytes([rec[6], rec[7]]);
                        if tgt != 0x4000 {
                            info.mixer_param_base =
                                Some(if tgt >= 0x7000 { 0x7000 } else { 0x2000 });
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(c) = chan.take() {
        flush_chan(c, &mut info.channels);
    }

    info.route_style = match (has16, has68) {
        (true, true) => "0x16 + 0x68 (mixed)",
        (true, false) => "0x16 (v20/21/24)",
        (false, true) => "0x68 (v25)",
        (false, false) => "none",
    };
    info.patterns = patterns.into_values().collect();
    info.lanes_used = lanes.len();

    let mut hist: BTreeMap<u8, usize> = BTreeMap::new();
    for ev in &flp.events {
        *hist.entry(ev.op).or_insert(0) += 1;
    }
    info.op_histogram = flp
        .events
        .iter()
        .map(|e| {
            (
                e.op,
                match e.payload {
                    Payload::U8(_) => "u8",
                    Payload::U16(_) => "u16",
                    Payload::U32(_) => "u32",
                    Payload::Fixed3(_) => "3-byte",
                    Payload::Blob(_) => "blob",
                },
            )
        })
        .collect::<BTreeMap<u8, &'static str>>()
        .into_iter()
        .map(|(op, ty)| (op, ty, hist[&op]))
        .collect();

    info
}
