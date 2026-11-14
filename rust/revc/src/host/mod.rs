//! The SDL3 platform host. Replaces GLFW for the C++ game:
//!  - `revc_host_window_api()` gives librw's HOST backend its window, GL context,
//!    buffer swaps and video modes (rw::gl3::HostWindowApi in vendor/librw/src/gl/rwgl3.h);
//!  - the `host*` functions implement skel/hostapi.h (mouse, window, joysticks, event pump).
//!
//! Everything runs on the game's main thread. State lives in a thread-local `RefCell`;
//! game callbacks (`revc_on_*`) are invoked only after that borrow is released, because
//! the C++ side calls back into `host*` from inside them.

mod geometry;
mod joystick;
mod keymap;
mod glyphs;
mod pads;
pub(crate) mod wgpu;

pub use pads::PadKind;

use geometry::Geometry;
use joystick::{HostGamepadState, Joysticks};
use sdl3_sys::everything::*;
use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char, c_double, c_int, c_void};
use std::ptr::{null, null_mut};

// ---------------------------------------------------------------- C++ side

unsafe extern "C" {
    fn revc_on_key(key: c_int, down: c_int);
    fn revc_on_resize(width: c_int, height: c_int);
    fn revc_on_scroll(dx: c_double, dy: c_double);
    fn revc_on_cursor(x: c_double, y: c_double);
    fn revc_on_cursor_enter(entered: c_int);
    fn revc_on_focus(focused: c_int);
    fn revc_on_iconify(iconified: c_int);
    fn revc_on_joystick(jid: c_int, connected: c_int);
    fn revc_on_toggle_fullscreen();
}

const HOST_CURSOR_NORMAL: c_int = 0;
const HOST_CURSOR_HIDDEN: c_int = 1;
const HOST_CURSOR_DISABLED: c_int = 2;
const VIDEOMODEEXCLUSIVE: u32 = 1; // rw::VideoModeFlag

/// rw::gl3::HostVideoMode
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct HostVideoMode {
    width: i32,
    height: i32,
    depth: i32,
    refresh_rate: i32,
    flags: u32,
}

/// rw::wgpu::HostWindowApi
#[repr(C)]
pub struct HostWindowApi {
    user: *mut c_void,
    num_displays: unsafe extern "C" fn(*mut c_void) -> i32,
    display_name: unsafe extern "C" fn(*mut c_void, i32) -> *const c_char,
    num_video_modes: unsafe extern "C" fn(*mut c_void, i32) -> i32,
    get_video_mode: unsafe extern "C" fn(*mut c_void, i32, i32, *mut HostVideoMode) -> i32,
    create_window: unsafe extern "C" fn(
        *mut c_void, *const c_char, i32, *const HostVideoMode, i32, i32, i32,
    ) -> *mut c_void,
    destroy_window: unsafe extern "C" fn(*mut c_void, *mut c_void),
    get_drawable_size: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut i32, *mut i32),
}
// SAFETY: contains only function pointers and a null user pointer.
unsafe impl Sync for HostWindowApi {}

// ---------------------------------------------------------------- state

struct Display {
    id: SDL_DisplayID,
    name: CString,
    /// Fullscreen modes, deduplicated by size (highest refresh rate kept). Mode index i+1.
    modes: Vec<SDL_DisplayMode>,
}

struct Host {
    initialised: bool,
    displays: Vec<Display>,
    window: *mut SDL_Window,
    should_close: bool,
    cursor_mode: c_int,
    /// Virtual cursor in window coordinates. In DISABLED (relative) mode it is
    /// unbounded and fed by motion deltas, like GLFW's disabled cursor.
    cursor: (f64, f64),
    mouse_buttons: [bool; 5],
    focused: bool,
    joysticks: Joysticks,
    /// The input device the player used last; prompts show its glyphs (plan task 9).
    active_kind: PadKind,
    /// Slot of the pad used last (for its button labels); None until a pad is used.
    active_pad: Option<i32>,
    /// Windowed-mode geometry, remembered across launches.
    geometry: Geometry,
    geometry_dirty: bool,
    /// Regression harness: read back the next presented frame.
    capture_requested: bool,
    captured: Option<(u32, u32, Vec<u8>)>,
}

impl Default for Host {
    fn default() -> Self {
        Host {
            initialised: false,
            displays: Vec::new(),
            window: null_mut(),
            should_close: false,
            cursor_mode: HOST_CURSOR_NORMAL,
            cursor: (0.0, 0.0),
            mouse_buttons: [false; 5],
            focused: true,
            joysticks: Joysticks::default(),
            active_kind: PadKind::KeyboardMouse,
            active_pad: None,
            geometry: Geometry::default(),
            geometry_dirty: false,
            capture_requested: false,
            captured: None,
        }
    }
}

thread_local! {
    static HOST: RefCell<Host> = RefCell::new(Host::default());
}

fn with<R>(f: impl FnOnce(&mut Host) -> R) -> R {
    HOST.with(|h| f(&mut h.borrow_mut()))
}

pub fn sdl_error() -> String {
    unsafe { CStr::from_ptr(SDL_GetError()) }.to_string_lossy().into_owned()
}

