#[derive(Clone, PartialEq)]
pub enum Payload {
    U8(u8),
    U16(u16),
    U32(u32),
    Fixed3([u8; 3]),
    Blob(Vec<u8>),
}

#[derive(Clone, PartialEq)]
pub struct Event {
    pub op: u8,
    pub payload: Payload,
}

impl Event {
    pub fn value(&self) -> Option<u32> {
        match self.payload {
            Payload::U8(v) => Some(v as u32),
            Payload::U16(v) => Some(v as u32),
            Payload::U32(v) => Some(v),
            _ => None,
        }
    }

    pub fn blob(&self) -> Option<&[u8]> {
        match &self.payload {
            Payload::Blob(b) => Some(b),
            _ => None,
        }
    }
}

pub struct Flp {
    pub format: u16,
    pub n_channels: u16,
    pub ppq: u16,
    pub header_raw: Vec<u8>,
    pub events: Vec<Event>,
    pub trailing: Vec<u8>,
}

pub mod op {
    pub const TIMESIG_NUM: u8 = 0x11;
    pub const TIMESIG_DEN: u8 = 0x12;
    pub const CHANNEL_KIND: u8 = 0x15;
    pub const CHANNEL_ROUTE: u8 = 0x16;
    pub const CHANNEL_NEW: u8 = 0x40;
    pub const PATTERN_NEW: u8 = 0x41;
    pub const SLOT_CLOSE: u8 = 0x62;
    pub const CHANNEL_ROUTE_FL25: u8 = 0x68;
    pub const TEMPO: u8 = 0x9C;
    pub const BUILD: u8 = 0x9F;
    pub const PATTERN_NAME: u8 = 0xC1;
    pub const TITLE: u8 = 0xC2;
    pub const SAMPLE_PATH: u8 = 0xC4;
    pub const VERSION: u8 = 0xC7;
    pub const PLUGIN_INTERNAL_NAME: u8 = 0xC9;
    pub const NAME: u8 = 0xCB;
    pub const WRAPPER: u8 = 0xD5;
    pub const CHANNEL_DECO: u8 = 0xD7;
    pub const NOTES: u8 = 0xE0;
    pub const MIXER_PARAMS: u8 = 0xE1;
    pub const AUTOMATION_LINK: u8 = 0xE3;
    pub const PLAYLIST: u8 = 0xE9;
    pub const ROUTE_TABLE: u8 = 0xEB;
    pub const INSERT_FLAGS: u8 = 0xEC;
    pub const LANE: u8 = 0xEE;
}

