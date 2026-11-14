//! Imports assets from the player's own copies of Vice City for consoles (Xbox, PS2) into a local
//! cache, so optional features can use them (goal "Asset import from original discs"): the Xbox
//! controls screen art and fonts, the PS2 controller-setup sprites, later car textures and more.
//! Nothing from the discs is shipped or committed; the game asks for the disc image (an .iso
//! file) when a feature needs it, and this crate copies the listed files out of it.
//!
//! Formats: Xbox discs use XDVDFS (`xdvdfs`), PS2 discs ISO 9660 (`iso9660`); see
//! Claude outputs/xbox-research/XBOX_RESEARCH.md for what the discs hold.

pub mod iso9660;
pub mod ps2geom;
pub mod ps2tex;
pub mod xboxgeom;
pub mod xboxlm;
pub mod xdvdfs;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// A file on a disc: its path (as on the disc, '/'-separated) and where its bytes are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub offset: u64,
    pub size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscKind {
    Xbox,
    Ps2,
}

impl DiscKind {
    /// Folder of this disc's files in the import cache.
    pub fn dir(self) -> &'static str {
        match self {
            DiscKind::Xbox => "xbox",
            DiscKind::Ps2 => "ps2",
        }
    }
}

/// An opened disc image of GTA Vice City.
pub struct Disc {
    pub kind: DiscKind,
    pub entries: Vec<Entry>,
    file: File,
}

fn has(entries: &[Entry], path: &str) -> bool {
    entries.iter().any(|e| e.path.eq_ignore_ascii_case(path))
}

impl Disc {
    /// Opens an image and checks it is Vice City for Xbox or PS2.
    pub fn open(path: &Path) -> Result<Disc, String> {
        let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let io = |e: std::io::Error| format!("{}: {e}", path.display());
        if let Some(entries) = xdvdfs::read_tree(&mut file).map_err(io)? {
            if has(&entries, "default.xbe") && has(&entries, "neo/ControlMapping1.txt") {
                return Ok(Disc { kind: DiscKind::Xbox, entries, file });
            }
            return Err(format!("{}: an Xbox disc, but not GTA Vice City", path.display()));
        }
        if let Some(entries) = iso9660::read_tree(&mut file).map_err(io)? {
            if has(&entries, "SYSTEM.CNF") && has(&entries, "MODELS/FRONTEN2.TXD") {
                return Ok(Disc { kind: DiscKind::Ps2, entries, file });
            }
            return Err(format!("{}: a disc image, but not GTA Vice City for PS2", path.display()));
        }
        Err(format!("{}: not an Xbox or PS2 disc image", path.display()))
    }

    pub fn find(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path.eq_ignore_ascii_case(path))
    }

    pub fn read(&mut self, e: &Entry) -> std::io::Result<Vec<u8>> {
        let mut data = vec![0u8; e.size as usize];
        self.file.seek(SeekFrom::Start(e.offset))?;
        self.file.read_exact(&mut data)?;
        Ok(data)
    }

    /// `size` bytes at `offset` inside the file `e` (clamped to its end).
    pub fn read_range(&mut self, e: &Entry, offset: u64, size: usize) -> std::io::Result<Vec<u8>> {
        let size = size.min(e.size.saturating_sub(offset) as usize);
        let mut data = vec![0u8; size];
        self.file.seek(SeekFrom::Start(e.offset + offset))?;
        self.file.read_exact(&mut data)?;
        Ok(data)
    }
}

/// What to lift from each disc: files whose path starts with `dir` (case-insensitive) and ends
/// with one of `exts` (empty = any), copied to `<cache>/<kind>/<dest>/<file name in lower case>`.
pub struct Lift {
    pub kind: DiscKind,
    pub dir: &'static str,
    pub exts: &'static [&'static str],
    pub dest: &'static str,
}