/// Initialise SDL. Called from main() before the game starts.
pub fn init() -> Result<(), String> {
    unsafe {
        // Keep the game's unscaled window-coordinate mouse maths valid.
        SDL_SetHint(SDL_HINT_VIDEO_WAYLAND_SCALE_TO_DISPLAY, c"0".as_ptr());
        if !SDL_Init(SDL_INIT_VIDEO | SDL_INIT_JOYSTICK | SDL_INIT_GAMEPAD) {
            return Err(sdl_error());
        }
    }
    with(|h| {
        h.initialised = true;
        h.focused = true;
        h.displays = enumerate_displays();
        h.geometry = Geometry::load();
        // Open joysticks that are already connected, so _InputInitialiseJoys() sees them
        // (their JOYSTICK_ADDED events are then ignored as duplicates).
        let mut n: c_int = 0;
        let ids = unsafe { SDL_GetJoysticks(&mut n) };
        if !ids.is_null() {
            for i in 0..n as usize {
                h.joysticks.add(unsafe { *ids.add(i) });
            }
            unsafe { SDL_free(ids.cast()) };
        }
    });
    Ok(())
}

/// Tear down SDL objects. Called from main() after the game returns.
pub fn shutdown() {
    crate::audio::close_device();
    with(|h| {
        if h.geometry_dirty {
            h.geometry.save();
        }
        h.joysticks = Joysticks::default(); // closes devices
        unsafe {
            if !h.window.is_null() {
                SDL_DestroyWindow(h.window);
            }
            SDL_Quit();
        }
        h.window = null_mut();
        h.initialised = false;
    });
}

fn enumerate_displays() -> Vec<Display> {
    let mut out = Vec::new();
    let mut n: c_int = 0;
    let ids = unsafe { SDL_GetDisplays(&mut n) };
    if ids.is_null() {
        return out;
    }
    // Primary display first, so display 0 is what the game picks by default.
    let primary = unsafe { SDL_GetPrimaryDisplay() };
    let mut list: Vec<SDL_DisplayID> = (0..n as usize).map(|i| unsafe { *ids.add(i) }).collect();
    unsafe { SDL_free(ids.cast()) };
    list.sort_by_key(|id| *id != primary);

    for id in list {
        let name = unsafe { SDL_GetDisplayName(id) };
        let name = if name.is_null() {
            c"Display".to_owned()
        } else {
            unsafe { CStr::from_ptr(name) }.to_owned()
        };
        let mut modes: Vec<SDL_DisplayMode> = Vec::new();
        let mut count: c_int = 0;
        let list = unsafe { SDL_GetFullscreenDisplayModes(id, &mut count) };
        if !list.is_null() {
            for i in 0..count as usize {
                // SAFETY: SDL_DisplayMode is plain data.
                let m = unsafe { std::ptr::read(*list.add(i)) };
                match modes.iter_mut().find(|e| e.w == m.w && e.h == m.h) {
                    Some(e) if m.refresh_rate > e.refresh_rate => *e = m,
                    Some(_) => {}
                    None => modes.push(m),
                }
            }
            unsafe { SDL_free(list.cast()) };
        }
        out.push(Display { id, name, modes });
    }
    out
}

// ---------------------------------------------------------------- window API (librw)

/// Put a windowed window where it was last time (position is ignored on Wayland).
fn restore_windowed_placement(win: *mut SDL_Window, g: &Geometry) {
    unsafe {
        match g.pos {
            Some((x, y)) => SDL_SetWindowPosition(win, x, y),
            None => SDL_SetWindowPosition(win, SDL_WINDOWPOS_CENTERED, SDL_WINDOWPOS_CENTERED),
        };
        if g.maximized {
            SDL_MaximizeWindow(win);
        }
    }
}

/// Record windowed size/position/maximized state from the live window.
fn track_geometry(h: &mut Host) {
    if h.window.is_null() {
        return;
    }
    let flags = unsafe { SDL_GetWindowFlags(h.window) };
    if flags.0 & SDL_WINDOW_FULLSCREEN.0 != 0 || flags.0 & SDL_WINDOW_MINIMIZED.0 != 0 {
        return;
    }
    let maximized = flags.0 & SDL_WINDOW_MAXIMIZED.0 != 0;
    let mut g = h.geometry;
    g.maximized = maximized;
    if !maximized {
        let (mut w, mut ht, mut x, mut y) = (0, 0, 0, 0);
        unsafe {
            SDL_GetWindowSize(h.window, &mut w, &mut ht);
            if SDL_GetWindowPosition(h.window, &mut x, &mut y) && (x, y) != (0, 0) {
                g.pos = Some((x, y));
            }
        }
        if w > 0 && ht > 0 {
            g.size = Some((w, ht));
        }
    }
    if g != h.geometry {
        h.geometry = g;
        h.geometry_dirty = true;
    }
}

unsafe extern "C" fn api_num_displays(_: *mut c_void) -> i32 {
    with(|h| h.displays.len().max(1) as i32)
}

