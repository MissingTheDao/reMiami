//! Console disc textures as texture packs (roadmap R2, R3): the imported TXDs are decoded once and
//! written as `<import cache>/<ps2|xbox>/packs/<ps2|xbox>-native.zip`, in the texture-pack layout
//! texmods reads (`textures/<txd>/<texture>.png`). The name prefix makes each apply in its UI style
//! only.
//!
//! PS2 (the game can't read PS2 native TXDs; revc_import::ps2tex decodes them): the HUD (`hud`),
//! the menu art (`frontend1`, `frontend2`: the game's names for FRONTEN1/2.TXD) and the loading,
//! splash and intro screens (global entries: they load into a shared slot and their texture names
//! are unique). Not the fonts: the PS2's glyph sheets are laid out differently from the PC's, which
//! the font code's tables expect.
//! Xbox (revc_formats::txd reads Xbox natives): the HUD, the fonts (the PC's layout, DXT5; its [ ]
//! slots hold the Xbox's arrow icons), the wheels, the vehicles (their own TXD names) and the
//! loading, splash and intro screens.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Console {
    Ps2,
    Xbox,
}

impl Console {
    fn dir(self) -> &'static str {
        match self {
            Console::Ps2 => "ps2",
            Console::Xbox => "xbox",
        }
    }
}

/// The pack's path in the import cache.
pub fn pack_path(cache: &Path, console: Console) -> PathBuf {
    let d = console.dir();
    cache.join(d).join("packs").join(format!("{d}-native.zip"))
}

/// (dictionary folder in the pack, TXD file in the import cache); "" = global entries.
fn sources(cache: &Path, console: Console) -> Vec<(String, PathBuf)> {
    let root = cache.join(console.dir());
    let mut out = match console {
        Console::Ps2 => vec![
            ("hud".to_string(), root.join("models/hud.txd")),
            ("frontend1".to_string(), root.join("models/fronten1.txd")),
            ("frontend2".to_string(), root.join("models/fronten2.txd")),
        ],
        Console::Xbox => vec![
            ("hud".to_string(), root.join("models/hud.txd")),
            ("fonts".to_string(), root.join("models/fonts.txd")),
            ("wheels".to_string(), root.join("models/wheels.txd")),
        ],
    };
    // Xbox vehicle textures: each TXD under its own name, as the game looks them up
    if let Ok(rd) = std::fs::read_dir(root.join("vehicles")) {
        let mut cars: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("txd"))).collect();
        cars.sort();
        out.extend(cars.into_iter().map(|p| (p.file_stem().unwrap().to_string_lossy().to_ascii_lowercase(), p)));
    }
    if let Ok(rd) = std::fs::read_dir(root.join("txd")) {
        let mut screens: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
                (n.starts_with("loadsc") || n.starts_with("splash") || n.starts_with("intro")) && n.ends_with(".txd")
            })
            .collect();
        screens.sort();
        out.extend(screens.into_iter().map(|p| (String::new(), p)));
    }
    out.into_iter().filter(|(_, p)| p.is_file()).collect()
}

