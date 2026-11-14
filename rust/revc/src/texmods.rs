//! Texture mods: loose image files that replace textures when their dictionary loads.
//!
//! Layout under the game folder (names are case-insensitive):
//!   mods/textures/<txd>/<texture>.png|.dds   replaces <texture> in that dictionary only
//!   mods/textures/<texture>.png|.dds         replaces <texture> in any dictionary
//! `<txd>` is the dictionary's name: the model's TXD in gta3.img (e.g. `infernus`), or the file
//! name of a loose TXD (`hud`, `fonts`, `generic`, `wheels` ...). Deeper folders are allowed for
//! grouping; the folder directly containing the file names the dictionary.
//! Packs: `mods/packs/*.zip` hold the same layout inside (`textures/<txd>/<texture>.png`, or the
//! files directly under `<txd>/`); loose files win over packs, and packs are read in name order
//! (the first pack with a texture wins).
//! Any size is accepted (non-square and larger than the original included). A file that can't
//! be read or decoded is reported once and the original texture is kept.
//!
//! The C++ side (skel/host/texmods.cpp) asks for each texture of a freshly loaded dictionary.

use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_int, c_uchar};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const MAX_SIZE: u32 = 8192;

/// Where an override comes from: a loose file or a file inside a pack.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    File(PathBuf),
    Zip(crate::ziparchive::Entry),
}

impl Source {
    fn read(&self) -> Result<Vec<u8>, String> {
        match self {
            Source::File(p) => std::fs::read(p).map_err(|e| e.to_string()),
            Source::Zip(e) => e.read(),
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::File(p) => write!(f, "{}", p.display()),
            Source::Zip(e) => write!(f, "{}:{}", e.archive.display(), e.name),
        }
    }
}

type Index = HashMap<(String, String), Source>;

/// One index per UI style (0 PC, 1 PS2, 2 Xbox): style packs differ.
static INDEX: [OnceLock<Index>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
/// "exact PS2 textures" ([Graphics] Ps2WorldTextures, default on): the PS2 disc's world textures
/// in the PS2 style. Set when the settings load.
static PS2_WORLD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

#[unsafe(no_mangle)]
pub extern "C" fn revc_texmods_set_ps2_world(on: std::ffi::c_int) {
    PS2_WORLD.store(on != 0, Ordering::Relaxed);
}

/// The UI style the game shows (Graphics menu UI STYLE), set by the host.
static STYLE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// The UI style picks which style packs apply; the C++ side reloads the HUD and font textures
/// when it changes.
/// The UI style (0 PC, 1 PS2, 2 Xbox).
pub fn style() -> i32 {
    STYLE.load(Ordering::Relaxed)
}

pub fn set_style(style: i32) {
    STYLE.store(style.clamp(0, 2), Ordering::Relaxed);
}

/// The UI style a pack is for, from its file name: `pc-*`, `ps2-*`, `xbox-*` (any case); None for
/// a pack that applies in every style.
fn pack_style(zip: &Path) -> Option<i32> {
    let name = zip.file_name()?.to_string_lossy().to_ascii_lowercase();
    [("pc-", 0), ("ps2-", 1), ("xbox-", 2)].iter().find(|(p, _)| name.starts_with(p)).map(|&(_, s)| s)
}
static APPLIED: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);
static COMPRESSED: AtomicUsize = AtomicUsize::new(0);
/// Files already reported as bad (each is reported once, though it may be asked for again when
/// its dictionary is streamed in again).
static REPORTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn mods_dir() -> PathBuf {
    Path::new("mods").join("textures")
}

fn packs_dir() -> PathBuf {
    Path::new("mods").join("packs")
}