pub const MANIFEST: &[Lift] = &[
    // Xbox front end: menu sprites (pad outline, A/B/X/Y, D-pad, icons, radio logos), the
    // 1536x1536 pause-menu map (UserInterface.txd) and its two bitmap fonts.
    Lift { kind: DiscKind::Xbox, dir: "neo/UserInterface/models/", exts: &[".dds"], dest: "ui" },
    Lift { kind: DiscKind::Xbox, dir: "neo/UserInterface/", exts: &[".dds", ".txd"], dest: "ui" },
    Lift { kind: DiscKind::Xbox, dir: "neo/UserInterface/fonts/", exts: &[], dest: "ui/fonts" },
    // The pause menu's "tutorial briefings" and "unlocked items" lists: which text keys each
    // collects (core/BriefLists.cpp).
    Lift { kind: DiscKind::Xbox, dir: "neo/Hints.txt", exts: &[], dest: "neo" },
    Lift { kind: DiscKind::Xbox, dir: "neo/UnlockedItems.txt", exts: &[], dest: "neo" },
    // The pause menu's Maestro animation (R3_Menu.anm): the selection bar's five outline shapes.
    Lift { kind: DiscKind::Xbox, dir: "neo/UserInterface/models/R3_Menu.anm", exts: &[], dest: "ui" },
    // HUD and fonts (DXT5: smoother alpha than the PC's DXT3).
    Lift { kind: DiscKind::Xbox, dir: "models/hud.txd", exts: &[], dest: "models" },
    Lift { kind: DiscKind::Xbox, dir: "models/fonts.txd", exts: &[], dest: "models" },
    // Wheels and the loading, splash and intro screens (with the HUD and fonts, the Xbox style's
    // texture pack: revc/src/ps2pack.rs).
    Lift { kind: DiscKind::Xbox, dir: "models/generic/wheels.txd", exts: &[], dest: "models" },
    Lift { kind: DiscKind::Xbox, dir: "txd/", exts: &[".txd"], dest: "txd" },
    // PS2 front end: the controller-setup sprites (DualShock picture, callouts) re3's PS2 menu uses,
    // the rest of the menu art, the HUD, and the loading, splash and intro screens. They are PS2
    // native (GS swizzled); the game reads them through a texture pack built from them
    // (revc/src/ps2pack.rs, ps2tex.rs).
    Lift { kind: DiscKind::Ps2, dir: "MODELS/FRONTEN2.TXD", exts: &[], dest: "models" },
    Lift { kind: DiscKind::Ps2, dir: "MODELS/FRONTEN1.TXD", exts: &[], dest: "models" },
    Lift { kind: DiscKind::Ps2, dir: "MODELS/HUD.TXD", exts: &[], dest: "models" },
    Lift { kind: DiscKind::Ps2, dir: "TXD/", exts: &[".txd"], dest: "txd" },
    // PS2 time cycle: 5 rows differ from the PC's (the PS2 visual style uses it, plan task 13).
    Lift { kind: DiscKind::Ps2, dir: "DATA/TIMECYC.DAT", exts: &[], dest: "data" },
    // PS2 ped animations: 13 differ from the PC's (walk_start, idle_armed, the woman's walks, ...);
    // the PS2 UI style plays them (animation/AnimManager.cpp ReplacePedAnimsFromPS2).
    Lift { kind: DiscKind::Ps2, dir: "ANIM/PED.IFP", exts: &[], dest: "anim" },
    // The consoles' English text (the console styles' menus and credits: revc/src/text.rs). The
    // Xbox's is one flat TKEY/TDAT table.
    Lift { kind: DiscKind::Xbox, dir: "TEXT/american.gxt", exts: &[], dest: "text" },
    // The Xbox's fire: its particle values (merged by name in the Xbox style) and flame frames.
    Lift { kind: DiscKind::Xbox, dir: "data/particle.cfg", exts: &[], dest: "data" },
    Lift { kind: DiscKind::Xbox, dir: "models/particle.txd", exts: &[], dest: "models" },
    Lift { kind: DiscKind::Ps2, dir: "TEXT/AMERICAN.GXT", exts: &[], dest: "text" },
    // The Xbox's cutscene hands (roadmap R4): which hands each cutscene character gets, and the
    // finger poses (RenderWare hierarchy animations); the model is in models-world, its
    // dictionary comes out of gta3.img below.
    Lift { kind: DiscKind::Xbox, dir: "data/cutscenehands.xml", exts: &[], dest: "data" },
    Lift { kind: DiscKind::Xbox, dir: "anim/", exts: &[".anm"], dest: "anim" },
];

/// Where a PS2 radio or cutscene stream goes under the import's folder, or None:
/// `AUDIO/MUSIC/FLASH.VB` -> `audio/music/flash.vb`, `AUDIO/CUTSCENE/COK/COK_4A.VB` -> `audio/cutscene/cok/cok_4a.vb`.
fn ps2_audio_dest(path: &str) -> Option<PathBuf> {
    let p = path.to_ascii_lowercase();
    let rest = p.strip_prefix("audio/")?;
    ((rest.starts_with("music/") || rest.starts_with("cutscene/")) && rest.ends_with(".vb")).then(|| Path::new("audio").join(rest))
}