unsafe extern "C" fn api_display_name(_: *mut c_void, display: i32) -> *const c_char {
    with(|h| h.displays.get(display as usize).map_or(null(), |d| d.name.as_ptr()))
}

unsafe extern "C" fn api_num_video_modes(_: *mut c_void, display: i32) -> i32 {
    with(|h| h.displays.get(display as usize).map_or(1, |d| d.modes.len() as i32 + 1))
}

unsafe extern "C" fn api_get_video_mode(_: *mut c_void, display: i32, index: i32, out: *mut HostVideoMode) -> i32 {
    let mode = with(|h| {
        let d = h.displays.get(display as usize)?;
        if index == 0 {
            let m = unsafe { SDL_GetDesktopDisplayMode(d.id) };
            if m.is_null() {
                return None;
            }
            let m = unsafe { &*m };
            Some(HostVideoMode { width: m.w, height: m.h, depth: 32, refresh_rate: m.refresh_rate as i32, flags: 0 })
        } else {
            let m = d.modes.get(index as usize - 1)?;
            Some(HostVideoMode {
                width: m.w,
                height: m.h,
                depth: 32,
                refresh_rate: m.refresh_rate as i32,
                flags: VIDEOMODEEXCLUSIVE,
            })
        }
    });
    match mode {
        Some(m) => {
            unsafe { *out = m };
            1
        }
        None => 0,
    }
}

unsafe extern "C" fn api_create_window(
    _: *mut c_void,
    title: *const c_char,
    display: i32,
    mode: *const HostVideoMode,
    width: i32,
    height: i32,
    samples: i32,
) -> *mut c_void {
    let mode = unsafe { *mode };
    let fullscreen = mode.flags & VIDEOMODEEXCLUSIVE != 0;
    // Points into Host::displays, which is not modified while the window exists.
    let sdl_mode: *const SDL_DisplayMode = with(|h| {
        h.displays
            .get(display as usize)
            .and_then(|d| d.modes.iter().find(|m| m.w == mode.width && m.h == mode.height))
            .map_or(null(), |m| m as *const _)
    });
    let remembered = with(|host| host.geometry);
    let (w, h) = if fullscreen { (mode.width, mode.height) } else { remembered.size.unwrap_or((width, height)) };

    // revc-rw (wgpu) presents to the window.
    let win = unsafe { SDL_CreateWindow(title, w, h, SDL_WINDOW_RESIZABLE) };
    if win.is_null() {
        eprintln!("revc: SDL_CreateWindow failed: {}", sdl_error());
        return null_mut();
    }
    if !fullscreen {
        restore_windowed_placement(win, &remembered);
    } else {
        unsafe {
            SDL_SetWindowFullscreenMode(win, sdl_mode);
            SDL_SetWindowFullscreen(win, true);
        }
    }
    if let Err(e) = wgpu::create(win, samples) {
        eprintln!("revc: {e}");
        unsafe { SDL_DestroyWindow(win) };
        return null_mut();
    }
    with(|host| {
        host.window = win;
        host.cursor = (w as f64 / 2.0, h as f64 / 2.0);
    });
    win.cast()
}

unsafe extern "C" fn api_destroy_window(_: *mut c_void, win: *mut c_void) {
    wgpu::destroy();
    with(|h| unsafe {
        SDL_DestroyWindow(win.cast());
        if h.window == win.cast() {
            h.window = null_mut();
        }
    });
}

unsafe extern "C" fn api_get_drawable_size(_: *mut c_void, win: *mut c_void, w: *mut i32, h: *mut i32) {
    unsafe { SDL_GetWindowSizeInPixels(win.cast(), w, h) };
}

/// Regression harness: capture the next frame that is presented.
pub fn request_capture() {
    with(|h| h.capture_requested = true);
}

/// Regression harness: the captured frame, once available.
pub fn take_capture() -> Option<(u32, u32, Vec<u8>)> {
    with(|h| h.captured.take())
}

static SCREENSHOT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The game's screenshot key (F12, or the second pad's left stick click; core/main.cpp): the next
/// presented frame is saved as screen_<unix time>.png in the game folder, as the original did.
/// (librw's camera readback has no pixels in this renderer, so it goes through the capture.)
#[unsafe(no_mangle)]
pub extern "C" fn revc_screenshot_request() {
    // The key reads as "just down" for a whole 30 Hz step and the request comes from the end of
    // every drawn frame, so several frames of one step ask: one screenshot per press.
    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(250)) {
        return;
    }
    *last = Some(std::time::Instant::now());
    SCREENSHOT.store(true, std::sync::atomic::Ordering::Relaxed);
    request_capture();
}

/// Writes a requested screenshot once its frame has been captured (main loop, every frame).
pub fn poll_screenshot() {
    if !SCREENSHOT.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let Some((w, h, rgba)) = take_capture() else { return };
    SCREENSHOT.store(false, std::sync::atomic::Ordering::Relaxed);
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let path = format!("screen_{secs:011}.png");
    let write = || -> Result<(), String> {
        let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().map_err(|e| e.to_string())?;
        wr.write_image_data(&rgba).map_err(|e| e.to_string())
    };
    match write() {
        Ok(()) => eprintln!("revc: screenshot {path}"),
        Err(e) => eprintln!("revc: screenshot {path}: {e}"),
    }
}