/// Adds the image files of every pack in `dir` (zips, in name order) that the index lacks.
/// Packs for this UI style first (`pc-`, `ps2-`, `xbox-` names), then the packs for every style;
/// packs for other styles are left out. In each group the first pack in name order wins.
fn add_packs(index: &mut Index, dir: &Path, style: i32) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut zips: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")))
        .filter(|p| pack_style(p).is_none_or(|s| s == style))
        .collect();
    zips.sort_by_key(|p| (pack_style(p).is_none(), p.clone()));
    for zip in zips {
        crate::discimport::note_access(&zip);
        let entries = match crate::ziparchive::list(&zip) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("texmods: {}: {e}; pack skipped", zip.display());
                continue;
            }
        };
        let mut added = 0;
        for e in entries {
            let parts: Vec<&str> = e.name.split('/').filter(|p| !p.is_empty()).collect();
            let Some((file, dirs)) = parts.split_last() else { continue };
            let Some((stem, ext)) = file.rsplit_once('.') else { continue };
            let ext = ext.to_ascii_lowercase();
            if ext != "png" && ext != "dds" {
                continue;
            }
            // The folder directly containing the file names the dictionary; a file at the top or
            // directly in `textures/` replaces the texture in any dictionary.
            let txd = match dirs.last() {
                Some(d) if !(dirs.len() == 1 && d.eq_ignore_ascii_case("textures")) => d.to_ascii_lowercase(),
                _ => String::new(),
            };
            let key = (txd, stem.to_ascii_lowercase());
            let png_over_dds = ext == "png" && matches!(index.get(&key), Some(Source::Zip(z)) if z.archive == e.archive && z.name.to_ascii_lowercase().ends_with(".dds"));
            if !index.contains_key(&key) || png_over_dds {
                index.insert(key, Source::Zip(e));
                added += 1;
            }
        }
        eprintln!("texmods: pack {}: {added} texture overrides", zip.display());
    }
}

fn build_index(root: &Path) -> Index {
    let mut index = HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if ext != "png" && ext != "dds" {
                continue;
            }
            let tex = p.file_stem().unwrap().to_string_lossy().to_ascii_lowercase();
            let txd = if dir == root {
                String::new()
            } else {
                dir.file_name().unwrap().to_string_lossy().to_ascii_lowercase()
            };
            // Prefer PNG if both exist; otherwise first found.
            let key = (txd, tex);
            if ext == "png" || !index.contains_key(&key) {
                index.insert(key, Source::File(p));
            }
        }
    }
    index
}

fn index() -> &'static Index {
    let style = STYLE.load(Ordering::Relaxed);
    INDEX[style as usize].get_or_init(|| {
        let mut idx = build_index(&mods_dir());
        if !idx.is_empty() {
            eprintln!("texmods: {} texture overrides in {}", idx.len(), mods_dir().display());
        }
        add_packs(&mut idx, &packs_dir(), style);
        // then the packs built from imported console discs (ps2pack.rs), so the player's own packs
        // win over them; the same style rule applies (ps2-* in the PS2 style only)
        let cache = crate::discimport::cache_dir();
        crate::ps2pack::ensure(&cache);
        for kind in ["ps2", "xbox"] {
            add_packs(&mut idx, &cache.join(kind).join("packs"), style);
        }
        idx
    })
}

/// The PS2 world textures (`ps2-world.zip`): an index of their own, used in the PS2 style while
/// "exact PS2 textures" is on. Apart from the main index, which is built at the first texture
/// (before the settings load), so the setting holds for every texture loaded after it.
fn world_index() -> Option<&'static Index> {
    static WORLD: OnceLock<Index> = OnceLock::new();
    if STYLE.load(Ordering::Relaxed) == 2 {
        return xbox_world_index();
    }
    if STYLE.load(Ordering::Relaxed) != 1 || !PS2_WORLD.load(Ordering::Relaxed) {
        return None;
    }
    let idx = WORLD.get_or_init(|| {
        let mut idx = Index::new();
        add_packs(&mut idx, &crate::ps2pack::world_dir(&crate::discimport::cache_dir()), 1);
        idx
    });
    (!idx.is_empty()).then_some(idx)
}

/// The Xbox style's fill-in (`xbox-world.zip`, xboxworld.rs): only textures a PC dictionary lacks
/// that the Xbox's world models use, so it adds and never replaces. Used while the Xbox world
/// models are on; built the first time from the import's Xbox dictionaries if it isn't there.
fn xbox_world_index() -> Option<&'static Index> {
    static XBOX: OnceLock<Index> = OnceLock::new();
    if !crate::ps2models::world_models_on(2) {
        return None;
    }
    let idx = XBOX.get_or_init(|| {
        let cache = crate::discimport::cache_dir();
        let dir = crate::xboxworld::world_dir(&cache);
        let zip = dir.join("xbox-world.zip");
        crate::discimport::note_access(&zip);
        if !zip.is_file() && cache.join("xbox").join("world").is_dir() {
            if let Err(e) = crate::xboxworld::build(Path::new("."), &cache) {
                eprintln!("xboxworld: {e}");
            }
        }
        let mut idx = Index::new();
        add_packs(&mut idx, &dir, 2);
        idx
    });
    (!idx.is_empty()).then_some(idx)
}

