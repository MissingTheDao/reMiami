//! The Xbox style's texture fill-in. In the Xbox style the Xbox's converted world models
//! (`<cache>/xbox/models-world`, ps2models.rs) load instead of the PC's, each still with the PC's
//! texture dictionary from the PC map definitions; some use texture names that dictionary lacks.
//! `xbox-world.zip` holds exactly those textures, decoded from the Xbox's own dictionaries (the
//! same-name one, else its shared generic.txd), as `textures/<txd>/<name>.png`; texmods adds them
//! to the PC dictionary when it loads (texmods.cpp TexModsApply), in the Xbox style only. It never
//! holds a texture the PC dictionary has, so nothing of the PC's is replaced. A name the Xbox
//! doesn't have either is reported and stays missing (the cutscene test fails on it).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub models: usize,
    pub dictionaries: usize,
    pub filled: usize,
    /// "txd/texture"
    pub not_found: Vec<String>,
    pub bad_models: usize,
}

/// Which Xbox dictionary a texture comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    SameName,
    Generic,
}

pub fn world_dir(cache: &Path) -> PathBuf {
    cache.join("xbox").join("packs-world")
}

/// `rel` under `root`, each `/` component matched case-insensitively (an exact-case entry first).
fn resolve_ci(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut cur = root.to_path_buf();
    for part in rel.split(['/', '\\']).filter(|p| !p.is_empty()) {
        let exact = cur.join(part);
        if exact.exists() {
            cur = exact;
            continue;
        }
        let entry = std::fs::read_dir(&cur).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
        cur = entry.path();
    }
    Some(cur)
}

fn read_ci_bytes(root: &Path, rel: &str) -> Option<Vec<u8>> {
    std::fs::read(resolve_ci(root, rel)?).ok()
}

/// model -> txd (both lower-case) from an IDE's objs/tobj/anim/hier sections.
fn parse_ide(text: &str, map: &mut HashMap<String, String>) {
    let mut section = false;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if !line.contains(',') {
            section = matches!(lower.as_str(), "objs" | "tobj" | "anim" | "hier");
            continue;
        }
        if section {
            let f: Vec<&str> = line.split(',').map(str::trim).collect();
            if f.len() >= 3 {
                map.insert(f[1].to_ascii_lowercase(), f[2].to_ascii_lowercase());
            }
        }
    }
}

fn pc_model_txds(game: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut ides = vec!["data/default.ide".to_string()];
    if let Some(dat) = read_ci_bytes(game, "data/gta_vc.dat") {
        for line in String::from_utf8_lossy(&dat).lines() {
            let t = line.trim();
            if let Some(p) = t.strip_prefix("IDE ").or_else(|| t.strip_prefix("ide ")) {
                ides.push(p.trim().replace('\\', "/"));
            }
        }
    }
    for ide in ides {
        if let Some(b) = read_ci_bytes(game, &ide) {
            parse_ide(&String::from_utf8_lossy(&b), &mut map);
        }
    }
    map
}

fn texture_names(t: &revc_formats::txd::Txd) -> HashSet<String> {
    t.textures.iter().map(|x| x.name.to_ascii_lowercase()).collect()
}

/// What the zip must hold: (txd, texture, source) per missing name, sorted, and the counts.
pub fn plan(game: &Path, cache: &Path) -> Result<(Vec<(String, String, Source)>, Report), String> {
    let model_txd = pc_model_txds(game);
    let img = resolve_ci(game, "models/gta3.img").ok_or("no models/gta3.img")?;
    let archive = revc_formats::img::Archive::open(&img).map_err(|e| e.to_string())?;
    let world = cache.join("xbox").join("world");
    let mut report = Report::default();
    // (txd -> texture names the Xbox models use)
    let mut used: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let dir = cache.join("xbox").join("models-world");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten().map(|e| e.path()).collect();
    files.sort();
    for p in files {
        if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dff")) {
            continue;
        }
        let stem = p.file_stem().unwrap().to_string_lossy().to_ascii_lowercase();
        let Some(txd) = model_txd.get(&stem) else { continue };
        report.models += 1;
        let Ok(clump) = std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| revc_formats::dff::parse(&b).map_err(|e| e.to_string())) else {
            report.bad_models += 1;
            continue;
        };
        let set = used.entry(txd.clone()).or_default();
        for g in &clump.geometries {
            for m in &g.materials {
                if let Some(t) = &m.texture {
                    if !t.name.is_empty() {
                        set.insert(t.name.to_ascii_lowercase());
                    }
                }
            }
        }
    }
    let generic = std::fs::read(world.join("generic.txd")).ok().and_then(|b| revc_formats::txd::parse(&b).ok()).map(|t| texture_names(&t)).unwrap_or_default();
    let mut out = Vec::new();
    for (txd, names) in used {
        let Some(entry) = archive.find(&format!("{txd}.txd")) else { continue };
        let Ok(pc) = archive.read(entry).map_err(|e| e.to_string()).and_then(|b| revc_formats::txd::parse(&b).map_err(|e| e.to_string())) else { continue };
        let pc = texture_names(&pc);
        let missing: Vec<&String> = names.iter().filter(|n| !pc.contains(*n)).collect();
        if missing.is_empty() {
            continue;
        }
        report.dictionaries += 1;
        let same = std::fs::read(world.join(format!("{txd}.txd"))).ok().and_then(|b| revc_formats::txd::parse(&b).ok()).map(|t| texture_names(&t)).unwrap_or_default();
        for n in missing {
            if same.contains(n) {
                out.push((txd.clone(), n.clone(), Source::SameName));
            } else if generic.contains(n) {
                out.push((txd.clone(), n.clone(), Source::Generic));
            } else {
                report.not_found.push(format!("{txd}/{n}"));
            }
        }
    }
    report.filled = out.len();
    Ok((out, report))
}

