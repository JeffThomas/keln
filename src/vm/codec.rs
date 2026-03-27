use std::collections::HashMap;
use sha2::{Sha256, Digest};
use super::ir::{Constant, KelnFn, KelnModule, TagTable, RecordLayoutTable, ConstantTable};

// =============================================================================
// .kbc binary format constants
// =============================================================================

pub const MAGIC: [u8; 4] = [0x4B, 0x45, 0x4C, 0x4E]; // "KELN"
pub const FORMAT_VERSION: u16 = 10;

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
// Load error (includes schema mismatch)
// =============================================================================

#[derive(Debug)]
pub enum LoadError {
    Codec(CodecError),
    SchemaMismatch {
        type_name: String,
        compiled_fingerprint: [u8; 32],
        current_fingerprint: [u8; 32],
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Codec(e) => write!(f, "{}", e),
            LoadError::SchemaMismatch { type_name, compiled_fingerprint, current_fingerprint } => {
                write!(
                    f,
                    "schema mismatch for type '{}': compiled fingerprint {:?} != current fingerprint {:?}",
                    type_name, compiled_fingerprint, current_fingerprint
                )
            }
        }
    }
}

impl From<CodecError> for LoadError {
    fn from(e: CodecError) -> Self {
        LoadError::Codec(e)
    }
}

// =============================================================================
// Schema fingerprinting
// =============================================================================

/// One entry in the schema table: type name + SHA-256 of (type_name + sorted field names).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SchemaEntry {
    pub type_name: String,
    pub fingerprint: [u8; 32],
}

/// Compute a schema fingerprint for a record layout: SHA-256 of type_name + sorted field names.
fn compute_layout_fingerprint(type_name: &str, fields: &[String]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(type_name.as_bytes());
    let mut sorted_fields = fields.to_vec();
    sorted_fields.sort();
    for field in &sorted_fields {
        hasher.update(b"|");
        hasher.update(field.as_bytes());
    }
    hasher.finalize().into()
}

/// Build a schema table from a RecordLayoutTable.
fn build_schema_table(layouts: &HashMap<String, Vec<String>>) -> Vec<SchemaEntry> {
    let mut entries: Vec<SchemaEntry> = layouts
        .iter()
        .map(|(type_name, fields)| SchemaEntry {
            type_name: type_name.clone(),
            fingerprint: compute_layout_fingerprint(type_name, fields),
        })
        .collect();
    // Sort for deterministic output
    entries.sort_by(|a, b| a.type_name.cmp(&b.type_name));
    entries
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

    // ---- schema_table: u32 byte-length + bincode(Vec<SchemaEntry>) ----
    let schema_table = build_schema_table(&module.layouts.layouts);
    write_section(&mut buf, &schema_table)?;

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
/// Returns `LoadError::SchemaMismatch` if any type's field layout has changed.
pub fn decode(data: &[u8]) -> Result<(KelnModule, u16, Option<usize>), LoadError> {
    // ---- header ----
    if data.len() < 8 {
        return Err(CodecError::new("file too short for header").into());
    }
    let magic = &data[0..4];
    if magic != MAGIC {
        return Err(CodecError::new(format!(
            "bad magic: expected KELN, got {:?}", std::str::from_utf8(magic).unwrap_or("?")
        )).into());
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != FORMAT_VERSION {
        return Err(CodecError::new(format!(
            "unsupported version: {} (expected {})", version, FORMAT_VERSION
        )).into());
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

    // ---- schema_table: compare compiled fingerprints against current layouts ----
    let schema_table: Vec<SchemaEntry> = read_section(data, &mut pos)?;
    for entry in &schema_table {
        let current_fields = layouts.get(&entry.type_name).map(|v| v.as_slice()).unwrap_or(&[]);
        let current_fingerprint = compute_layout_fingerprint(&entry.type_name, current_fields);
        if current_fingerprint != entry.fingerprint {
            return Err(LoadError::SchemaMismatch {
                type_name: entry.type_name.clone(),
                compiled_fingerprint: entry.fingerprint,
                current_fingerprint,
            });
        }
    }

    // ---- fn_table ----
    let (fns, fn_index): (Vec<KelnFn>, HashMap<String, usize>) =
        read_section(data, &mut pos)?;

    // ---- entry_point ----
    if pos + 4 > data.len() {
        return Err(CodecError::new("truncated: missing entry_point").into());
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