/// (textures replaced, files rejected) since start.
pub fn stats() -> (usize, usize) {
    (APPLIED.load(Ordering::Relaxed), FAILED.load(Ordering::Relaxed))
}

/// Replacements served as DXT blocks (BC textures on the GPU) rather than decoded to RGBA8.
pub fn compressed() -> usize {
    COMPRESSED.load(Ordering::Relaxed)
}

/// A DXT1/3/5 DDS that can be uploaded as it is: size a multiple of 4 (BC textures are whole
/// blocks), and mip levels in the file when the texture is sampled with mips (otherwise the
/// RGBA8 path keeps it mipmapped by generating them).
fn dds_blocks(src: &Source, want_mips: bool) -> Option<(u32, u32, u8, bool, Vec<Vec<u8>>)> {
    let data = src.read().ok()?;
    if !data.starts_with(b"DDS ") {
        return None;
    }
    let b = revc_formats::dds::blocks(&data).ok()??;
    if b.width % 4 != 0 || b.height % 4 != 0 || b.width > MAX_SIZE || b.height > MAX_SIZE {
        return None;
    }
    if want_mips && b.levels.len() < 2 {
        return None;
    }
    let levels = if want_mips { b.levels.iter().map(|l| l.to_vec()).collect() } else { vec![b.levels[0].to_vec()] };
    Some((b.width, b.height, b.kind, b.has_alpha, levels))
}

/// RGBA8, top row first.
pub(crate) fn decode(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    decode_bytes(&std::fs::read(path).map_err(|e| e.to_string())?)
}

fn decode_bytes(data: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let (w, h, rgba) = if data.starts_with(b"DDS ") {
        let img = revc_formats::dds::decode(data).map_err(|e| e.to_string())?;
        (img.width, img.height, img.rgba)
    } else {
        decode_png(data)?
    };
    if w == 0 || h == 0 || w > MAX_SIZE || h > MAX_SIZE {
        return Err(format!("size {w}x{h} outside 1..={MAX_SIZE}"));
    }
    Ok((w, h, rgba))
}

fn decode_png(data: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(data));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width, info.height);
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.as_chunks::<2>().0.iter().flat_map(|&[g, a]| [g, g, g, a]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG not expanded".into()),
    };
    if rgba.len() != (w * h * 4) as usize {
        return Err(format!("unexpected PNG layout ({:?}, {} bytes)", info.color_type, px.len()));
    }
    Ok((w, h, rgba))
}

/// An override for (folder, name) decoded to RGBA (loose file or pack): used for imported console
/// art (`ui/r3_menu_241` ...) so a pack can replace it like a texture.
pub fn find_rgba(txd: &str, tex: &str) -> Option<(u32, u32, Vec<u8>)> {
    let src = index().get(&(txd.to_ascii_lowercase(), tex.to_ascii_lowercase()))?;
    match src.read().and_then(|d| decode_bytes(&d)) {
        Ok(img) => Some(img),
        Err(e) => {
            eprintln!("texmods: {src}: {e}; using the imported original");
            None
        }
    }
}

fn lookup(txd: &str, tex: &str) -> Option<&'static Source> {
    let idx = index();
    let key = (txd.to_ascii_lowercase(), tex.to_ascii_lowercase());
    idx.get(&key)
        .or_else(|| world_index().and_then(|w| w.get(&key)))
        .or_else(|| idx.get(&(String::new(), key.1.clone())))
}