pub(crate) fn png_bytes(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(rgba).map_err(|e| e.to_string())?;
    wr.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// A zip with stored (uncompressed) entries: the PNGs are compressed already. ZIP64 records only
/// where a value doesn't fit the classic fields (more than 65,535 entries, 4 GB); otherwise the
/// classic layout, byte for byte.
pub(crate) fn write_zip(path: &Path, files: &[(String, Vec<u8>)]) -> std::io::Result<()> {
    let tmp = path.with_extension("zip.part");
    std::fs::write(&tmp, zip_bytes(files))?;
    std::fs::rename(&tmp, path)
}

const FULL32: u64 = 0xFFFF_FFFF;

fn zip_bytes(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut data = Vec::new();
    let mut central = Vec::new();
    for (name, body) in files {
        let crc = crc32fast::hash(body);
        let offset = data.len() as u64;
        let size = body.len() as u64;
        let n = name.len() as u16;
        let big_size = size >= FULL32;
        let big_offset = offset >= FULL32;
        let size32 = if big_size { FULL32 as u32 } else { size as u32 };
        let version: u8 = if big_size || big_offset { 45 } else { 20 };
        // ZIP64 extra fields: the values that don't fit, in the order uncompressed, compressed, offset
        let mut local_extra = Vec::new();
        if big_size {
            local_extra.extend_from_slice(&1u16.to_le_bytes());
            local_extra.extend_from_slice(&16u16.to_le_bytes());
            local_extra.extend_from_slice(&size.to_le_bytes());
            local_extra.extend_from_slice(&size.to_le_bytes());
        }
        let mut cd_extra = Vec::new();
        if big_size || big_offset {
            let mut vals = Vec::new();
            if big_size {
                vals.extend_from_slice(&size.to_le_bytes());
                vals.extend_from_slice(&size.to_le_bytes());
            }
            if big_offset {
                vals.extend_from_slice(&offset.to_le_bytes());
            }
            cd_extra.extend_from_slice(&1u16.to_le_bytes());
            cd_extra.extend_from_slice(&(vals.len() as u16).to_le_bytes());
            cd_extra.extend_from_slice(&vals);
        }
        // local header: signature, version, flags, method 0, time, date, crc, sizes, name length, extra
        data.extend_from_slice(&0x04034b50u32.to_le_bytes());
        data.extend_from_slice(&[version, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        data.extend_from_slice(&crc.to_le_bytes());
        data.extend_from_slice(&size32.to_le_bytes());
        data.extend_from_slice(&size32.to_le_bytes());
        data.extend_from_slice(&n.to_le_bytes());
        data.extend_from_slice(&(local_extra.len() as u16).to_le_bytes());
        data.extend_from_slice(name.as_bytes());
        data.extend_from_slice(&local_extra);
        data.extend_from_slice(body);
        central.extend_from_slice(&0x02014b50u32.to_le_bytes());
        central.extend_from_slice(&[version, 0, version, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size32.to_le_bytes());
        central.extend_from_slice(&size32.to_le_bytes());
        central.extend_from_slice(&n.to_le_bytes());
        central.extend_from_slice(&(cd_extra.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 10]); // comment, disk, internal and external attributes
        central.extend_from_slice(&(if big_offset { FULL32 as u32 } else { offset as u32 }).to_le_bytes());
        central.extend_from_slice(name.as_bytes());
        central.extend_from_slice(&cd_extra);
    }
    let cd_offset = data.len() as u64;
    let cd_size = central.len() as u64;
    let count = files.len() as u64;
    data.extend_from_slice(&central);
    let zip64 = count > 0xFFFF || cd_offset >= FULL32 || cd_size >= FULL32;
    if zip64 {
        let end64 = data.len() as u64;
        data.extend_from_slice(&0x06064b50u32.to_le_bytes());
        data.extend_from_slice(&44u64.to_le_bytes()); // the record's size after this field
        data.extend_from_slice(&[45, 0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // versions, disks
        for v in [count, count, cd_size, cd_offset] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&0x07064b50u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&end64.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
    }
    let count16 = if count > 0xFFFF { 0xFFFF } else { count as u16 };
    let fit = |v: u64| if v >= FULL32 { FULL32 as u32 } else { v as u32 };
    data.extend_from_slice(&0x06054b50u32.to_le_bytes());
    data.extend_from_slice(&[0, 0, 0, 0]);
    data.extend_from_slice(&count16.to_le_bytes());
    data.extend_from_slice(&count16.to_le_bytes());
    data.extend_from_slice(&fit(cd_size).to_le_bytes());
    data.extend_from_slice(&fit(cd_offset).to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data
}

/// Every texture of a TXD as (name, width, height, RGBA) or an error.
fn decode_txd(d: &[u8], console: Console) -> Vec<Result<(String, u32, u32, Vec<u8>), String>> {
    match console {
        Console::Ps2 => revc_import::ps2tex::read_ps2_txd(d).into_iter().map(|t| t.map(|t| (t.name, t.width, t.height, t.rgba))).collect(),
        Console::Xbox => match revc_formats::txd::parse(d) {
            Ok(txd) => txd
                .textures
                .iter()
                .map(|t| t.rgba(0).map(|px| (t.name.clone(), t.width as u32, t.height as u32, px)).map_err(|e| format!("{}: {e}", t.name)))
                .collect(),
            Err(e) => vec![Err(e.to_string())],
        },
    }
}

/// Builds the console's pack from its TXDs in the import cache: Ok(textures written).
pub fn build(cache: &Path, console: Console) -> Result<usize, String> {
    let mut files = Vec::new();
    let mut failed = 0;
    for (folder, txd) in sources(cache, console) {
        let d = std::fs::read(&txd).map_err(|e| format!("{}: {e}", txd.display()))?;
        for t in decode_txd(&d, console) {
            match t {
                Ok((name, w, h, rgba)) => {
                    let name = name.to_ascii_lowercase();
                    let path = if folder.is_empty() { format!("textures/{name}.png") } else { format!("textures/{folder}/{name}.png") };
                    files.push((path, png_bytes(w, h, &rgba)?));
                }
                Err(e) => {
                    failed += 1;
                    eprintln!("ps2pack: {}: {e}", txd.display());
                }
            }
        }
    }
    if files.is_empty() {
        return Err("no textures in the import".into());
    }
    let path = pack_path(cache, console);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    write_zip(&path, &files).map_err(|e| e.to_string())?;
    eprintln!("ps2pack: {} textures in {} ({failed} not decoded)", files.len(), path.display());
    Ok(files.len())
}

/// The PS2 world textures' pack (the "exact PS2 textures" option): its own folder, so texmods can
/// leave it out.
pub fn world_dir(cache: &Path) -> PathBuf {
    cache.join("ps2").join("packs-world")
}

/// Builds `ps2-world.zip` from the world TXDs a PS2 import wrote (`ps2/world/*.txd`), each under
/// its own TXD name: Ok(textures written).
pub fn build_world(cache: &Path) -> Result<usize, String> {
    let rd = std::fs::read_dir(cache.join("ps2").join("world")).map_err(|e| format!("world TXDs: {e}"))?;
    let mut txds: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("txd"))).collect();
    txds.sort();
    let (mut files, mut failed) = (Vec::new(), 0);
    for txd in txds {
        let folder = txd.file_stem().unwrap().to_string_lossy().to_ascii_lowercase();
        let d = std::fs::read(&txd).map_err(|e| format!("{}: {e}", txd.display()))?;
        for t in revc_import::ps2tex::read_ps2_txd(&d) {
            match t {
                Ok(t) => files.push((format!("textures/{folder}/{}.png", t.name.to_ascii_lowercase()), png_bytes(t.width, t.height, &t.rgba)?)),
                Err(_) => failed += 1,
            }
        }
    }
    if files.is_empty() {
        return Err("no PS2 world textures in the import".into());
    }
    let path = world_dir(cache).join("ps2-world.zip");
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    write_zip(&path, &files).map_err(|e| e.to_string())?;
    eprintln!("ps2pack: {} world textures in {} ({failed} not decoded)", files.len(), path.display());
    Ok(files.len())
}

/// Builds a console's pack if its import exists without one (an import made before the packs).
pub fn ensure(cache: &Path) {
    for (console, kind) in [(Console::Ps2, revc_import::DiscKind::Ps2), (Console::Xbox, revc_import::DiscKind::Xbox)] {
        if revc_import::imported(cache, kind) && !pack_path(cache, console).is_file() {
            crate::discimport::note_access(&pack_path(cache, console));
            if let Err(e) = build(cache, console) {
                eprintln!("ps2pack: {e}");
            }
        }
    }
    if revc_import::imported(cache, revc_import::DiscKind::Ps2)
        && cache.join("ps2").join("world").is_dir()
        && !world_dir(cache).join("ps2-world.zip").is_file()
    {
        crate::discimport::note_access(&world_dir(cache).join("ps2-world.zip"));
        if let Err(e) = build_world(cache) {
            eprintln!("ps2pack: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn written_zip_reads_back() {
        let dir = std::env::temp_dir().join(format!("revc-ps2pack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zip = dir.join("t.zip");
        let files = vec![("textures/hud/a.png".to_string(), b"one".to_vec()), ("textures/b.png".to_string(), b"second".to_vec())];
        super::write_zip(&zip, &files).unwrap();
        let entries = crate::ziparchive::list(&zip).unwrap();
        assert_eq!(entries.len(), 2);
        for (e, (name, body)) in entries.iter().zip(&files) {
            assert_eq!(&e.name, name);
            assert_eq!(&e.read().unwrap(), body);
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}


#[cfg(test)]
mod zip_tests {
    use super::*;

    /// The writer as it was before ZIP64 (classic archives must come out the same).
    fn classic(files: &[(String, Vec<u8>)]) -> Vec<u8> {
        let mut data = Vec::new();
        let mut central = Vec::new();
        for (name, body) in files {
            let crc = crc32fast::hash(body);
            let offset = data.len() as u32;
            let (n, size) = (name.len() as u16, body.len() as u32);
            data.extend_from_slice(&0x04034b50u32.to_le_bytes());
            data.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
            data.extend_from_slice(&crc.to_le_bytes());
            data.extend_from_slice(&size.to_le_bytes());
            data.extend_from_slice(&size.to_le_bytes());
            data.extend_from_slice(&n.to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes());
            data.extend_from_slice(name.as_bytes());
            data.extend_from_slice(body);
            central.extend_from_slice(&0x02014b50u32.to_le_bytes());
            central.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&n.to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_offset = data.len() as u32;
        let count = files.len() as u16;
        data.extend_from_slice(&central);
        data.extend_from_slice(&0x06054b50u32.to_le_bytes());
        data.extend_from_slice(&[0, 0, 0, 0]);
        data.extend_from_slice(&count.to_le_bytes());
        data.extend_from_slice(&count.to_le_bytes());
        data.extend_from_slice(&(central.len() as u32).to_le_bytes());
        data.extend_from_slice(&cd_offset.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data
    }

    #[test]
    fn classic_archives_unchanged() {
        let files: Vec<(String, Vec<u8>)> = (0..50).map(|i| (format!("t/{i}.png"), vec![i as u8; i * 7])).collect();
        assert_eq!(zip_bytes(&files), classic(&files));
    }

    #[test]
    fn many_entries_round_trip() {
        let files: Vec<(String, Vec<u8>)> = (0..70_000).map(|i| (format!("{i:05}.png"), vec![(i % 251) as u8; 3])).collect();
        let dir = std::env::temp_dir().join(format!("revc-zip64-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.zip");
        write_zip(&path, &files).unwrap();
        let entries = crate::ziparchive::list(&path).unwrap();
        assert_eq!(entries.len(), 70_000);
        assert_eq!(entries[69_999].name, "69999.png");
        assert_eq!(entries[69_999].read().unwrap(), vec![(69_999 % 251) as u8; 3]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
