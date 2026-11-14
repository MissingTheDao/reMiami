//! The Xbox's cutscene hands configuration (`data/cutscenehands.xml` in the Xbox import): which
//! replacement hand each cutscene character gets, per side, and its colour, size, position offset
//! and finger pose. The file is XML-like but not well-formed (mixed-case tags, `<hand>` closed by
//! `</HAND>`), so it is read by a tolerant scanner. Evaluation walks the whole tree in document
//! order from defaults; every value written lands in the current side(s) and later writes win.
//! (extras/CutsceneHands.cpp uses it through revc_cshands_set_config / revc_cshands_eval.)

use std::ffi::{CStr, c_char};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimMode {
    Loop,
    Static,
}

/// One side's replacement hand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandSettings {
    pub on: bool,
    pub glove: bool,
    pub female: bool,
    pub black: bool,
    /// alpha is always 255
    pub rgb: [u8; 3],
    pub scale: [f32; 3],
    /// metres, along the forearm bone's axes
    pub position: [f32; 3],
    pub anim: AnimMode,
    /// seconds into the pose animation (static)
    pub time: f32,
}

impl Default for HandSettings {
    fn default() -> HandSettings {
        HandSettings {
            on: false,
            glove: false,
            female: false,
            black: false,
            rgb: [255; 3],
            scale: [1.0; 3],
            position: [0.0; 3],
            anim: AnimMode::Loop,
            time: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// lower-case
    pub tag: String,
    /// (lower-case name, value as written)
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Node {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
}

/// Parses `text`; returns the children of the root element `gta_xml` (empty if the first
/// element is something else). Never fails: comments, declarations and text are skipped, an end
/// tag closes the nearest open element of the same name (any case), stray end tags are ignored.
pub fn parse(text: &str) -> Vec<Node> {
    let b = text.as_bytes();
    // stack of open elements; [0] is a synthetic document node
    let mut stack: Vec<Node> = vec![Node { tag: String::new(), attrs: Vec::new(), children: Vec::new() }];
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &text[i..];
        if rest.starts_with("<!--") {
            i += rest.find("-->").map_or(rest.len(), |e| e + 3);
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            i += rest.find('>').map_or(rest.len(), |e| e + 1);
            continue;
        }
        let Some(end) = tag_end(rest) else { break };
        let inner = &rest[1..end];
        i += end + 1;
        if let Some(name) = inner.strip_prefix('/') {
            let name = name.trim().to_ascii_lowercase();
            if let Some(pos) = stack.iter().rposition(|n| n.tag == name) {
                if pos > 0 {
                    while stack.len() > pos {
                        close(&mut stack);
                    }
                }
            }
            continue;
        }
        let self_closing = inner.trim_end().ends_with('/');
        let inner = inner.trim_end().trim_end_matches('/');
        let (name, attrs) = parse_tag(inner);
        if name.is_empty() {
            continue;
        }
        stack.push(Node { tag: name, attrs, children: Vec::new() });
        if self_closing {
            close(&mut stack);
        }
    }
    while stack.len() > 1 {
        close(&mut stack);
    }
    let doc = stack.pop().unwrap();
    match doc.children.into_iter().next() {
        Some(root) if root.tag == "gta_xml" => root.children,
        _ => Vec::new(),
    }
}

/// The index of the `>` ending the tag at the start of `s` (quotes respected).
fn tag_end(s: &str) -> Option<usize> {
    let mut quoted = false;
    for (k, c) in s.char_indices().skip(1) {
        match c {
            '"' => quoted = !quoted,
            '>' if !quoted => return Some(k),
            _ => {}
        }
    }
    None
}

fn close(stack: &mut Vec<Node>) {
    let n = stack.pop().unwrap();
    stack.last_mut().unwrap().children.push(n);
}

/// `name a="1" b="x"` -> (lower-case name, lower-case attribute names with values).
fn parse_tag(inner: &str) -> (String, Vec<(String, String)>) {
    let inner = inner.trim();
    let name_end = inner.find(|c: char| c.is_whitespace()).unwrap_or(inner.len());
    let name = inner[..name_end].to_ascii_lowercase();
    let mut attrs = Vec::new();
    let mut s = &inner[name_end..];
    loop {
        s = s.trim_start();
        let Some(eq) = s.find('=') else { break };
        let key = s[..eq].trim().to_ascii_lowercase();
        let after = s[eq + 1..].trim_start();
        let Some(v) = after.strip_prefix('"') else { break };
        let Some(close) = v.find('"') else { break };
        attrs.push((key, v[..close].to_string()));
        s = &v[close + 1..];
    }
    (name, attrs)
}

/// Settings of (left, right) for `character` in a cutscene whose name, or one of whose active
/// mission names, is in `names` (all compared case-insensitively).
pub fn evaluate(roots: &[Node], names: &[&str], character: &str) -> [HandSettings; 2] {
    let mut out = [HandSettings::default(); 2];
    let mut cutscene_name = String::new();
    let mut ev = Eval { names, character, out: &mut out, cutscene_name: &mut cutscene_name };
    for n in roots {
        ev.node(n, [true, true]);
    }
    out
}

struct Eval<'a> {
    names: &'a [&'a str],
    character: &'a str,
    out: &'a mut [HandSettings; 2],
    cutscene_name: &'a mut String,
}