/// # Safety
/// `txd` and `texture` must be NUL-terminated strings; the out pointers must be valid.
/// If the override for txd/texture is a DXT DDS that can stay compressed, returns 1 with its
/// blocks: `levels` mip levels one after the other (largest first, each whole 4x4 blocks), `size`
/// bytes in all, freed with revc_texmod_free_blocks. `want_mips`: the texture samples mip levels.
/// Returns 0 otherwise (then revc_texmod_find decodes it to RGBA8).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_texmod_find_dxt(
    txd: *const c_char,
    texture: *const c_char,
    want_mips: c_int,
    width: *mut c_int,
    height: *mut c_int,
    kind: *mut c_int,
    levels: *mut c_int,
    has_alpha: *mut c_int,
    blocks: *mut *mut c_uchar,
    size: *mut usize,
) -> c_int {
    let (txd, tex) = unsafe { (CStr::from_ptr(txd).to_string_lossy(), CStr::from_ptr(texture).to_string_lossy()) };
    let Some(src) = lookup(&txd, &tex) else { return 0 };
    let Some((w, h, k, alpha, lv)) = dds_blocks(src, want_mips != 0) else { return 0 };
    let n = lv.len();
    let data = lv.concat().into_boxed_slice();
    unsafe {
        *width = w as c_int;
        *height = h as c_int;
        *kind = k as c_int;
        *levels = n as c_int;
        *has_alpha = alpha as c_int;
        *size = data.len();
        *blocks = Box::into_raw(data) as *mut c_uchar;
    }
    APPLIED.fetch_add(1, Ordering::Relaxed);
    COMPRESSED.fetch_add(1, Ordering::Relaxed);
    1
}

/// # Safety
/// `blocks` and `size` must come from revc_texmod_find_dxt, freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_texmod_free_blocks(blocks: *mut c_uchar, size: usize) {
    if !blocks.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(blocks, size)) });
    }
}

/// # Safety
/// `txd` and `texture` must be NUL-terminated strings.
/// 1 when txd/texture's override comes from the PS2 world pack: it then keeps the original's mip
/// state. Generated mips average thin details with what's around them; with alpha-tested
/// textures (the frame of a shelf, `mob_metal1`: an opaque border round a transparent middle)
/// the border's alpha drops below the test with distance and the bars vanish.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_texmod_from_world(txd: *const c_char, texture: *const c_char) -> c_int {
    let (txd, tex) = unsafe { (CStr::from_ptr(txd).to_string_lossy(), CStr::from_ptr(texture).to_string_lossy()) };
    let key = (txd.to_ascii_lowercase(), tex.to_ascii_lowercase());
    (!index().contains_key(&key) && world_index().is_some_and(|w| w.contains_key(&key))) as c_int
}

/// # Safety
/// `txd` a NUL-terminated string.
/// The textures the PS2 world pack has for `txd` (while it is in use), newline-separated, as a
/// string freed with revc_texmod_free_names; null if none. The PS2 models (ps2models.rs) use some
/// textures the PC dictionaries lack, which texmods.cpp adds from here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_texmod_world_names(txd: *const c_char) -> *mut c_char {
    let Some(w) = world_index() else { return std::ptr::null_mut() };
    let txd = unsafe { CStr::from_ptr(txd) }.to_string_lossy().to_ascii_lowercase();
    let mut names: Vec<&str> = w.keys().filter(|k| k.0 == txd).map(|k| k.1.as_str()).collect();
    if names.is_empty() {
        return std::ptr::null_mut();
    }
    names.sort_unstable();
    std::ffi::CString::new(names.join("\n")).map_or(std::ptr::null_mut(), |c| c.into_raw())
}

/// # Safety
/// `p` from revc_texmod_world_names.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_texmod_free_names(p: *mut c_char) {
    if !p.is_null() {
        drop(unsafe { std::ffi::CString::from_raw(p) });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_texmod_active() -> c_int {
    (!index().is_empty() || world_index().is_some()) as c_int
}

/// # Safety
/// `txd` and `texture` must be NUL-terminated strings.
/// 1 when a mod (loose file or a pack of the current UI style) replaces txd/texture: the console
/// font atlases, for one, place some icons at other character codes than the PC's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_texmod_has(txd: *const c_char, texture: *const c_char) -> c_int {
    let (txd, tex) = unsafe { (CStr::from_ptr(txd).to_string_lossy(), CStr::from_ptr(texture).to_string_lossy()) };
    lookup(&txd, &tex).is_some() as c_int
}