static WINDOW_API: HostWindowApi = HostWindowApi {
    user: null_mut(),
    num_displays: api_num_displays,
    display_name: api_display_name,
    num_video_modes: api_num_video_modes,
    get_video_mode: api_get_video_mode,
    create_window: api_create_window,
    destroy_window: api_destroy_window,
    get_drawable_size: api_get_drawable_size,
};

#[unsafe(no_mangle)]
pub extern "C" fn revc_host_window_api() -> *const HostWindowApi {
    &WINDOW_API
}

// ---------------------------------------------------------------- events

/// A game callback, queued while the host state is borrowed.
enum Cb {
    Key(c_int, bool),
    Resize(c_int, c_int),
    Scroll(f64, f64),
    Cursor(f64, f64),
    CursorEnter(bool),
    Focus(bool),
    Iconify(bool),
    Joystick(c_int, bool),
    ToggleFullscreen,
}

/// SDL mouse button number -> hostapi.h HOST_MOUSE_BUTTON_* index.
fn mouse_index(button: u8) -> Option<usize> {
    match button as i32 {
        1 => Some(0), // left
        3 => Some(1), // right
        2 => Some(2), // middle
        4 => Some(3), // x1
        5 => Some(4), // x2
        _ => None,
    }
}

fn pump(h: &mut Host, out: &mut Vec<Cb>) {
    let mut ev = SDL_Event::default();
    while unsafe { SDL_PollEvent(&mut ev) } {
        let ty = SDL_EventType(unsafe { ev.r#type });
        match ty {
            SDL_EVENT_QUIT | SDL_EVENT_WINDOW_CLOSE_REQUESTED => h.should_close = true,

            SDL_EVENT_KEY_DOWN | SDL_EVENT_KEY_UP => {
                let k = unsafe { ev.key };
                if k.down {
                    h.active_kind = PadKind::KeyboardMouse;
                }
                let enter = k.scancode == SDL_SCANCODE_RETURN || k.scancode == SDL_SCANCODE_KP_ENTER;
                if enter && (k.r#mod.0 & SDL_KMOD_ALT.0) != 0 {
                    // Alt+Enter toggles fullscreen; the Enter itself is swallowed so it
                    // doesn't reach the game (it is "enter vehicle" on foot).
                    if k.down && !k.repeat {
                        out.push(Cb::ToggleFullscreen);
                    }
                } else if !k.repeat {
                    out.push(Cb::Key(keymap::rs_key_from_scancode(k.scancode), k.down));
                }
            }

            SDL_EVENT_MOUSE_MOTION => {
                let m = unsafe { ev.motion };
                if h.cursor_mode == HOST_CURSOR_DISABLED {
                    h.cursor.0 += m.xrel as f64;
                    h.cursor.1 += m.yrel as f64;
                } else {
                    h.cursor = (m.x as f64, m.y as f64);
                }
                out.push(Cb::Cursor(h.cursor.0, h.cursor.1));
            }
            SDL_EVENT_MOUSE_BUTTON_DOWN | SDL_EVENT_MOUSE_BUTTON_UP => {
                let b = unsafe { ev.button };
                if b.down {
                    h.active_kind = PadKind::KeyboardMouse;
                }
                if let Some(i) = mouse_index(b.button) {
                    h.mouse_buttons[i] = b.down;
                }
            }
            SDL_EVENT_MOUSE_WHEEL => {
                let w = unsafe { ev.wheel };
                h.active_kind = PadKind::KeyboardMouse;
                out.push(Cb::Scroll(w.x as f64, w.y as f64));
            }

            SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED => {
                let w = unsafe { ev.window };
                out.push(Cb::Resize(w.data1, w.data2));
            }
            SDL_EVENT_WINDOW_RESIZED | SDL_EVENT_WINDOW_MOVED | SDL_EVENT_WINDOW_MAXIMIZED => track_geometry(h),
            SDL_EVENT_WINDOW_MOUSE_ENTER => out.push(Cb::CursorEnter(true)),
            SDL_EVENT_WINDOW_MOUSE_LEAVE => {
                // A captured (relative) cursor never "leaves", same as GLFW's disabled cursor.
                if h.cursor_mode != HOST_CURSOR_DISABLED {
                    out.push(Cb::CursorEnter(false));
                }
            }
            SDL_EVENT_WINDOW_FOCUS_GAINED => {
                h.focused = true;
                out.push(Cb::Focus(true));
                if h.cursor_mode == HOST_CURSOR_DISABLED {
                    out.push(Cb::CursorEnter(true));
                }
            }
            SDL_EVENT_WINDOW_FOCUS_LOST => {
                h.focused = false;
                h.mouse_buttons = [false; 5];
                out.push(Cb::Focus(false));
            }
            SDL_EVENT_WINDOW_MINIMIZED => out.push(Cb::Iconify(true)),
            SDL_EVENT_WINDOW_RESTORED => {
                track_geometry(h);
                out.push(Cb::Iconify(false));
            }

            SDL_EVENT_JOYSTICK_BUTTON_DOWN | SDL_EVENT_JOYSTICK_HAT_MOTION => {
                // jbutton/jhat both start with the device id.
                let id = unsafe { ev.jbutton.which };
                if let Some(k) = h.joysticks.kind_of(id) {
                    h.active_kind = k;
                    h.active_pad = h.joysticks.slot_of(id);
                }
            }
            SDL_EVENT_JOYSTICK_AXIS_MOTION => {
                let a = unsafe { ev.jaxis };
                if a.value.unsigned_abs() > pads::ACTIVE_AXIS_THRESHOLD as u16
                    && let Some(k) = h.joysticks.kind_of(a.which)
                {
                    h.active_kind = k;
                    h.active_pad = h.joysticks.slot_of(a.which);
                }
            }
            SDL_EVENT_JOYSTICK_ADDED => {
                let id = unsafe { ev.jdevice.which };
                if let Some(slot) = h.joysticks.add(id) {
                    out.push(Cb::Joystick(slot as c_int, true));
                }
            }
            SDL_EVENT_JOYSTICK_REMOVED => {
                let id = unsafe { ev.jdevice.which };
                if let Some(slot) = h.joysticks.remove(id) {
                    out.push(Cb::Joystick(slot as c_int, false));
                }
            }
            _ => {}
        }
    }
}

/// Sends a key press/release to the game exactly like a real key event (regression harness).
/// `key` is an RsKeyCodes value (see host/keymap.rs).
pub fn inject_key(key: c_int, down: bool) {
    // SAFETY: game callback on the main thread; no host borrow is held here.
    unsafe { revc_on_key(key, down as c_int) };
}

/// Processes window/input events and dispatches them to the game. Called by the main
/// loop before every revc_game_frame().
pub fn poll_events() {
    let mut cbs = Vec::new();
    with(|h| pump(h, &mut cbs));
    for cb in cbs {
        // SAFETY: plain C entry points in skel/host/host.cpp; no host borrow is held here.
        unsafe {
            match cb {
                Cb::Key(k, d) => revc_on_key(k, d as c_int),
                Cb::Resize(w, h) => revc_on_resize(w, h),
                Cb::Scroll(x, y) => revc_on_scroll(x, y),
                Cb::Cursor(x, y) => revc_on_cursor(x, y),
                Cb::CursorEnter(e) => revc_on_cursor_enter(e as c_int),
                Cb::Focus(f) => revc_on_focus(f as c_int),
                Cb::Iconify(i) => revc_on_iconify(i as c_int),
                Cb::Joystick(j, c) => revc_on_joystick(j, c as c_int),
                Cb::ToggleFullscreen => revc_on_toggle_fullscreen(),
            }
        }
    }
    crate::testdrive::tick();
}

#[unsafe(no_mangle)]
pub extern "C" fn hostWindowShouldClose() -> c_int {
    with(|h| h.should_close as c_int)
}

// ---------------------------------------------------------------- mouse / window

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostGetCursorPos(x: *mut c_double, y: *mut c_double) {
    let (cx, cy) = with(|h| h.cursor);
    unsafe {
        if !x.is_null() {
            *x = cx;
        }
        if !y.is_null() {
            *y = cy;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hostSetCursorPos(x: c_double, y: c_double) {
    with(|h| {
        h.cursor = (x, y);
        // Only a visible-but-free cursor needs a real warp. In relative mode the position is
        // virtual (this is what makes mouse look work on Wayland, where warping is refused).
        if h.cursor_mode != HOST_CURSOR_DISABLED && h.focused && !h.window.is_null() {
            unsafe { SDL_WarpMouseInWindow(h.window, x as f32, y as f32) };
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn hostGetMouseButton(button: c_int) -> c_int {
    with(|h| h.mouse_buttons.get(button as usize).copied().unwrap_or(false) as c_int)
}

#[unsafe(no_mangle)]
pub extern "C" fn hostSetCursorMode(mode: c_int) {
    let entered = with(|h| {
        if h.window.is_null() {
            h.cursor_mode = mode;
            return false;
        }
        let was_relative = h.cursor_mode == HOST_CURSOR_DISABLED;
        h.cursor_mode = mode;
        unsafe {
            match mode {
                HOST_CURSOR_DISABLED => {
                    SDL_SetWindowRelativeMouseMode(h.window, true);
                }
                HOST_CURSOR_HIDDEN => {
                    SDL_SetWindowRelativeMouseMode(h.window, false);
                    SDL_HideCursor();
                }
                _ => {
                    SDL_SetWindowRelativeMouseMode(h.window, false);
                    SDL_ShowCursor();
                }
            }
        }
        if was_relative && mode != HOST_CURSOR_DISABLED {
            // Leave the virtual position where the real cursor is.
            let (mut x, mut y) = (0f32, 0f32);
            unsafe { SDL_GetMouseState(&mut x, &mut y) };
            h.cursor = (x as f64, y as f64);
        }
        mode == HOST_CURSOR_DISABLED && h.focused
    });
    if entered {
        // SAFETY: no host borrow held.
        unsafe { revc_on_cursor_enter(1) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostGetWindowSize(w: *mut c_int, ht: *mut c_int) {
    with(|h| unsafe {
        *w = 0;
        *ht = 0;
        if !h.window.is_null() {
            SDL_GetWindowSize(h.window, w, ht);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn hostSetWindowSize(w: c_int, ht: c_int) {
    with(|h| {
        if !h.window.is_null() {
            unsafe {
                // A maximized window ignores size requests.
                if SDL_GetWindowFlags(h.window).0 & SDL_WINDOW_MAXIMIZED.0 != 0 {
                    SDL_RestoreWindow(h.window);
                }
                SDL_SetWindowSize(h.window, w, ht);
            }
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn hostSetFullscreen(fullscreen: c_int, width: c_int, height: c_int) -> c_int {
    with(|h| {
        if h.window.is_null() {
            return 0;
        }
        let win = h.window;
        let ok = unsafe {
            if fullscreen != 0 {
                // Exclusive mode of that size on the window's display, else borderless desktop.
                let display = SDL_GetDisplayForWindow(win);
                let mode: *const SDL_DisplayMode = h
                    .displays
                    .iter()
                    .find(|d| d.id == display)
                    .and_then(|d| d.modes.iter().find(|m| m.w == width && m.h == height))
                    .map_or(null(), |m| m as *const _);
                SDL_SetWindowFullscreenMode(win, mode) && SDL_SetWindowFullscreen(win, true)
            } else {
                let ok = SDL_SetWindowFullscreen(win, false);
                if ok {
                    let (w, ht) = h.geometry.size.unwrap_or((width, height));
                    SDL_SetWindowSize(win, w, ht);
                    restore_windowed_placement(win, &h.geometry);
                }
                ok
            }
        };
        if !ok {
            eprintln!("revc: fullscreen switch failed: {}", sdl_error());
            return 0;
        }
        unsafe { SDL_SyncWindow(win) };
        if h.geometry_dirty {
            h.geometry.save();
            h.geometry_dirty = false;
        }
        1
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostGetDesktopMode(w: *mut c_int, ht: *mut c_int, hz: *mut c_int) {
    let m = unsafe { SDL_GetDesktopDisplayMode(SDL_GetPrimaryDisplay()) };
    let (mw, mh, mr) = if m.is_null() {
        (1280, 720, 60)
    } else {
        let m = unsafe { &*m };
        (m.w, m.h, m.refresh_rate as c_int)
    };
    unsafe {
        *w = mw;
        *ht = mh;
        *hz = mr;
    }
}

// ---------------------------------------------------------------- fixed-timestep clock

thread_local! {
    static SIM_CLOCK: RefCell<(revc_timing::FixedStep, Option<std::time::Instant>)> =
        RefCell::new((revc_timing::FixedStep::new(30, 4), None));
}

/// Motion captures (REVC_TEST=motion) keep the render interpolation of the captured frame.
pub static CAPTURE_KEEPS_ALPHA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn fixed_frame_time() -> Option<std::time::Duration> {
    static FT: std::sync::OnceLock<Option<std::time::Duration>> = std::sync::OnceLock::new();
    *FT.get_or_init(|| {
        let fps: f64 = std::env::var("REVC_FIXED_FPS").ok()?.parse().ok()?;
        (fps >= 1.0).then(|| std::time::Duration::from_secs_f64(1.0 / fps))
    })
}

/// Non-zero while the regression harness waits for this frame to be captured.
#[unsafe(no_mangle)]
pub extern "C" fn hostCaptureRequested() -> c_int {
    with(|h| h.capture_requested) as c_int
}

/// 30 Hz simulation steps due this frame (see revc_timing::FixedStep); `alpha` gets the
/// position between the last step and the next one, for render interpolation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostSimStepsDue(alpha: *mut f32) -> c_int {
    let (steps, a) = SIM_CLOCK.with(|c| {
        let mut c = c.borrow_mut();
        let now = std::time::Instant::now();
        let elapsed = match fixed_frame_time() {
            // REVC_FIXED_FPS=N (tests): every frame is exactly 1/N s, whatever the wall clock, so
            // which frames fall between which simulation steps is the same in every run.
            Some(dt) => dt,
            None => c.1.map_or(std::time::Duration::ZERO, |t| now - t),
        };
        c.1 = Some(now);
        c.0.advance(elapsed)
    });
    // A frame captured by the regression harness shows the simulated state itself (alpha 1),
    // not a blend that depends on wall-clock timing, so screenshots are repeatable; motion
    // captures (videos) keep the blend.
    let a = if with(|h| h.capture_requested) && !CAPTURE_KEEPS_ALPHA.load(std::sync::atomic::Ordering::Relaxed) { 1.0 } else { a };
    if !alpha.is_null() {
        unsafe { *alpha = a };
    }
    steps as c_int
}

// ---------------------------------------------------------------- joysticks

/// Glyph set of the input device used last (a PadKind value, see host/pads.rs).
#[unsafe(no_mangle)]
pub extern "C" fn hostActiveInputKind() -> c_int {
    active_kind() as c_int
}

pub fn active_kind() -> PadKind {
    // REVC_TEST_INPUT_KIND=n (tests only): pretend a PadKind is the input in use, to capture
    // menus and prompts as a controller player sees them.
    static FORCED: std::sync::OnceLock<Option<i32>> = std::sync::OnceLock::new();
    let forced = *FORCED.get_or_init(|| {
        std::env::var_os("REVC_TEST")?;
        std::env::var("REVC_TEST_INPUT_KIND").ok()?.parse().ok()
    });
    if let Some(k) = forced.and_then(PadKind::from_i32) {
        return k;
    }
    with(|h| h.active_kind)
}

/// Controls-menu override of the glyph style (0 = follow the pad in use).
static GLYPH_OVERRIDE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn revc_glyph_set_override(style: c_int) {
    GLYPH_OVERRIDE.store(style, std::sync::atomic::Ordering::Relaxed);
}

/// UI style (0 PC, 1 PS2, 2 Xbox; Graphics menu UI STYLE): picks the glyph variant.
static UI_STYLE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn revc_ui_style_set(style: c_int) {
    UI_STYLE.store(style, std::sync::atomic::Ordering::Relaxed);
    crate::texmods::set_style(style);
}

fn ui_style() -> glyphs::UiStyle {
    glyphs::UiStyle::from_index(UI_STYLE.load(std::sync::atomic::Ordering::Relaxed))
}

/// The glyph set prompts should show: the menu override if set, else the pad used last, or
/// failing that the first connected pad. (kind, face labels); None without pads and override.
fn glyph_pad() -> Option<(PadKind, glyphs::FaceLabels)> {
    if let Some(forced) = glyphs::override_style(GLYPH_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed)) {
        return Some(forced);
    }
    with(|h| {
        let slot = h.active_pad.filter(|&s| h.joysticks.present(s)).or_else(|| (0..16).find(|&s| h.joysticks.present(s)))?;
        let kind = h.joysticks.kind(slot);
        Some((kind, h.joysticks.face_labels(slot).unwrap_or(glyphs::XBOX_LABELS)))
    })
}

/// Identifies the glyph set prompts need now (pad kind, face-button layout and UI style); 0
/// without pads. The font code reloads its button sprites when this changes; it takes the pad
/// kind from bits 16-23.
#[unsafe(no_mangle)]
pub extern "C" fn revc_glyph_set_id() -> c_int {
    match glyph_pad() {
        None => 0,
        Some((kind, labels)) => {
            let layout = labels.iter().fold(0, |acc, l| acc * 16 + (l.0 as c_int & 15));
            1 + (ui_style() as c_int) * (1 << 24) + kind as c_int * 65536 + layout
        }
    }
}

/// Whether the pad actually in use has Nintendo's face layout (B at the bottom, A on the right),
/// whatever the BUTTON PROMPTS setting shows. Menus then confirm with A and go back with B in every
/// UI style (user decision 2026-11-12), so CPad::Update swaps the roles while a menu is open.
fn real_pad_nintendo() -> bool {
    with(|h| {
        let slot = h.active_pad.filter(|&s| h.joysticks.present(s)).or_else(|| (0..16).find(|&s| h.joysticks.present(s)));
        slot.and_then(|s| h.joysticks.face_labels(s)).is_some_and(|l| l[0] == SDL_GAMEPAD_BUTTON_LABEL_B)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_pad_menu_nintendo() -> c_int {
    real_pad_nintendo() as c_int
}

/// The name of a button of the pad in use, as the console styles' help lines print it (research
/// glyph-research/CONSOLE_STYLE_GLYPHS.md 4): `pos` 0-3 the face buttons south, east, west,
/// north (their printed label: "A", "B", ...; PlayStation shapes as "cross", "circle", "square",
/// "triangle" for the caller to draw), 4 start, 5 select (the pad's own word: "START", "MENU",
/// "OPTIONS", "+", ...). `pos` is the menu role's position: on a Nintendo pad the menus confirm
/// with east (A) and go back with south (B), so 0 names east and the back role (1, or 3 in the
/// PS2 style, whose back is triangle) names south. Written NUL-terminated into `buf`; returns the
/// length, 0 without a pad (keyboard) or for an unknown position.
///
/// # Safety
/// `buf` holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_pad_button_name(pos: c_int, buf: *mut c_char, len: c_int) -> c_int {
    let Some((kind, labels)) = glyph_pad() else { return 0 };
    if kind == PadKind::KeyboardMouse || buf.is_null() || len <= 1 {
        return 0;
    }
    let mut pos = pos;
    if real_pad_nintendo() {
        let back = if ui_style() == glyphs::UiStyle::Ps2 { 3 } else { 1 };
        pos = match pos {
            0 => 1,
            p if p == back => 0,
            1 => 3,	// (PS2 style: circle takes north, as CPad::Update swaps them)
            p => p,
        };
    }
    let name: &str = match pos {
        0..=3 => match labels[pos as usize] {
            SDL_GAMEPAD_BUTTON_LABEL_A => "A",
            SDL_GAMEPAD_BUTTON_LABEL_B => "B",
            SDL_GAMEPAD_BUTTON_LABEL_X => "X",
            SDL_GAMEPAD_BUTTON_LABEL_Y => "Y",
            SDL_GAMEPAD_BUTTON_LABEL_CROSS => "cross",
            SDL_GAMEPAD_BUTTON_LABEL_CIRCLE => "circle",
            SDL_GAMEPAD_BUTTON_LABEL_SQUARE => "square",
            SDL_GAMEPAD_BUTTON_LABEL_TRIANGLE => "triangle",
            _ => ["A", "B", "X", "Y"][pos as usize],
        },
        4 => match kind {
            PadKind::XboxSeries | PadKind::SteamDeck => "MENU",
            PadKind::PlayStation4 | PadKind::PlayStation5 => "OPTIONS",
            PadKind::Switch => "+",
            _ => "START",
        },
        5 => match kind {
            PadKind::XboxSeries | PadKind::SteamDeck => "VIEW",
            PadKind::PlayStation3 => "SELECT",
            PadKind::PlayStation4 => "SHARE",
            PadKind::PlayStation5 => "CREATE",
            PadKind::Switch => "-",
            _ => "BACK",
        },
        _ => return 0,
    };
    let n = name.len().min(len as usize - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(name.as_ptr(), buf as *mut u8, n);
        *buf.add(n) = 0;
    }
    n as c_int
}

/// Short name of the current glyph set, for logs and tests ("PlayStation5", "none").
pub fn glyph_set_name() -> String {
    glyph_pad().map_or("none".into(), |(k, _)| format!("{k:?}"))
}

/// Button glyph `index` (CFont BUTTON_* order) of the current glyph set as RGBA8, top row first.
/// Free with revc_texmod_free. 0 if there is no pad or no such glyph in the assets.
///
/// # Safety
/// The out pointers must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_glyph_rgba(index: c_int, width: *mut c_int, height: *mut c_int, rgba: *mut *mut u8) -> c_int {
    let Some((kind, labels)) = glyph_pad() else { return 0 };
    let Some(path) = glyphs::button(index as usize).and_then(|b| glyphs::glyph_path(kind, labels, b, ui_style())) else { return 0 };
    match crate::texmods::decode(&path) {
        Ok((w, h, px)) => {
            unsafe {
                *width = w as c_int;
                *height = h as c_int;
                *rgba = Box::into_raw(px.into_boxed_slice()) as *mut u8;
            }
            1
        }
        Err(e) => {
            eprintln!("glyphs: {}: {e}", path.display());
            0
        }
    }
}

/// Glyph set of the pad in slot `jid` (KeyboardMouse = 0 if none).
#[unsafe(no_mangle)]
pub extern "C" fn hostJoystickKind(jid: c_int) -> c_int {
    with(|h| h.joysticks.kind(jid) as c_int)
}

/// Vibration for `ms` milliseconds (low/high-frequency motor strength 0..65535; 0/0 stops).
/// Returns 0 if the pad has no rumble.
#[unsafe(no_mangle)]
pub extern "C" fn hostGamepadRumble(jid: c_int, low: u16, high: u16, ms: u32) -> c_int {
    with(|h| h.joysticks.rumble(jid, low, high, ms) as c_int)
}

#[unsafe(no_mangle)]
pub extern "C" fn hostJoystickPresent(jid: c_int) -> c_int {
    with(|h| h.joysticks.present(jid) as c_int)
}

#[unsafe(no_mangle)]
pub extern "C" fn hostJoystickIsGamepad(jid: c_int) -> c_int {
    with(|h| h.joysticks.is_gamepad(jid) as c_int)
}

#[unsafe(no_mangle)]
pub extern "C" fn hostGetJoystickName(jid: c_int) -> *const c_char {
    with(|h| h.joysticks.name(jid))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostGetJoystickButtons(jid: c_int, count: *mut c_int) -> *const u8 {
    let (p, n) = with(|h| h.joysticks.buttons(jid));
    if !count.is_null() {
        unsafe { *count = n };
    }
    p
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostGetJoystickAxes(jid: c_int, count: *mut c_int) -> *const f32 {
    let (p, n) = with(|h| h.joysticks.axes(jid));
    if !count.is_null() {
        unsafe { *count = n };
    }
    p
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostGetGamepadState(jid: c_int, state: *mut HostGamepadState) -> c_int {
    match with(|h| h.joysticks.gamepad_state(jid)) {
        Some(s) => {
            unsafe { *state = s };
            1
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hostUpdateGamepadMappings(mappings: *const c_char) -> c_int {
    if mappings.is_null() {
        return 0;
    }
    let bytes = unsafe { CStr::from_ptr(mappings) }.to_bytes();
    let n = unsafe {
        let io = SDL_IOFromConstMem(bytes.as_ptr().cast(), bytes.len());
        SDL_AddGamepadMappingsFromIO(io, true)
    };
    with(|h| h.joysticks.refresh_gamepads());
    (n >= 0) as c_int
}
