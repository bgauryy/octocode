//! Structural inspection of executable/object/archive files via `goblin`.
//!
//! Magic-byte sniffing (`identify_format`) is independent of `goblin` so it can
//! name formats `goblin` does not parse (wasm, gzip, zip, …). For the formats
//! `goblin` *does* parse — ELF / Mach-O / PE / COFF / ar — we additionally pull
//! arch, bits, endianness, symbols, imports, exports, sections and dynamic
//! deps. A parse failure degrades gracefully to the sniffed identity plus a
//! note; it never errors out.

use goblin::Object;

use super::types::BinaryInspectInfo;

/// Per-list display cap. The true totals are reported via the `*_count` fields.
pub const LIST_CAP: usize = 2000;

/// Files larger than this are rejected outright — `goblin` needs the whole
/// buffer mapped and we will not hold an unbounded allocation for it.
pub const MAX_FILE: usize = 512 * 1024 * 1024;

/// Bytes shown in `magic_hex` (matches the old `xxd -p -l 32`).
const MAGIC_BYTES: usize = 32;

struct Lists {
    items: Vec<String>,
    total: u32,
}

fn capped<I: IntoIterator<Item = String>>(iter: I) -> Lists {
    let mut items = Vec::new();
    let mut total = 0u32;
    for s in iter {
        if s.is_empty() {
            continue;
        }
        total += 1;
        if items.len() < LIST_CAP {
            items.push(s);
        }
    }
    Lists { items, total }
}