impl Eval<'_> {
    fn node(&mut self, n: &Node, sides: [bool; 2]) {
        match n.tag.as_str() {
            "cutscene" => {
                if let Some(name) = n.attr("name") {
                    *self.cutscene_name = name.to_string();
                }
                if self.names.iter().any(|m| m.eq_ignore_ascii_case(self.cutscene_name)) {
                    for c in &n.children {
                        self.node(c, sides);
                    }
                }
            }
            "character" => {
                if n.attr("name").is_none_or(|c| c.eq_ignore_ascii_case(self.character)) {
                    for c in &n.children {
                        self.node(c, sides);
                    }
                }
            }
            "hand" => {
                let sides = match n.attr("side") {
                    Some(s) if s.eq_ignore_ascii_case("left") => [true, false],
                    Some(s) if s.eq_ignore_ascii_case("right") => [false, true],
                    Some(_) => [true, true],
                    None => sides,
                };
                self.apply(n, sides);
                for c in &n.children {
                    self.node(c, sides);
                }
            }
            "animation" => self.apply(n, sides),
            _ => {}
        }
    }

    fn apply(&mut self, n: &Node, sides: [bool; 2]) {
        for (k, v) in &n.attrs {
            for s in 0..2 {
                if sides[s] {
                    set(&mut self.out[s], &n.tag, k, v);
                }
            }
        }
    }
}

fn set(h: &mut HandSettings, tag: &str, key: &str, v: &str) {
    let f = |x: &mut f32| {
        if let Ok(n) = v.trim().parse::<f32>() {
            *x = n;
        }
    };
    let byte = |x: &mut u8| {
        if let Ok(n) = v.trim().parse::<i64>() {
            *x = (n & 0xFF) as u8;
        }
    };
    match key {
        "model" => {
            if v.eq_ignore_ascii_case("non-skinned") {
                h.on = false;
            } else {
                h.on = true;
                h.glove = v.eq_ignore_ascii_case("skinned-glove");
            }
        }
        "gender" => h.female = v.eq_ignore_ascii_case("female"),
        "race" => h.black = v.eq_ignore_ascii_case("black"),
        "red" => byte(&mut h.rgb[0]),
        "green" => byte(&mut h.rgb[1]),
        "blue" => byte(&mut h.rgb[2]),
        "scalex" => f(&mut h.scale[0]),
        "scaley" => f(&mut h.scale[1]),
        "scalez" => f(&mut h.scale[2]),
        "positionx" => f(&mut h.position[0]),
        "positiony" => f(&mut h.position[1]),
        "positionz" => f(&mut h.position[2]),
        "type" if tag == "animation" => {
            if v.eq_ignore_ascii_case("static") {
                h.anim = AnimMode::Static;
            } else if v.eq_ignore_ascii_case("loop") {
                h.anim = AnimMode::Loop;
            }
        }
        "time" if tag == "animation" => f(&mut h.time),
        _ => {}
    }
}

// ---------------------------------------------------------------- C ABI

#[repr(C)]
pub struct RevcHandSettings {
    pub on: i32,
    pub glove: i32,
    pub female: i32,
    pub black: i32,
    /// r, g, b, 255
    pub rgb: [u8; 4],
    pub scale: [f32; 3],
    pub position: [f32; 3],
    /// 1 static, 0 loop
    pub anim_static: i32,
    pub time: f32,
}