/// Where a map file of the disc goes under the import's folder, or None: the map definitions and
/// placements (IDE, IPL, ZON) and the area collision files below `data/` keep their folders under
/// `maps/` (lower case; `DATA/MAPS/BANK/BANK.IPL` -> `maps/maps/bank/bank.ipl`), with the level file
/// and `default.ide`; the loose collision files of `models/coll/` go to `coll/`.
fn map_file_dest(path: &str) -> Option<PathBuf> {
    let p = path.to_ascii_lowercase();
    let ext = |xs: &[&str]| xs.iter().any(|x| p.ends_with(x));
    if let Some(rest) = p.strip_prefix("data/") {
        let wanted = (rest.starts_with("maps/") && ext(&[".ide", ".ipl", ".zon", ".col"]))
            || rest == "default.ide"
            || rest == "gta_vc.dat"
            || (!rest.contains('/') && ext(&[".zon"]));
        return wanted.then(|| Path::new("maps").join(rest));
    }
    let name = p.strip_prefix("models/coll/")?;
    (!name.contains('/') && name.ends_with(".col")).then(|| Path::new("coll").join(name))
}

/// Copies the disc's map files (map_file_dest) into the import; the number written.
fn lift_map_files(disc: &mut Disc, root: &Path, report: &mut Report) -> Result<usize, String> {
    let wanted: Vec<(Entry, PathBuf)> = disc.entries.iter().filter_map(|e| Some((e.clone(), root.join(map_file_dest(&e.path)?)))).collect();
    for (e, dest) in &wanted {
        let data = disc.read(e).map_err(|err| format!("{}: {err}", e.path))?;
        space::write_file(&dest, &data)?;
        report.written.push(dest.clone());
    }
    Ok(wanted.len())
}

/// The PS2 cutscene entries the PS2 style plays instead of the PC's: three animations (a deal
/// re-keyed, a door and a dancer's fingers) and four camera paths.
pub const PS2_CUTS: &[&str] = &["int_d.ifp", "law_4.ifp", "stripa.ifp", "cok_4a.dat", "fin.dat", "rok_2.dat", "tex_3.dat"];

fn matches(lift: &Lift, path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    let dir = lift.dir.to_ascii_lowercase();
    if !p.starts_with(&dir) {
        return false;
    }
    // A directory lift takes only its own files, not subfolders.
    let rest = &p[dir.len()..];
    if dir.ends_with('/') && rest.contains('/') {
        return false;
    }
    lift.exts.is_empty() || lift.exts.iter().any(|x| p.ends_with(x))
}

/// The result of an import: files written, and where.
#[derive(Debug, Default)]
pub struct Report {
    pub written: Vec<PathBuf>,
}