pub fn parse(bytes: &[u8]) -> Result<Flp, String> {
    let header_len = chunk_len(bytes, 0, b"FLhd")?;
    let header_raw = bytes
        .get(8..8 + header_len)
        .ok_or("FLhd length exceeds file size")?
        .to_vec();
    if header_raw.len() < 6 {
        return Err("FLhd payload shorter than 6 bytes".into());
    }

    let data_off = 8 + header_len;
    let data_len = chunk_len(bytes, data_off, b"FLdt")?;
    let data = bytes
        .get(data_off + 8..data_off + 8 + data_len)
        .ok_or("FLdt length exceeds file size")?;
    let trailing = bytes[data_off + 8 + data_len..].to_vec();

    let mut events = Vec::new();
    let mut p = 0usize;
    while p < data.len() {
        let op = data[p];
        p += 1;
        let payload = if op == 0xAC {
            // 25's one violation of the opcode-range encoding is that 0xAC sits in the u32 range but carries a fixed 3 byte payload
            let b = take(data, p, 3)?;
            p += 3;
            Payload::Fixed3([b[0], b[1], b[2]])
        } else if op < 0x40 {
            let b = take(data, p, 1)?;
            p += 1;
            Payload::U8(b[0])
        } else if op < 0x80 {
            let b = take(data, p, 2)?;
            p += 2;
            Payload::U16(u16::from_le_bytes([b[0], b[1]]))
        } else if op < 0xC0 {
            let b = take(data, p, 4)?;
            p += 4;
            Payload::U32(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        } else {
            let (len, adv) = read_varint(data, p)?;
            p += adv;
            let b = take(data, p, len)?;
            p += len;
            Payload::Blob(b.to_vec())
        };
        events.push(Event { op, payload });
    }

    Ok(Flp {
        format: u16::from_le_bytes([header_raw[0], header_raw[1]]),
        n_channels: u16::from_le_bytes([header_raw[2], header_raw[3]]),
        ppq: u16::from_le_bytes([header_raw[4], header_raw[5]]),
        header_raw,
        events,
        trailing,
    })
}

pub fn serialize(flp: &Flp) -> Vec<u8> {
    let mut data = Vec::new();
    for ev in &flp.events {
        data.push(ev.op);
        match &ev.payload {
            Payload::U8(v) => data.push(*v),
            Payload::U16(v) => data.extend_from_slice(&v.to_le_bytes()),
            Payload::U32(v) => data.extend_from_slice(&v.to_le_bytes()),
            Payload::Fixed3(b) => data.extend_from_slice(b),
            Payload::Blob(b) => {
                write_varint(&mut data, b.len());
                data.extend_from_slice(b);
            }
        }
    }

    let mut out = Vec::with_capacity(16 + flp.header_raw.len() + data.len());
    out.extend_from_slice(b"FLhd");
    out.extend_from_slice(&(flp.header_raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&flp.header_raw);
    out.extend_from_slice(b"FLdt");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out.extend_from_slice(&flp.trailing);
    out
}

impl Flp {
    pub fn version(&self) -> Option<String> {
        self.events
            .iter()
            .find(|e| e.op == op::VERSION)
            .and_then(|e| e.blob())
            .map(asciiz)
    }

    pub fn version_major(&self) -> Option<u32> {
        self.version()?.split('.').next()?.parse().ok()
    }
}

pub fn asciiz(blob: &[u8]) -> String {
    let end = blob.iter().position(|&b| b == 0).unwrap_or(blob.len());
    String::from_utf8_lossy(&blob[..end]).into_owned()
}

pub fn utf16z(blob: &[u8]) -> String {
    let mut units = Vec::new();
    let mut i = 0;
    while i + 2 <= blob.len() {
        let u = u16::from_le_bytes([blob[i], blob[i + 1]]);
        if u == 0 {
            break;
        }
        units.push(u);
        i += 2;
    }
    String::from_utf16_lossy(&units)
}

fn chunk_len(bytes: &[u8], off: usize, magic: &[u8; 4]) -> Result<usize, String> {
    let hdr = bytes
        .get(off..off + 8)
        .ok_or_else(|| format!("file too short for {} chunk", String::from_utf8_lossy(magic)))?;
    if &hdr[0..4] != magic {
        return Err(format!(
            "missing {} chunk (not an FLP?)",
            String::from_utf8_lossy(magic)
        ));
    }
    Ok(u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize)
}

fn take(data: &[u8], p: usize, n: usize) -> Result<&[u8], String> {
    data.get(p..p + n)
        .ok_or_else(|| "event stream truncated".to_string())
}

fn read_varint(data: &[u8], mut p: usize) -> Result<(usize, usize), String> {
    let mut len = 0usize;
    let mut shift = 0u32;
    let start = p;
    loop {
        let b = *data.get(p).ok_or("varint runs past end of stream")?;
        p += 1;
        len |= ((b & 0x7F) as usize) << shift;
        shift += 7;
        if b & 0x80 == 0 {
            return Ok((len, p - start));
        }
        if shift > 35 {
            return Err("varint too long".into());
        }
    }
}

fn write_varint(out: &mut Vec<u8>, mut len: usize) {
    loop {
        let b = (len & 0x7F) as u8;
        len >>= 7;
        if len == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}
