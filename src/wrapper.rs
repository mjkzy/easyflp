/* The state version ladder is 7 (10.0.9), 8 (20.0.5), 10 (20.8), 12 (25). */

pub(crate) struct Chunk {
    pub id: u32,
    pub data: Vec<u8>,
}

pub(crate) fn parse_state(b: &[u8]) -> Option<(u32, Vec<Chunk>)> {
    if b.len() < 4 {
        return None;
    }
    let version = u32::from_le_bytes(b[0..4].try_into().unwrap());
    let mut chunks = Vec::new();
    let mut pos = 4usize;
    while pos + 12 <= b.len() {
        let id = u32::from_le_bytes(b[pos..pos + 4].try_into().unwrap());
        let len = u64::from_le_bytes(b[pos + 4..pos + 12].try_into().unwrap()) as usize;
        if len > b.len() - pos - 12 {
            return None;
        }
        chunks.push(Chunk { id, data: b[pos + 12..pos + 12 + len].to_vec() });
        pos += 12 + len;
    }
    if pos != b.len() {
        return None;
    }
    Some((version, chunks))
}

pub(crate) fn build_state(version: u32, chunks: &[Chunk]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&version.to_le_bytes());
    for c in chunks {
        out.extend_from_slice(&c.id.to_le_bytes());
        out.extend_from_slice(&(c.data.len() as u64).to_le_bytes());
        out.extend_from_slice(&c.data);
    }
    out
}

pub(crate) fn chunk_offset(b: &[u8], want: u32) -> Option<(usize, usize)> {
    let mut pos = 4usize;
    while pos + 12 <= b.len() {
        let id = u32::from_le_bytes(b[pos..pos + 4].try_into().unwrap());
        let len = u64::from_le_bytes(b[pos + 4..pos + 12].try_into().unwrap()) as usize;
        if len > b.len() - pos - 12 {
            return None;
        }
        if id == want {
            return Some((pos + 12, len));
        }
        pos += 12 + len;
    }
    None
}