/// Copies the manifest's files from `disc` into `cache/<kind>/...` and writes
/// `cache/<kind>/IMPORTED.txt` (which image, how many files) last, as the marker that the
/// import is complete.
pub fn import(disc: &mut Disc, image: &Path, cache: &Path) -> Result<Report, String> {
    let root = cache.join(disc.kind.dir());
    let wanted: Vec<(Entry, &Lift)> = disc
        .entries
        .iter()
        .filter_map(|e| MANIFEST.iter().find(|l| l.kind == disc.kind && matches(l, &e.path)).map(|l| (e.clone(), l)))
        .collect();
    if wanted.is_empty() {
        return Err("nothing to import from this disc".into());
    }
    // Disk space: the files to write, an upper bound for the converted archive contents, plus a
    // margin (space.rs); refused before anything is written when short
    let ps2_audio: Vec<Entry> = if disc.kind == DiscKind::Ps2 {
        disc.entries.iter().filter(|e| ps2_audio_dest(&e.path).is_some()).cloned().collect()
    } else {
        Vec::new()
    };
    let archives: u64 = disc.entries.iter().filter(|e| {
        let p = e.path.to_ascii_lowercase();
        (p.starts_with("models/gta3") && p.ends_with(".img")) || p == "anim/cuts.img"
    }).map(|e| e.size).sum::<u64>()
        // the Xbox's archive entries are LZO-compressed and are written unpacked (models,
        // world dictionaries): a generous bound for what they unpack to
        * if disc.kind == DiscKind::Xbox { 3 } else { 1 };
    let needed: u64 = wanted.iter().map(|(e, _)| e.size).sum::<u64>()
        + ps2_audio.iter().map(|e| e.size).sum::<u64>()
        + disc.entries.iter().filter(|e| map_file_dest(&e.path).is_some()).map(|e| e.size).sum::<u64>()
        + archives;
    std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    match space::check(&root, needed, space::free_bytes(&root))? {
        Some(free) => eprintln!("revc-import: about {} to write, {} free", space::human(needed), space::human(free)),
        None => eprintln!("revc-import: free disk space unknown here; writing about {} (a full disk stops the import cleanly)", space::human(needed)),
    }
    let mut report = Report::default();
    // The PS2's radio and cutscene streams (roadmap R5: the PS2 style plays them)
    for e in &ps2_audio {
        let dest = root.join(ps2_audio_dest(&e.path).unwrap());
        let data = disc.read(e).map_err(|err| format!("{}: {err}", e.path))?;
        space::write_file(&dest, &data)?;
        report.written.push(dest);
    }
    for (e, lift) in wanted {
        let name = e.path.rsplit('/').next().unwrap_or(&e.path).to_ascii_lowercase();
        let dest = root.join(lift.dest).join(name);
        let data = disc.read(&e).map_err(|err| format!("{}: {err}", e.path))?;
        space::write_file(&dest, &data)?;
        report.written.push(dest);
    }
    // Map data per style (roadmap R4): the definitions, placements and collision, read in the
    // console's style in place of the PC's (matched by model name)
    let maps = lift_map_files(disc, &root, &mut report)?;
    eprintln!("revc-import: {maps} map files");
    if disc.kind == DiscKind::Xbox {
        // World lightmaps (the textures of the Xbox's MatFX dual pass, `<model>@`, and the second
        // coordinates; xboxlm)
        // (skipped if the disc has no gta3.img: not a full game disc)
        let lightmaps = match (disc.find("models/gta3.dir").cloned(), disc.find("models/gta3.img").cloned()) {
            (Some(d), Some(i)) => vec![xbox_lightmaps(disc, &d, &i)?],
            _ => Vec::new(),
        };
        // Vehicle textures (the Xbox's are sharper: 256 x 256 car textures): the TXDs that the
        // disc's default.ide names in its cars section, out of gta3.img, unpacked
        if let (Some(d), Some(i), Some(ide)) = (disc.find("models/gta3.dir").cloned(), disc.find("models/gta3.img").cloned(), disc.find("data/default.ide").cloned()) {
            let ide = disc.read(&ide).map_err(|e| format!("default.ide: {e}"))?;
            let names = vehicle_txds(&String::from_utf8_lossy(&ide));
            let dir = disc.read(&d).map_err(|e| format!("gta3.dir: {e}"))?;
            let (mut models, mut kept_pc) = (0usize, std::collections::BTreeMap::<String, usize>::new());
            for e in dir.chunks_exact(32) {
                let name = String::from_utf8_lossy(e[8..32].split(|&c| c == 0).next().unwrap_or(&[])).to_ascii_lowercase();
                // Models (the "exact Xbox models" option): the Xbox's native geometry converted to
                // ordinary geometry (xboxgeom.rs); the ones it can't convert stay PC models.
                if name.ends_with(".dff") {
                    let off = u32::from_le_bytes(e[0..4].try_into().unwrap()) as u64 * 2048;
                    let size = u32::from_le_bytes(e[4..8].try_into().unwrap()) as usize * 2048;
                    let raw = disc.read_range(&i, off, size).map_err(|err| format!("{name}: {err}"))?;
                    let converted = xboxlm::img_entry(&raw).and_then(|d| xboxgeom::convert_dff(&d));
                    match converted {
                        Ok(data) => {
                            let dest = root.join("models-world").join(&name);
                            space::write_file(&dest, &data)?;
                            report.written.push(dest);
                            models += 1;
                        }
                        Err(why) => *kept_pc.entry(why.split(" (").next().unwrap_or("").to_string()).or_default() += 1,
                    }
                    continue;
                }
                // Area collision files (map data per style)
                if name.ends_with(".col") {
                    let off = u32::from_le_bytes(e[0..4].try_into().unwrap()) as u64 * 2048;
                    let size = u32::from_le_bytes(e[4..8].try_into().unwrap()) as usize * 2048;
                    let raw = disc.read_range(&i, off, size).map_err(|err| format!("{name}: {err}"))?;
                    let data = xboxlm::img_entry(&raw).map_err(|err| format!("{name}: {err}"))?;
                    let dest = root.join("coll").join(&name);
                    space::write_file(&dest, &data)?;
                    report.written.push(dest);
                    continue;
                }
                let Some(stem) = name.strip_suffix(".txd") else { continue };
                // the cutscene hands' textures (with the hands model, models-world/cshands.dff)
                if stem == "cshands" {
                    let off = u32::from_le_bytes(e[0..4].try_into().unwrap()) as u64 * 2048;
                    let size = u32::from_le_bytes(e[4..8].try_into().unwrap()) as usize * 2048;
                    let raw = disc.read_range(&i, off, size).map_err(|err| format!("{name}: {err}"))?;
                    let data = xboxlm::img_entry(&raw).map_err(|err| format!("{name}: {err}"))?;
                    let dest = root.join("models").join(&name);
                    space::write_file(&dest, &data)?;
                    report.written.push(dest);
                    continue;
                }
                let off = u32::from_le_bytes(e[0..4].try_into().unwrap()) as u64 * 2048;
                let size = u32::from_le_bytes(e[4..8].try_into().unwrap()) as usize * 2048;
                let raw = disc.read_range(&i, off, size).map_err(|err| format!("{name}: {err}"))?;
                let data = xboxlm::img_entry(&raw).map_err(|err| format!("{name}: {err}"))?;
                // World dictionaries: the textures the Xbox's models use that a PC dictionary
                // lacks come from these (revc/src/xboxworld.rs); the generic slot is the disc's
                // standalone models/generic.txd (below)
                if !names.contains(stem) {
                    if stem != "generic" {
                        let dest = root.join("world").join(&name);
                        space::write_file(&dest, &data)?;
                        report.written.push(dest);
                    }
                    continue;
                }
                let dest = root.join("vehicles").join(&name);
                space::write_file(&dest, &data)?;
                report.written.push(dest);
            }
            eprintln!("revc-import: {models} Xbox models converted; kept as PC models: {kept_pc:?}");
            if let Some(g) = disc.find("models/generic.txd").cloned() {
                let data = disc.read(&g).map_err(|e| format!("generic.txd: {e}"))?;
                let dest = root.join("world").join("generic.txd");
                space::write_file(&dest, &data)?;
                report.written.push(dest);
            }
        }
        for (bin, txd) in lightmaps {
        for (file, data) in [("lightmaps.bin", bin), ("lightmaps.txd", txd)] {
            let dest = root.join("models").join(file);
            space::write_file(&dest, &data)?;
            report.written.push(dest);
        }
        }
    }
    if disc.kind == DiscKind::Ps2 {
        // World textures (the "exact PS2 textures" option; revc/src/ps2pack.rs): every TXD of the
        // split GTA3.IMG (GTA3.IMG, GTA3_1..6.IMG, each part the first one's size, offsets running
        // on across them), written out as it is
        if let Some(dir_e) = disc.find("MODELS/GTA3.DIR").cloned() {
            let parts: Vec<Entry> = std::iter::once("MODELS/GTA3.IMG".to_string())
                .chain((1..=6).map(|i| format!("MODELS/GTA3_{i}.IMG")))
                .map_while(|p| disc.find(&p).cloned())
                .collect();
            if let Some(first) = parts.first() {
                let part_size = first.size;
                let dir = disc.read(&dir_e).map_err(|e| format!("GTA3.DIR: {e}"))?;
                let (mut models, mut kept_pc) = (0usize, std::collections::BTreeMap::<String, usize>::new());
                for e in dir.chunks_exact(32) {
                    let name = String::from_utf8_lossy(e[8..32].split(|&c| c == 0).next().unwrap_or(&[])).to_ascii_lowercase();
                    let is_dff = name.ends_with(".dff");
                    let is_col = name.ends_with(".col");
                    if !name.ends_with(".txd") && !is_dff && !is_col {
                        continue;
                    }
                    let mut pos = u32::from_le_bytes(e[0..4].try_into().unwrap()) as u64 * 2048;
                    let mut left = u32::from_le_bytes(e[4..8].try_into().unwrap()) as u64 * 2048;
                    let mut data = Vec::with_capacity(left as usize);
                    while left > 0 {
                        let (i, o) = ((pos / part_size) as usize, pos % part_size);
                        let part = parts.get(i).ok_or_else(|| format!("{name}: beyond GTA3_{}.IMG", parts.len() - 1))?;
                        let n = left.min(part_size - o);
                        data.extend(disc.read_range(part, o, n as usize).map_err(|err| format!("{name}: {err}"))?);
                        pos += n;
                        left -= n;
                    }
                    // Models (the "exact PS2 models" option): the PS2's native geometry converted to
                    // ordinary geometry (ps2geom.rs); the ones it can't convert stay PC models.
                    let (dest, data) = if is_dff {
                        match ps2geom::convert_dff(&data) {
                            Ok(d) => {
                                models += 1;
                                (root.join("models-world").join(&name), d)
                            }
                            Err(why) => {
                                *kept_pc.entry(why.split(':').next_back().unwrap_or("").trim().to_string()).or_default() += 1;
                                continue;
                            }
                        }
                    } else if is_col {
                        // area collision files (map data per style)
                        (root.join("coll").join(&name), data)
                    } else {
                        (root.join("world").join(&name), data)
                    };
                    space::write_file(&dest, &data)?;
                    report.written.push(dest);
                }
                eprintln!("revc-import: {models} PS2 models converted; kept as PC models: {kept_pc:?}");
            }
        }
        // Cutscenes whose animation or camera path differ from the PC's in substance (not only in
        // padding): the PS2 style plays these (animation/CutsceneMgr.cpp). The cuts with PS2-only
        // characters are left out: nothing would spawn those characters.
        if let (Some(d), Some(i)) = (disc.find("ANIM/CUTS.DIR").cloned(), disc.find("ANIM/CUTS.IMG").cloned()) {
            let dir = disc.read(&d).map_err(|e| format!("CUTS.DIR: {e}"))?;
            for e in dir.chunks_exact(32) {
                let name = String::from_utf8_lossy(e[8..32].split(|&c| c == 0).next().unwrap_or(&[])).to_ascii_lowercase();
                if !PS2_CUTS.contains(&name.as_str()) {
                    continue;
                }
                let off = u32::from_le_bytes(e[0..4].try_into().unwrap()) as u64 * 2048;
                let size = u32::from_le_bytes(e[4..8].try_into().unwrap()) as usize * 2048;
                let data = disc.read_range(&i, off, size).map_err(|err| format!("{name}: {err}"))?;
                let dest = root.join("cuts").join(&name);
                space::write_file(&dest, &data)?;
                report.written.push(dest);
            }
        }
        // The game fills the "generic" slot from the standalone MODELS/GENERIC.TXD, not from the
        // archive's generic.txd (an older copy: its parking-lane road lacks the yellow centre line)
        if let Some(e) = disc.find("MODELS/GENERIC.TXD").cloned() {
            let data = disc.read(&e).map_err(|err| format!("GENERIC.TXD: {err}"))?;
            let dest = root.join("world").join("generic.txd");
            space::write_file(&dest, &data)?;
            if !report.written.contains(&dest) {
                report.written.push(dest);
            }
        }
    }
    let marker = format!(
        "imported from {}\nfiles {}\n(from the player's own disc; not for redistribution)\n",
        image.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        report.written.len()
    );
    std::fs::write(root.join("IMPORTED.txt"), marker).map_err(|e| e.to_string())?;
    Ok(report)
}