impl From<&HandSettings> for RevcHandSettings {
    fn from(h: &HandSettings) -> RevcHandSettings {
        RevcHandSettings {
            on: h.on as i32,
            glove: h.glove as i32,
            female: h.female as i32,
            black: h.black as i32,
            rgb: [h.rgb[0], h.rgb[1], h.rgb[2], 255],
            scale: h.scale,
            position: h.position,
            anim_static: (h.anim == AnimMode::Static) as i32,
            time: h.time,
        }
    }
}

static CONFIG: Mutex<Vec<Node>> = Mutex::new(Vec::new());

/// Parses the configuration (`len` bytes) and keeps it, replacing an earlier one; returns the
/// number of top-level entries.
///
/// # Safety
/// `text` points to `len` readable bytes (or is null with `len` 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_cshands_set_config(text: *const u8, len: usize) -> i32 {
    let bytes = if text.is_null() { &[][..] } else { unsafe { std::slice::from_raw_parts(text, len) } };
    let nodes = parse(&String::from_utf8_lossy(bytes));
    let n = nodes.len() as i32;
    *CONFIG.lock().unwrap_or_else(|e| e.into_inner()) = nodes;
    n
}

fn cstr(p: *const c_char) -> String {
    if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
}

/// (left, right) settings of `character` in the cutscene `cutscene` with the active mission names
/// `missions[0..num_missions]`, into out[0] and out[1]. 0 (and defaults) without a configuration.
///
/// # Safety
/// The strings are NUL-terminated or null; `missions` holds `num_missions` pointers or is null;
/// `out` points to two writable entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_cshands_eval(cutscene: *const c_char, missions: *const *const c_char, num_missions: i32, character: *const c_char, out: *mut RevcHandSettings) -> i32 {
    let mut names = vec![cstr(cutscene)];
    if !missions.is_null() {
        for k in 0..num_missions.max(0) as usize {
            let p = unsafe { *missions.add(k) };
            if !p.is_null() {
                names.push(cstr(p));
            }
        }
    }
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let cfg = CONFIG.lock().unwrap_or_else(|e| e.into_inner());
    let have = !cfg.is_empty();
    let r = if have { evaluate(&cfg, &refs, &cstr(character)) } else { [HandSettings::default(); 2] };
    if !out.is_null() {
        unsafe {
            *out = RevcHandSettings::from(&r[0]);
            *out.add(1) = RevcHandSettings::from(&r[1]);
        }
    }
    have as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(text: &str, names: &[&str], ch: &str) -> [HandSettings; 2] {
        evaluate(&parse(text), names, ch)
    }

    #[test]
    fn defaults() {
        let r = ev("<GTA_XML></GTA_XML>", &["a"], "c");
        assert_eq!(r, [HandSettings::default(); 2]);
    }

    #[test]
    fn mismatched_case_end_tag() {
        let r = ev(r#"<GTA_XML><hand side="right" red="1"></HAND><HAND red="2"/></GTA_XML>"#, &["a"], "c");
        assert_eq!(r[1].rgb[0], 2);
        assert_eq!(r[0].rgb[0], 2);
    }

    const CFG: &str = r#"<?xml version="1.0"?>
<GTA_XML>
  <!-- global -->
  <HAND model="skinned" red="200"/>
  <CUTSCENE name="A">
    <CHARACTER name="c">
      <HAND side="right" positionZ="-0.25">
        <ANIMATION type="static" time="2.5"/>
      </HAND>
    </CHARACTER>
  </CUTSCENE>
</GTA_XML>"#;

    #[test]
    fn global_then_cutscene_override() {
        let r = ev(CFG, &["a"], "C");
        for s in &r {
            assert!(s.on);
            assert_eq!(s.rgb, [200, 255, 255]);
        }
        assert_eq!(r[1].position, [0.0, 0.0, -0.25]);
        assert_eq!((r[1].anim, r[1].time), (AnimMode::Static, 2.5));
        assert_eq!(r[0].anim, AnimMode::Loop);
        assert_eq!(r[0].position, [0.0; 3]);
        let other = ev(CFG, &["b"], "c");
        assert_eq!(other[1].anim, AnimMode::Loop);
        assert_eq!(other[1].position, [0.0; 3]);
    }

    #[test]
    fn cutscene_without_name_reuses_last() {
        let t = r#"<GTA_XML><CUTSCENE name="x"></CUTSCENE><CUTSCENE><HAND red="7"/></CUTSCENE></GTA_XML>"#;
        assert_eq!(ev(t, &["X"], "c")[0].rgb[0], 7);
        assert_eq!(ev(t, &["y"], "c")[0].rgb[0], 255);
    }

    #[test]
    fn character_without_name_matches_all() {
        let t = r#"<GTA_XML><CUTSCENE name="x"><CHARACTER><HAND blue="9"/></CHARACTER></CUTSCENE></GTA_XML>"#;
        assert_eq!(ev(t, &["x"], "anyone")[1].rgb[2], 9);
    }

    #[test]
    fn model_values() {
        let t = r#"<GTA_XML><HAND model="skinned"/><HAND side="left" model="NON-SKINNED"/><HAND side="right" model="SKINNED-GLOVE"/></GTA_XML>"#;
        let r = ev(t, &["a"], "c");
        assert!(!r[0].on);
        assert!(r[1].on && r[1].glove);
    }

    #[test]
    fn unknown_elements_and_comments_ignored() {
        let t = r#"<?xml version="1.0"?><GTA_XML><!-- <HAND red="1"/> --><FOO><HAND red="2"/></FOO><HAND green="3"/></GTA_XML>"#;
        let r = ev(t, &["a"], "c");
        assert_eq!(r[0].rgb, [255, 3, 255]);
    }

    #[test]
    fn bad_number_keeps_value() {
        let t = r#"<GTA_XML><HAND red="40"/><HAND red="x" scaleX="?"/></GTA_XML>"#;
        let r = ev(t, &["a"], "c");
        assert_eq!(r[0].rgb[0], 40);
        assert_eq!(r[0].scale[0], 1.0);
    }

    #[test]
    fn root_must_be_gta_xml() {
        assert!(parse(r#"<OTHER><HAND red="1"/></OTHER>"#).is_empty());
    }

    #[test]
    fn mission_name_and_ffi() {
        let t = r#"<GTA_XML><CUTSCENE name="gspot"><CHARACTER name="csplay"><HAND side="right" model="skinned" red="12"/></CHARACTER></CUTSCENE></GTA_XML>"#;
        let mut out = [RevcHandSettings::from(&HandSettings::default()), RevcHandSettings::from(&HandSettings::default())];
        unsafe {
            assert_eq!(revc_cshands_set_config(t.as_ptr(), t.len()), 1);
            let cut = c"int_b";
            let ch = c"CSPLAY";
            let m = [c"intro".as_ptr(), std::ptr::null(), c"GSPOT".as_ptr()];
            assert_eq!(revc_cshands_eval(cut.as_ptr(), m.as_ptr(), 3, ch.as_ptr(), out.as_mut_ptr()), 1);
            assert_eq!((out[1].on, out[1].rgb), (1, [12, 255, 255, 255]));
            assert_eq!(out[0].on, 0);
            assert_eq!(revc_cshands_eval(cut.as_ptr(), std::ptr::null(), 0, ch.as_ptr(), out.as_mut_ptr()), 1);
            assert_eq!(out[1].on, 0);
            // without a configuration: 0 and defaults
            revc_cshands_set_config(std::ptr::null(), 0);
            assert_eq!(revc_cshands_eval(cut.as_ptr(), std::ptr::null(), 0, ch.as_ptr(), out.as_mut_ptr()), 0);
        }
    }

    /// The real file (local import, skipped without it): every cutscene and character in it
    /// evaluates, and the opening hotel cutscene's player gets both hands (values not quoted here:
    /// they are the game's data).
    #[test]
    fn real_config() {
        let Some(p) = std::env::var_os("REVC_XBOX_HANDS_XML") else { return };
        let text = std::fs::read_to_string(p).unwrap();
        let roots = parse(&text);
        assert!(!roots.is_empty());
        let r = evaluate(&roots, &["int_b"], "csplay");
        assert!(r[0].on && r[1].on);
        assert!(r.iter().all(|h| h.scale.iter().all(|&s| s > 0.5 && s < 2.0)));
    }
}
