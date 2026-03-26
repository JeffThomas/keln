use std::collections::HashMap;
use super::ir::{Constant, KelnFn, KelnModule, TagTable, RecordLayoutTable, ConstantTable};

// =============================================================================
// .kbc binary format constants
// =============================================================================

pub const MAGIC: [u8; 4] = [0x4B, 0x45, 0x4C, 0x4E]; // "KELN"
pub const FORMAT_VERSION: u16 = 9;

pub const FLAG_ASYNC:      u16 = 0x01;
pub const FLAG_DEBUG_INFO: u16 = 0x02;
pub const FLAG_HAS_ENTRY:  u16 = 0x04;

pub const NO_ENTRY: u32 = 0xFFFF;

// =============================================================================
// Codec error
// =============================================================================

#[derive(Debug)]
pub struct CodecError(pub String);

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "codec error: {}", self.0)
    }
}

impl CodecError {
    fn new(msg: impl Into<String>) -> Self {
        CodecError(msg.into())
    }
}

// =============================================================================
// Encode
// =============================================================================

/// Encode a `KelnModule` to `.kbc` bytes.
///
/// - `flags`: OR of `FLAG_*` constants (caller sets `FLAG_ASYNC`, `FLAG_DEBUG_INFO`).
///   `FLAG_HAS_ENTRY` is set automatically when `entry` is `Some`.
/// - `entry`: index into `module.fns` of the entry-point function, or `None` for a library.
pub fn encode(module: &KelnModule, flags: u16, entry: Option<usize>) -> Result<Vec<u8>, CodecError> {
    let mut buf = Vec::<u8>::new();

    // ---- header ----
    buf.extend_from_slice(&MAGIC);
    buf.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    let flags = if entry.is_some() { flags | FLAG_HAS_ENTRY } else { flags };
    buf.extend_from_slice(&flags.to_le_bytes());

    // ---- const_table: u32 byte-length + bincode(Vec<Constant>) ----
    write_section(&mut buf, &module.constants.entries)?;

    // ---- tag_table: u32 byte-length + bincode(Vec<String>) ----
    write_section(&mut buf, &module.tags.names)?;

    // ---- layout_table: u32 byte-length + bincode((&layouts, &by_idx)) ----
    let layout_pair = (&module.layouts.layouts, &module.layouts.by_idx);
    write_section(&mut buf, &layout_pair)?;

    // ---- fn_table: u32 byte-length + bincode((&fns, &fn_index)) ----
    let fn_pair = (&module.fns, &module.fn_index);
    write_section(&mut buf, &fn_pair)?;

    // ---- entry_point: u32 ----
    let ep = entry.map(|e| e as u32).unwrap_or(NO_ENTRY);
    buf.extend_from_slice(&ep.to_le_bytes());

    Ok(buf)
}

fn write_section<T: serde::Serialize>(buf: &mut Vec<u8>, value: &T) -> Result<(), CodecError> {
    let bytes = bincode::serialize(value)
        .map_err(|e| CodecError::new(format!("serialize error: {}", e)))?;
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(&bytes);
    Ok(())
}

// =============================================================================
// Decode
// =============================================================================

/// Decode `.kbc` bytes into a `KelnModule` and optional entry-point index.
///
/// Returns `(module, flags, entry)` where `entry` is `None` for library modules.
pub fn decode(data: &[u8]) -> Result<(KelnModule, u16, Option<usize>), CodecError> {
    // ---- header ----
    if data.len() < 8 {
        return Err(CodecError::new("file too short for header"));
    }
    let magic = &data[0..4];
    if magic != MAGIC {
        return Err(CodecError::new(format!(
            "bad magic: expected KELN, got {:?}", std::str::from_utf8(magic).unwrap_or("?")
        )));
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != FORMAT_VERSION {
        return Err(CodecError::new(format!(
            "unsupported version: {} (expected {})", version, FORMAT_VERSION
        )));
    }
    let flags = u16::from_le_bytes([data[6], data[7]]);
    let mut pos = 8usize;

    // ---- const_table ----
    let entries: Vec<Constant> = read_section(data, &mut pos)?;

    // ---- tag_table ----
    let tag_names: Vec<String> = read_section(data, &mut pos)?;

    // ---- layout_table ----
    let (layouts, by_idx): (HashMap<String, Vec<String>>, Vec<String>) =
        read_section(data, &mut pos)?;

    // ---- fn_table ----
    let (fns, fn_index): (Vec<KelnFn>, HashMap<String, usize>) =
        read_section(data, &mut pos)?;

    // ---- entry_point ----
    if pos + 4 > data.len() {
        return Err(CodecError::new("truncated: missing entry_point"));
    }
    let ep = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]);
    let entry = if ep == NO_ENTRY { None } else { Some(ep as usize) };

    // ---- reconstruct KelnModule ----
    let mut module = KelnModule::new();

    // Rebuild ConstantTable from entries
    module.constants = {
        let mut ct = ConstantTable::new();
        for c in entries {
            ct.entries.push(c);
        }
        // Rebuild intern indices from the raw entries vec
        ct.rebuild_indices();
        ct
    };

    // Rebuild TagTable from names
    module.tags = {
        let mut tt = TagTable::new();
        for name in tag_names {
            tt.intern_raw(name);
        }
        tt
    };

    // Rebuild RecordLayoutTable
    module.layouts = RecordLayoutTable { layouts, by_idx };

    module.fns     = fns;
    module.fn_index = fn_index;

    Ok((module, flags, entry))
}

fn read_section<T: serde::de::DeserializeOwned>(
    data: &[u8],
    pos: &mut usize,
) -> Result<T, CodecError> {
    if *pos + 4 > data.len() {
        return Err(CodecError::new("truncated: missing section length"));
    }
    let len = u32::from_le_bytes([data[*pos], data[*pos+1], data[*pos+2], data[*pos+3]]) as usize;
    *pos += 4;
    if *pos + len > data.len() {
        return Err(CodecError::new(format!(
            "truncated: section claims {} bytes but only {} remain", len, data.len() - *pos
        )));
    }
    let value = bincode::deserialize(&data[*pos..*pos+len])
        .map_err(|e| CodecError::new(format!("deserialize error: {}", e)))?;
    *pos += len;
    Ok(value)
}