/// The TXD names of the vehicles in an IDE file's `cars` section (third field; lower case).
pub fn vehicle_txds(ide: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let mut in_cars = false;
    for line in ide.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if !in_cars {
            in_cars = line.eq_ignore_ascii_case("cars");
            continue;
        }
        if line.eq_ignore_ascii_case("end") {
            in_cars = false;
            continue;
        }
        if let Some(txd) = line.split(',').nth(2) {
            out.insert(txd.trim().to_ascii_lowercase());
        }
    }
    out
}

/// The Xbox world lightmap sidecar and TXD (xboxlm) from the disc's models/gta3.dir, gta3.img and
/// generic.txd.
fn xbox_lightmaps(disc: &mut Disc, dir_e: &Entry, img_e: &Entry) -> Result<(Vec<u8>, Vec<u8>), String> {
    let dir = disc.read(dir_e).map_err(|e| format!("gta3.dir: {e}"))?;
    let generic = match disc.find("models/generic.txd").cloned() {
        Some(e) => disc.read(&e).map_err(|err| format!("generic.txd: {err}"))?,
        None => Vec::new(),
    };
    let (bin, txd, _) = xboxlm::img_sidecar(&dir, &[&generic], |off, size| disc.read_range(img_e, off, size))?;
    Ok((bin, txd))
}

/// Whether `cache` holds a complete import from this kind of disc.
pub fn imported(cache: &Path, kind: DiscKind) -> bool {
    cache.join(kind.dir()).join("IMPORTED.txt").is_file()
}

#[cfg(test)]
mod tests;
pub mod space;