/// # Safety
/// `txd` and `texture` must be NUL-terminated strings; the out pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_texmod_find(
    txd: *const c_char,
    texture: *const c_char,
    width: *mut c_int,
    height: *mut c_int,
    rgba: *mut *mut c_uchar,
) -> c_int {
    let (txd, tex) = unsafe { (CStr::from_ptr(txd).to_string_lossy(), CStr::from_ptr(texture).to_string_lossy()) };
    let Some(src) = lookup(&txd, &tex) else { return 0 };
    match src.read().and_then(|d| decode_bytes(&d)) {
        Ok((w, h, px)) => {
            let boxed = px.into_boxed_slice();
            unsafe {
                *width = w as c_int;
                *height = h as c_int;
                *rgba = Box::into_raw(boxed) as *mut c_uchar;
            }
            APPLIED.fetch_add(1, Ordering::Relaxed);
            1
        }
        Err(e) => {
            let mut reported = REPORTED.lock().unwrap();
            let name = src.to_string();
            if !reported.contains(&name) {
                eprintln!("texmods: {name}: {e}; keeping the original {txd}/{tex}");
                reported.push(name);
                FAILED.fetch_add(1, Ordering::Relaxed);
            }
            0
        }
    }
}

/// # Safety
/// `rgba` must come from `revc_texmod_find` with the same width and height.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_texmod_free(rgba: *mut c_uchar, width: c_int, height: c_int) {
    if rgba.is_null() {
        return;
    }
    let len = width as usize * height as usize * 4;
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(rgba, len)) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_global_and_per_txd() {
        let root = std::env::temp_dir().join(format!("revc-texmods-{}", std::process::id()));
        std::fs::create_dir_all(root.join("HUD")).unwrap();
        std::fs::create_dir_all(root.join("group/infernus")).unwrap();
        for f in ["HUD/Fist.PNG", "radar.dds", "group/infernus/body.png", "notes.txt"] {
            std::fs::write(root.join(f), b"x").unwrap();
        }
        let idx = build_index(&root);
        assert!(idx.contains_key(&("hud".into(), "fist".into())));
        assert!(idx.contains_key(&("".into(), "radar".into())));
        assert!(idx.contains_key(&("infernus".into(), "body".into())));
        assert_eq!(idx.len(), 3);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn packs_add_what_loose_files_lack() {
        let root = std::env::temp_dir().join(format!("revc-texpacks-{}", std::process::id()));
        std::fs::create_dir_all(root.join("loose/hud")).unwrap();
        std::fs::create_dir_all(root.join("packs")).unwrap();
        std::fs::write(root.join("loose/hud/fist.png"), b"loose").unwrap();
        let zip = crate::ziparchive::tests::build(&[
            ("textures/hud/fist.png", b"packed", false),
            ("textures/hud/radardisc.png", b"disc", true),
            ("textures/radar.dds", b"any", false),
            ("fonts/font1.png", b"font", false),
            ("readme.txt", b"no", false),
        ]);
        std::fs::write(root.join("packs/b.zip"), &zip).unwrap();
        std::fs::write(root.join("packs/a.zip"), crate::ziparchive::tests::build(&[("hud/radardisc.png", b"first", false)])).unwrap();
        let mut idx = build_index(&root.join("loose"));
        add_packs(&mut idx, &root.join("packs"), 0);
        assert_eq!(idx.len(), 4);
        assert!(matches!(&idx[&("hud".into(), "fist".into())], Source::File(_)));
        assert_eq!(idx[&("hud".into(), "radardisc".into())].read().unwrap(), b"first");
        assert_eq!(idx[&("".into(), "radar".into())].read().unwrap(), b"any");
        assert_eq!(idx[&("fonts".into(), "font1".into())].read().unwrap(), b"font");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn style_packs_by_name() {
        let root = std::env::temp_dir().join(format!("revc-stylepacks-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let one = |body: &[u8]| crate::ziparchive::tests::build(&[("textures/hud/fist.png", body, false)]);
        std::fs::write(root.join("a-general.zip"), one(b"general")).unwrap();
        std::fs::write(root.join("PS2-ui.zip"), one(b"ps2")).unwrap();
        std::fs::write(root.join("xbox-ui.zip"), one(b"xbox")).unwrap();
        for (style, want) in [(0, &b"general"[..]), (1, b"ps2"), (2, b"xbox")] {
            let mut idx = Index::new();
            add_packs(&mut idx, &root, style);
            assert_eq!(idx[&("hud".into(), "fist".into())].read().unwrap(), want, "style {style}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn decodes_png_and_rejects_garbage() {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, 2, 1);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&[255, 0, 0, 0, 0, 255]).unwrap();
        }
        assert_eq!(decode_png(&out).unwrap(), (2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]));
        assert!(decode_png(b"not a png").is_err());
    }
}