/// Builds `xbox-world.zip` (see the module notes).
pub fn build(game: &Path, cache: &Path) -> Result<Report, String> {
    let (plan, report) = plan(game, cache)?;
    let world = cache.join("xbox").join("world");
    let mut loaded: HashMap<String, Option<revc_formats::txd::Txd>> = HashMap::new();
    let mut files = Vec::new();
    for (txd, name, src) in &plan {
        let file = if *src == Source::SameName { txd.as_str() } else { "generic" };
        let dict = loaded.entry(file.to_string()).or_insert_with(|| std::fs::read(world.join(format!("{file}.txd"))).ok().and_then(|b| revc_formats::txd::parse(&b).ok()));
        let Some(tex) = dict.as_ref().and_then(|d| d.find(name)) else { continue };
        let rgba = tex.rgba(0).map_err(|e| format!("{file}/{name}: {e}"))?;
        files.push((format!("textures/{txd}/{name}.png"), crate::ps2pack::png_bytes(tex.width as u32, tex.height as u32, &rgba)?));
    }
    if files.is_empty() {
        return Err("no Xbox textures to fill in".into());
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let path = world_dir(cache).join("xbox-world.zip");
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    crate::ps2pack::write_zip(&path, &files).map_err(|e| e.to_string())?;
    eprintln!(
        "xboxworld: {} textures filled in for {} dictionaries ({} Xbox models scanned), {} not found on the Xbox either",
        files.len(),
        report.dictionaries,
        report.models,
        report.not_found.len()
    );
    for n in &report.not_found {
        eprintln!("xboxworld: {n}: missing on the PC and the Xbox");
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_case_first() {
        let d = std::env::temp_dir().join(format!("revc-xboxworld-{}", std::process::id()));
        std::fs::create_dir_all(d.join("Data")).unwrap();
        std::fs::create_dir_all(d.join("data")).unwrap();
        std::fs::write(d.join("data").join("a.txt"), b"lower").unwrap();
        std::fs::write(d.join("Data").join("a.txt"), b"upper").unwrap();
        assert_eq!(read_ci_bytes(&d, "data/a.txt").unwrap(), b"lower");
        assert_eq!(read_ci_bytes(&d, "Data/A.TXT").unwrap(), b"upper");
        assert_eq!(read_ci_bytes(&d, "DATA\\a.txt").map(|v| v.len()), Some(5));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn ide_sections() {
        let t = "# comment\nobjs\n100, ModelA, TxdA, 1, 100, 0 # tail\nend\ncars\n200, car, cartxd, x\nend\ntobj\n 101 , m_b , t_b , 1, 50, 0, 6, 20\nend\nhier\n102, cut, cuttxd\nend\n";
        let mut m = HashMap::new();
        parse_ide(t, &mut m);
        assert_eq!(m.get("modela").map(String::as_str), Some("txda"));
        assert_eq!(m.get("m_b").map(String::as_str), Some("t_b"));
        assert_eq!(m.get("cut").map(String::as_str), Some("cuttxd"));
        assert!(!m.contains_key("car"));
    }

    /// The real data (local game and Xbox import, skipped without them): the count measured
    /// 2026-11-14, pinned like the map-data counts. The PC archive lists a few dictionaries twice
    /// (metal, generic, ...); the game keeps the first entry (CStreaming::LoadCdDirectory "appears
    /// more than once") and so does img::Archive::find. REVC_XBOXWORLD_DUMP=<file>: the list.
    #[test]
    fn real_data_count() {
        let (Some(game), Some(cache)) = (std::env::var_os("REVC_GAME_DIR"), std::env::var_os("REVC_IMPORT_DIR")) else { return };
        let cache = Path::new(&cache);
        if !cache.join("xbox").join("world").is_dir() {
            return;
        }
        let (p, r) = plan(Path::new(&game), cache).unwrap();
        if let Some(out) = std::env::var_os("REVC_XBOXWORLD_DUMP") {
            let lines: Vec<String> = p.iter().map(|(t, n, _)| format!("{t}/{n}")).collect();
            std::fs::write(out, lines.join("\n")).unwrap();
        }
        assert!(r.not_found.is_empty(), "{:?}", r.not_found);
        assert_eq!(r.filled, 138);
    }

    /// REVC_XBOXWORLD_BUILD=1 with the two folders: builds the zip into that import (to measure it).
    #[test]
    fn real_data_build() {
        let (Some(game), Some(cache), Some(_)) = (std::env::var_os("REVC_GAME_DIR"), std::env::var_os("REVC_IMPORT_DIR"), std::env::var_os("REVC_XBOXWORLD_BUILD")) else { return };
        let r = build(Path::new(&game), Path::new(&cache)).unwrap();
        assert_eq!(r.filled, 138);
        assert!(world_dir(Path::new(&cache)).join("xbox-world.zip").is_file());
    }
}