fn magic_hex(buf: &[u8]) -> String {
    buf.iter()
        .take(MAGIC_BYTES)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Sniff a coarse `(format, description)` from leading magic bytes. Covers more
/// than `goblin` parses so the result is useful for any binary.
fn identify_format(buf: &[u8]) -> (&'static str, String) {
    let m = buf;
    let starts = |sig: &[u8]| m.len() >= sig.len() && &m[..sig.len()] == sig;

    if starts(b"\x7fELF") {
        ("elf", "ELF executable / shared object".to_string())
    } else if starts(&[0xfe, 0xed, 0xfa, 0xce])
        || starts(&[0xfe, 0xed, 0xfa, 0xcf])
        || starts(&[0xce, 0xfa, 0xed, 0xfe])
        || starts(&[0xcf, 0xfa, 0xed, 0xfe])
    {
        ("macho", "Mach-O object".to_string())
    } else if starts(&[0xca, 0xfe, 0xba, 0xbe]) || starts(&[0xbe, 0xba, 0xfe, 0xca]) {
        ("macho-fat", "Mach-O universal (fat) binary".to_string())
    } else if starts(b"MZ") {
        ("pe", "PE / COFF executable (Windows)".to_string())
    } else if starts(b"\x00asm") {
        ("wasm", "WebAssembly module".to_string())
    } else if starts(b"!<arch>\n") {
        ("archive", "ar archive (static library / .deb)".to_string())
    } else if starts(&[0x1f, 0x8b]) {
        ("gzip", "gzip-compressed data".to_string())
    } else if starts(b"PK\x03\x04") || starts(b"PK\x05\x06") {
        ("zip", "ZIP archive".to_string())
    } else if starts(&[0x42, 0x5a, 0x68]) {
        ("bzip2", "bzip2-compressed data".to_string())
    } else if starts(&[0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00]) {
        ("xz", "xz-compressed data".to_string())
    } else if starts(&[0x28, 0xb5, 0x2f, 0xfd]) {
        ("zstd", "zstandard-compressed data".to_string())
    } else {
        ("unknown", "unrecognized or data file".to_string())
    }
}

fn elf_machine(e_machine: u16) -> Option<String> {
    Some(
        match e_machine {
            3 => "x86",
            40 => "arm",
            62 => "x86_64",
            183 => "aarch64",
            243 => "riscv",
            8 => "mips",
            21 => "ppc64",
            _ => return None,
        }
        .to_string(),
    )
}

fn pe_machine(machine: u16) -> Option<String> {
    Some(
        match machine {
            0x014c => "x86",
            0x8664 => "x86_64",
            0xaa64 => "aarch64",
            0x01c0 | 0x01c4 => "arm",
            _ => return None,
        }
        .to_string(),
    )
}

fn mach_cpu(cputype: u32) -> Option<String> {
    Some(
        match cputype {
            7 => "x86",
            0x0100_0007 => "x86_64",
            12 => "arm",
            0x0100_000c => "aarch64",
            _ => return None,
        }
        .to_string(),
    )
}

fn base(format: &str, description: String, buf: &[u8]) -> BinaryInspectInfo {
    BinaryInspectInfo {
        format: format.to_string(),
        description,
        magic_hex: magic_hex(buf),
        arch: None,
        bits: None,
        endianness: None,
        stripped: None,
        entry: None,
        symbols: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        sections: Vec::new(),
        libraries: Vec::new(),
        symbol_count: 0,
        import_count: 0,
        export_count: 0,
        truncated: false,
        notes: Vec::new(),
    }
}

/// Inspect `buf` (already read and size-checked by the caller). `size_truncated`
/// marks that the on-disk file was larger than what was read.
pub fn inspect(buf: &[u8], size_truncated: bool) -> BinaryInspectInfo {
    let (fmt, desc) = identify_format(buf);
    let mut info = base(fmt, desc, buf);
    if size_truncated {
        info.notes.push(
            "file exceeds the inspection size cap; structural data may be incomplete".to_string(),
        );
    }

    match Object::parse(buf) {
        Ok(Object::Elf(elf)) => inspect_elf(&elf, &mut info),
        Ok(Object::PE(pe)) => inspect_pe(&pe, &mut info),
        Ok(Object::Mach(goblin::mach::Mach::Binary(macho))) => inspect_macho(&macho, &mut info),
        Ok(Object::Mach(goblin::mach::Mach::Fat(_))) => {
            info.format = "macho-fat".to_string();
            info.notes
                .push("universal binary — per-architecture detail not expanded".to_string());
        }
        Ok(Object::Archive(ar)) => {
            let members = capped(ar.members().into_iter().map(|m| m.to_string()));
            info.sections = members.items;
            info.notes.push(
                "ar archive — entries listed as sections; use mode=list to extract".to_string(),
            );
        }
        Ok(Object::Unknown(_)) | Err(_) => note_unrecognized(&mut info),
        Ok(_) => {
            // COFF / TE / anything else goblin recognizes but we don't expand.
            info.notes
                .push("recognized object format without expanded detail".to_string());
        }
    }

    info.truncated = info.symbol_count as usize > info.symbols.len()
        || info.import_count as usize > info.imports.len()
        || info.export_count as usize > info.exports.len();
    info
}

fn note_unrecognized(info: &mut BinaryInspectInfo) {
    let note = match info.format.as_str() {
        "zip" | "gzip" | "bzip2" | "xz" | "zstd" | "archive" => {
            "not a recognized executable — for archives/compressed data use mode=list or mode=decompress"
        }
        "unknown" => "not a recognized executable or archive — likely data or text",
        // Magic said it is an executable, but the structural parse failed
        // (truncated / corrupt). Identity from magic bytes still stands.
        _ => "structural parse failed; identity from magic bytes only",
    };
    info.notes.push(note.to_string());
}

fn inspect_elf(elf: &goblin::elf::Elf, info: &mut BinaryInspectInfo) {
    info.bits = Some(if elf.is_64 { 64 } else { 32 });
    info.endianness = Some(if elf.little_endian { "little" } else { "big" }.to_string());
    info.arch = elf_machine(elf.header.e_machine);
    info.entry = Some(format!("{:#x}", elf.entry));
    info.stripped = Some(elf.syms.is_empty());

    info.libraries = elf.libraries.iter().map(|l| l.to_string()).collect();

    // Section names.
    let sections = capped(
        elf.section_headers
            .iter()
            .map(|sh| elf.shdr_strtab.get_at(sh.sh_name).unwrap_or("").to_string()),
    );
    info.sections = sections.items;

    // Symbols (full symtab).
    let syms = capped(
        elf.syms
            .iter()
            .map(|s| elf.strtab.get_at(s.st_name).unwrap_or("").to_string()),
    );
    info.symbols = syms.items;
    info.symbol_count = syms.total;

    // Dynamic symbols split into imports (undefined) and exports (defined global/weak).
    let mut imports: Vec<String> = Vec::with_capacity(64);
    let mut exports: Vec<String> = Vec::with_capacity(64);
    let mut import_total = 0u32;
    let mut export_total = 0u32;
    for sym in elf.dynsyms.iter() {
        let name = elf.dynstrtab.get_at(sym.st_name).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let global = matches!(sym.st_bind(), 1 | 2); // STB_GLOBAL | STB_WEAK
        if !global {
            continue;
        }
        if sym.st_shndx == 0 {
            // SHN_UNDEF
            import_total += 1;
            if imports.len() < LIST_CAP {
                imports.push(name.to_string());
            }
        } else {
            export_total += 1;
            if exports.len() < LIST_CAP {
                exports.push(name.to_string());
            }
        }
    }
    info.imports = imports;
    info.exports = exports;
    info.import_count = import_total;
    info.export_count = export_total;
}

fn inspect_pe(pe: &goblin::pe::PE, info: &mut BinaryInspectInfo) {
    info.bits = Some(if pe.is_64 { 64 } else { 32 });
    info.endianness = Some("little".to_string());
    info.arch = pe_machine(pe.header.coff_header.machine);
    info.entry = Some(format!("{:#x}", pe.entry));

    info.libraries = pe.libraries.iter().map(|l| l.to_string()).collect();

    let sections = capped(
        pe.sections
            .iter()
            .map(|s| s.name().unwrap_or("").to_string()),
    );
    info.sections = sections.items;

    let imports = capped(pe.imports.iter().map(|i| i.name.to_string()));
    info.imports = imports.items;
    info.import_count = imports.total;

    let exports = capped(pe.exports.iter().map(|e| e.name.unwrap_or("").to_string()));
    info.exports = exports.items;
    info.export_count = exports.total;

    info.stripped = Some(info.symbols.is_empty());
}

fn inspect_macho(macho: &goblin::mach::MachO, info: &mut BinaryInspectInfo) {
    info.bits = Some(if macho.is_64 { 64 } else { 32 });
    info.endianness = Some(if macho.little_endian { "little" } else { "big" }.to_string());
    info.arch = mach_cpu(macho.header.cputype);
    info.entry = Some(format!("{:#x}", macho.entry));

    info.libraries = macho.libs.iter().map(|l| l.to_string()).collect();

    // Section names live under each load segment (`SEGMENT,section`).
    let mut sections: Vec<String> = Vec::with_capacity(32);
    for segment in &macho.segments {
        if let Ok(secs) = segment.sections() {
            for (sec, _data) in secs {
                if let Ok(name) = sec.name() {
                    if !name.is_empty() && sections.len() < LIST_CAP {
                        let seg = sec.segname().unwrap_or("");
                        sections.push(if seg.is_empty() {
                            name.to_string()
                        } else {
                            format!("{seg},{name}")
                        });
                    }
                }
            }
        }
    }
    info.sections = sections;

    // Symbols.
    if let Some(symbols) = macho.symbols.as_ref() {
        let mut names = Vec::new();
        let mut total = 0u32;
        for (name, _nlist) in symbols.iter().flatten() {
            if name.is_empty() {
                continue;
            }
            total += 1;
            if names.len() < LIST_CAP {
                names.push(name.to_string());
            }
        }
        info.symbol_count = total;
        info.symbols = names;
    }
    info.stripped = Some(info.symbols.is_empty());

    if let Ok(imports) = macho.imports() {
        let c = capped(imports.iter().map(|i| i.name.to_string()));
        info.imports = c.items;
        info.import_count = c.total;
    }
    if let Ok(exports) = macho.exports() {
        let c = capped(exports.iter().map(|e| e.name.to_string()));
        info.exports = c.items;
        info.export_count = c.total;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_elf_magic() {
        let (fmt, _) = identify_format(b"\x7fELF\x02\x01\x01");
        assert_eq!(fmt, "elf");
    }

    #[test]
    fn identifies_wasm_and_gzip_and_zip() {
        assert_eq!(identify_format(b"\x00asm\x01\x00\x00\x00").0, "wasm");
        assert_eq!(identify_format(&[0x1f, 0x8b, 0x08]).0, "gzip");
        assert_eq!(identify_format(b"PK\x03\x04").0, "zip");
    }

    #[test]
    fn unknown_data_does_not_panic_and_is_marked() {
        let info = inspect(b"not a binary at all, just text", false);
        assert_eq!(info.format, "unknown");
        assert!(!info.notes.is_empty());
    }

    #[test]
    fn truncated_elf_header_degrades_with_note() {
        // Valid magic, then garbage — goblin parse should fail cleanly.
        let mut buf = vec![0x7f, b'E', b'L', b'F'];
        buf.extend_from_slice(&[0u8; 8]);
        let info = inspect(&buf, false);
        assert_eq!(info.format, "elf");
        assert!(info.notes.iter().any(|n| n.contains("parse failed")));
    }
}
