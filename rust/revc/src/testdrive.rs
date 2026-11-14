//! Regression-test driver. Enabled by `REVC_TEST`; driven once per frame from
//! `host::hostPollEvents`, using the C hooks in cpp/re3-miami/src/skel/host/testhooks.cpp.
//!
//! Scenarios:
//!   REVC_TEST=boot      reach the front-end menu, check the Controls page has revc's entries;
//!                       with REVC_TEST_SHOT, open that page and capture it (the menu backdrop
//!                       animates in real time, so there is no reference to compare with).
//!   REVC_TEST=audio     like saveload, then plays a scripted scene with injected keys (walk
//!                       forward 3 s, cycle weapon, fire 3 times) for the A/B audio comparison.
//!                       REVC_TEST_WAV=path records the Rust mixer's output for the whole run.
//!   REVC_TEST=saveload  load save slot REVC_TEST_SLOT (1-8, default 1), play until
//!                       REVC_TEST_MS of game time (default 8000) have passed and the screen
//!                       is no longer fading, check the player position, optionally capture a
//!                       screenshot and compare it with a reference. Waiting on game time
//!                       rather than frames keeps the captured moment independent of fps.
//!   REVC_TEST=texmods   like saveload with the texture mods in tests/mods installed: checks
//!                       they were applied (and the broken one rejected) and the screenshot.
//!   REVC_TEST=gamepad   like saveload, then plugs in SDL virtual pads (DualSense, Steam Deck)
//!                       and checks pad detection, glyph-set switching and rumble (padtest.rs).
//!   REVC_TEST=spots     like saveload, then visits SPOTS (no peds/traffic, noon, sunny, fixed
//!                       camera) and compares each frame with REVC_TEST_REF_DIR/<spot>.png,
//!                       writing REVC_TEST_SHOT_DIR/<spot>.png (plan task 3). Logs the frame
//!                       times at each spot (`perf <spot>: ...`, the last 3 s before the
//!                       capture; plan task 2 baselines, run with VSync and FrameLimiter off).
//!   REVC_TEST=savegame  like saveload, then saves to slot REVC_TEST_SAVE_SLOT (default 2) as
//!                       the pause menu does and checks the slot list (name, date) and the file
//!                       (the game's own check). scripts/test.sh then loads that slot.
//!   REVC_TEST=formats   like saveload, then dumps what the C++ loaders hold and compares it
//!                       with revc-formats (formatcheck.rs).
//!   REVC_TEST=pedhit    like saveload, then a car rolls at 20 km/h into five standing peds on
//!                       the airport tarmac; logs how high each ped is thrown (metres above where
//!                       it stood), both in the physics and as drawn; fails if a ped is drawn far
//!                       above its physics position (render interpolation). scripts/test.sh runs it
//!                       at 50 km/h (REVC_TEST_PEDHIT_KMH) and 320 fps (REVC_FIXED_FPS).
//!   REVC_TEST=cutscenes loads save slot 1 (at the hotel; REVC_TEST_CUT_NEW=1: a new game, the
//!                       opening's cutscenes first) and puts the player on the nearest mission
//!                       marker once no cutscene ran for 6 s (REVC_TEST_CUT_MARKERS times, default 1;
//!                       slot 1: the first mission's hotel cutscene). Ends 5 s after the last
//!                       marker's cutscene, or after REVC_TEST_CUT_MS of game time (default 240 s). Every REVC_TEST_CUT_SHOT_MS (default 1000) of a running
//!                       cutscene a frame goes to REVC_TEST_SHOT_DIR (cut_<name>_<ms>.png; outside
//!                       them game_<ms>.png every REVC_TEST_CUT_GAME_MS, default 5 s). Fails on textures a model asked for during a
//!                       cutscene that nobody had (drawn untextured), except the ones in
//!                       tests/golden/missing-textures.txt (missing in the game's own data); the
//!                       ones outside cutscenes are only logged.
//!   REVC_TEST=motion    like saveload, then walks the player forward and turns (injected keys)
//!                       while capturing every frame with its render interpolation (run with
//!                       REVC_FIXED_FPS, e.g. 144). Writes the frames to REVC_TEST_VIDEO_DIR
//!                       (frame_0000.png ...; scripts/test.sh makes motion.mp4) and checks that
//!                       the player's body and a bone move smoothly from frame to frame: jitter =
//!                       mean |second difference| / mean |first difference| of their rendered
//!                       positions, at most REVC_TEST_MAX_JITTER (default 0.5).
//! Options:
//!   REVC_TEST_SHOT=path.png   write the final frame here
//!   REVC_TEST_REF=path.png    compare against this; writes path.png.diff.png on failure
//!   REVC_TEST_MAXDIFF=0.005   allowed fraction of differing pixels (runs differ by <0.02%)
//!   REVC_TEST_TIMEOUT=180     seconds before the run is failed
//! Results are printed as `[revc-test] ...` lines; the exit code is 0 pass, 1 fail, 3 timeout.

use std::cell::RefCell;
use std::sync::atomic::{AtomicI32, Ordering};
use std::path::PathBuf;
use std::time::{Duration, Instant};

unsafe extern "C" {
    fn revc_test_state() -> i32;
    fn revc_test_frame() -> u32;
    fn revc_test_game_ms() -> u32;
    fn revc_test_fading() -> i32;
    fn revc_test_audio_fx(supported: *mut i32, mode: *mut i32, level: *mut f32);
    fn revc_test_quit();
    fn revc_test_load_slot(slot: i32) -> i32;
    fn revc_test_new_game() -> i32;
    fn revc_test_cutscene(name: *mut std::ffi::c_char, n: i32) -> i32;
    fn revc_test_goto_marker(x: *mut f32, y: *mut f32, z: *mut f32) -> i32;
    fn revc_test_player_pos(x: *mut f32, y: *mut f32, z: *mut f32);
    fn revc_test_dump_formats(path: *const std::ffi::c_char) -> i32;
    fn revc_test_load_all_txds() -> i32;
    fn revc_test_read_txd(data: *const u8, len: usize) -> i32;
    fn revc_dff_plugin_counts(rust: *mut i32, librw: *mut i32);
    fn revc_test_hash_dff(data: *const u8, len: usize) -> u64;
    fn revc_test_clear_help();
    fn revc_test_show_controls_menu();
    fn revc_test_check_prompt_menu() -> i32;
    fn revc_test_check_pad_layouts() -> i32;
    fn revc_test_save_slot(slot: i32, state: *mut i32, name: *mut u16, date: *mut u16) -> i32;
    fn revc_test_motion_sample(out: *mut f32);
    fn revc_test_pedhit_setup(kmh: f32) -> i32;
    fn revc_test_scanner_setup() -> i32;
    fn revc_test_pedhit_sample() -> i32;
    fn revc_test_pedhit_result(max_rise: *mut f32, car_dist: *mut f32);
    fn revc_test_pedhit_drawn(body_rise: *mut f32, head_rise: *mut f32, hit: *mut i32);
    fn revc_test_menu_state(page: i32, option: i32, context: i32, layout: i32, legend: i32) -> i32;
    fn revc_test_menu_page_id(name: *const std::ffi::c_char) -> i32;
    fn revc_test_menu_input(down: i32, change: i32);
    fn revc_test_menu_key(down: i32, up: i32, select: i32, back: i32, change: i32);
    fn revc_test_map_zoom(dir: i32);
    fn revc_test_scene(x: f32, y: f32, heading: f32, hour: i32, minute: i32, weather: i32) -> f32;
    fn revc_test_hud_text(kind: i32);
    fn revc_test_fire(kind: i32);
    fn revc_test_map_skipped() -> i32;
    fn revc_test_pool_report();
    fn revc_test_swap_building(x: f32, y: f32, z: f32, from: *const std::ffi::c_char, to: *const std::ffi::c_char) -> i32;
    fn revc_test_cut_load(cut: *const std::ffi::c_char, out: *mut std::ffi::c_char, len: i32) -> i32;
    fn revc_test_prompt_text(key: *const std::ffi::c_char, out: *mut std::ffi::c_char, len: i32) -> i32;
    fn revc_test_particle_frames(kind: i32) -> i32;
    #[allow(clippy::too_many_arguments)]
    fn revc_test_pad(on: i32, buttons: u32, lx: f32, ly: f32, rx: f32, ry: f32, l2: f32, r2: f32);
    fn revc_test_menu_where(open: *mut i32, page: *mut i32, row: *mut i32) -> i32;
    fn revc_test_world_start(kind: i32, x: f32, y: f32, heading: f32);
    fn revc_test_sweep_state(idx: i32, page: *mut i32, row: *mut i32) -> i32;
    fn revc_test_menu_walk(out: *mut std::ffi::c_char, cap: i32) -> i32;
    fn revc_test_menu_set_focus(page: i32, list: i32);
    fn revc_test_menu_status(open: *mut i32, page: *mut i32, row: *mut i32, skinned: *mut i32, fading: *mut i32);
    fn revc_test_telemetry(out: *mut f32, anim: *mut std::ffi::c_char, anim_len: i32, anim_time: *mut f32);
    fn revc_test_goto_spot(x: f32, y: f32, z: f32, dx: f32, dy: f32, dz: f32, hour: i32, minute: i32) -> f32;
}

const STATE_FRONTEND: i32 = 1;
const STATE_PLAYING: i32 = 2;
/// Part of the gamepad screenshots compared with the reference (fractions of width and height:
/// x0, y0, x1, y1): the help box with every button glyph, top left.
const GLYPH_BOX: [f32; 4] = [0.0, 0.0, 0.3, 0.2];
/// Parts of the in-game screenshots after loading a save that aren't compared (fractions, as
/// GLYPH_BOX): HUD elements animated in real time, which land on another phase depending on the
/// frame pacing (saveload went from 0.38 % to 1.66 % differing over six runs on these alone):
/// the wanted stars, which fade in after a load, and, bottom right, the save cassette icon and
/// the zone name as it fades. The world, the player and the rest of the HUD are still compared.
const LOADED_HUD_SKIP: [[f32; 4]; 2] = [[0.70, 0.16, 0.90, 0.25], [0.64, 0.72, 1.0, 0.98]];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Scenario {
    Boot,
    SaveLoad,
    Audio,
    Formats,
    TexMods,
    Gamepad,
    Spots,
    SaveGame,
    Motion,
    PedHit,
    Scanner,
    Scenes,
    Menus,
    PadScript,
    MenuSweep,
    Cutscenes,
    Screenshot,
    MenuWalk,
}

#[derive(Debug)]
enum Phase {
    WaitFrontend { frames: u32 },
    WaitPlaying,
    Playing { start_ms: u32, start_frame: u32 },
    Capturing,
    /// Scripted input (audio scenario); `t0` is the game time it started.
    Script { t0: u32, step: usize },
    /// Virtual-controller checks (gamepad scenario, padtest.rs).
    Pads,
    /// Boot: waiting on the Controls menu page before capturing it.
    MenuShot { frames: u32 },
    /// Fixed-spot screenshots. Spot `idx` is visited at game time `base + idx * SPOT_PERIOD_MS`
    /// and captured SPOT_WAIT_MS later, so wind, waves and the clock are at the same phase in every
    /// run whatever the frame rate (the simulation advances in exact 30 Hz steps).
    Spot { idx: usize, base: u32, stage: SpotStage },
    /// Menu comparison shots (MENU_SHOTS): shot `idx`, frames since its state was set (0 = not yet).
    Menus { idx: usize, frames: u32 },
    /// Console reference scenes (SCENES), same timing scheme as Spot.
    Scene { idx: usize, base: u32, stage: SpotStage },
    /// Motion capture: scripted walk since game time `t0`, `step` into MOTION_SCRIPT.
    Motion { t0: u32, step: usize },
    PedHit,
    /// Scanner check: recording since game time `t0`.
    Scanner { t0: u32 },
    /// Pad script (REVC_TEST_PAD_SCRIPT): frames since the start state was set.
    PadScript { frame: u32 },
    Screenshot { frame: u32 },
    MenuWalk,
    /// Button sweep: state `idx` (revc_test_sweep_state), pad button `button`, frames into it.
    MenuSweep { idx: i32, button: u32, frame: u32, flagged: u32 },
    /// Cutscenes: game time `t0` the game started, `last` the last frame saved (game time).
    Cutscenes { t0: u32, last: u32 },
    Done,
}

/// Fixed screenshot spots: (name, x, y, camera offset dx, dy, dz). The player stands on the ground
/// at (x, y); the camera looks at the player from the offset. Chosen to cover the map's districts
/// and typical content (streets, water, foliage, big buildings, interiors' exteriors).
const SPOTS: &[(&str, f32, f32, f32, f32, f32)] = &[
    ("ocean_drive", 285.0, -1200.0, 24.0, 18.0, 14.0),
    ("washington_beach", 400.0, -470.0, 24.0, -18.0, 16.0),
    ("vice_point", 470.0, 1150.0, -30.0, 12.0, 26.0),
    ("downtown", -720.0, 1150.0, 24.0, -20.0, 24.0),
    ("little_haiti", -950.0, 100.0, 22.0, 22.0, 14.0),
    ("little_havana", -1050.0, -500.0, -22.0, 22.0, 14.0),
    ("starfish_island", -380.0, -560.0, 26.0, 26.0, 20.0),
    ("airport", -1450.0, -900.0, 0.0, -40.0, 26.0),
];
/// Console reference scenes (plan task 13; the PS2/Xbox stills are in Claude outputs/world-captures,
/// local only): (name, x, y, heading in degrees with 0 = north, hour, minute, weather 0 sunny
/// 1 cloudy 2 rain 3 fog 4 extra sunny). The player stands with the follow camera behind him and the HUD on, as in
/// the console captures. REVC_TEST_SCENES=S1,S4 picks some; REVC_TEST_SCENE=x,y,heading,hour,minute,
/// weather[;...] adds scenes named custom0, custom1, ... (for finding a spot).
const SCENES: &[(&str, f32, f32, f32, i32, i32, i32)] = &[
    ("S1_noon", MOONLITE.0, MOONLITE.1, 0.0, 12, 9, 0),
    ("S2_night", MOONLITE.0, MOONLITE.1, 0.0, 22, 0, 0),
    ("S3_rain", MOONLITE.0, MOONLITE.1, 0.0, 12, 0, 2),
    ("S4_fog", MOONLITE.0, MOONLITE.1, 0.0, 12, 0, 3),
    ("S5_east", MOONLITE.0, MOONLITE.1, 270.0, 7, 0, 0),
    ("S6_lighthouse", LIGHTHOUSE.0, LIGHTHOUSE.1, 35.0, 19, 0, 0),
    ("S6_lighthouse_noon", LIGHTHOUSE.0, LIGHTHOUSE.1, 35.0, 11, 38, 0),
    ("S10_dawn", MOONLITE.0, MOONLITE.1, 0.0, 5, 33, 0),
    // the Xbox stills' noon: extra sunny (weather 4: no clouds, its own sky; research
    // XBOX_COLOUR_RE.md 6.1, matched to the capture's sky)
    ("S1x_noon", MOONLITE.0, MOONLITE.1, 0.0, 12, 46, 4),
];
/// Middle of Ocean Drive in front of the Moonlite Hotel, and next to the lighthouse.
const MOONLITE: (f32, f32) = (257.0, -1250.0);
const LIGHTHOUSE: (f32, f32) = (470.0, -1695.0);
const SCENE_WAIT_MS: u32 = 2500;

/// Menu comparison shots (plan task 13, research request menu-compare/REQUEST.md, local): (id,
/// page name, selected row, controller context, pad layout, map legend, ms to wait). -1 leaves
/// a setting as it is. REVC_TEST_MENU_SET=ps2|xbox picks the ids' set (the UI style comes from the
/// config); shots go to REVC_TEST_SHOT_DIR/<set>_<id>.png. States revc doesn't have are left out
/// (docs/CONSOLE_STYLES.md).
type MenuShot = (&'static str, &'static str, i32, i32, i32, i32, u32);
const MENU_SHOTS_PS2: &[MenuShot] = &[
    ("map_legend_on", "map", 0, -1, -1, 1, 1000),
    ("map_legend_off", "map", 0, -1, -1, 0, 1000),
    ("brief", "brief", 0, -1, -1, -1, 1000),
    ("load_slots", "load", 1, -1, -1, -1, 1000),
    ("stats", "stats", 0, -1, -1, -1, 1000),
    ("stats_2", "stats", 0, -1, -1, -1, 5000),
    ("controls", "console_controls", 0, -1, -1, -1, 1000),
    ("controls_on_foot", "console_controls", 0, 0, -1, -1, 1000),
    ("controls_in_car", "console_controls", 0, 1, -1, -1, 1000),
    ("controls_flying", "console_controls", 0, 2, -1, -1, 1000),
    ("controls_setup1_on_foot", "console_controls", 0, 0, 0, -1, 1000),
    ("controls_setup1_in_car", "console_controls", 0, 1, 0, -1, 1000),
    ("controls_setup2_on_foot", "console_controls", 0, 0, 1, -1, 1000),
    ("controls_setup2_in_car", "console_controls", 0, 1, 1, -1, 1000),
    ("controls_setup3_on_foot", "console_controls", 0, 0, 2, -1, 1000),
    ("controls_setup3_in_car", "console_controls", 0, 1, 2, -1, 1000),
    ("controls_setup4_on_foot", "console_controls", 0, 0, 3, -1, 1000),
    ("controls_setup4_in_car", "console_controls", 0, 1, 3, -1, 1000),
    ("audio", "audio", 0, -1, -1, -1, 1000),
    ("audio_radio", "audio", 2, -1, -1, -1, 1000),
    ("display", "console_display", 0, -1, -1, -1, 1000),
    ("display_2", "console_display", 1, -1, -1, -1, 1000),
    ("display_screen_position", "console_display", 6, -1, -1, -1, 1000),
    ("options", "console_options", 1, -1, -1, -1, 1000),
    ("map_zoomed", "map", 0, -1, 1, 1, 1000),
    ("map_zoomed_max", "map", 0, -1, 2, 1, 1000),
    ("save_overwrite", "save_overwrite", 1, -1, -1, -1, 1000),
    ("save_overwrite_yes", "save_overwrite", 2, -1, -1, -1, 1000),
    ("save_success", "save_success", 1, -1, -1, -1, 1000),
    ("options_2", "console_options", 14, -1, -1, -1, 1000),
    ("options_3", "console_options", 27, -1, -1, -1, 1000),
    ("save_slots", "save", 1, -1, -1, -1, 1000),
    ("load_confirm", "load_confirm", 1, -1, -1, -1, 1000),
    ("new_game_confirm", "new_game_confirm", 1, -1, -1, -1, 1000),
    ("quit_confirm", "quit_confirm", 1, -1, -1, -1, 1000),
];
const MENU_SHOTS_XBOX: &[MenuShot] = &[
    ("map_legend_on", "map", 0, -1, -1, 1, 1000),
    ("map_legend_off", "map", 0, -1, -1, 0, 1000),
    ("brief", "brief", 0, 1, -1, -1, 1000),	// (the console reference: the tutorial sub-tab)
    ("stats", "stats", 0, -1, -1, -1, 1000),
    ("stats_scrolled", "stats", 0, -1, -1, -1, 5000),
    ("unlocked_items", "stats", 0, 1, -1, -1, 1000),
    ("brief_briefs", "brief", 0, 0, -1, -1, 1000),
    ("controls", "console_controls", 0, -1, -1, -1, 1000),
    ("controls_select", "console_controls", 0, -1, -1, -1, 1000),
    ("controls_walking", "pad", 0, 0, 0, -1, 1000),
    ("controls_driving", "pad", 0, 1, 0, -1, 1000),
    ("controls_flying", "pad", 0, 2, 0, -1, 1000),
    ("controls_classic_walking", "pad", 0, 0, 1, -1, 1000),
    ("controls_classic_driving", "pad", 0, 1, 1, -1, 1000),
    ("controls_classic_flying", "pad", 0, 2, 1, -1, 1000),
    ("audio_music", "audio", 0, -1, -1, -1, 1000),
    ("audio_sfx", "audio", 1, -1, -1, -1, 1000),
    ("audio_radio_vcpr", "audio", 2, -1, -1, -1, 1000),
    ("display", "console_display", 0, -1, -1, -1, 1000),
    ("display_advanced", "console_options", 1, 0, -1, -1, 1000),
    ("audio_advanced", "console_options", 1, 1, -1, -1, 1000),
    ("controls_advanced", "console_options", 1, 2, -1, -1, 1000),
    ("load_top", "new_game", 0, -1, -1, -1, 1000),
    ("load_slots", "load", 1, -1, -1, -1, 1000),
    ("load_confirm_new_game", "new_game_confirm", 1, -1, -1, -1, 1000),
    ("load_confirm_quit", "quit_confirm", 1, -1, -1, -1, 1000),
    ("save_slots", "save", 1, -1, -1, -1, 1000),
    ("save_overwrite", "save_overwrite", 1, -1, -1, -1, 1000),
    ("save_overwrite_yes", "save_overwrite", 2, -1, -1, -1, 1000),
];

/// Stats with an endgame save (REVC_TEST_SAVE, set ps2_endgame / xbox_endgame): the top, then the
/// list scrolled further and further down.
const MENU_SHOTS_PS2_END: &[MenuShot] = &[
    ("stats_endgame", "stats", 0, -1, -1, -1, 1000),
    ("stats_endgame_scrolled_1", "stats", 0, -1, -1, -1, 8000),
    ("stats_endgame_scrolled_2", "stats", 0, -1, -1, -1, 16000),
    ("stats_endgame_scrolled_3", "stats", 0, -1, -1, -1, 24000),
    ("stats_endgame_scrolled_4", "stats", 0, -1, -1, -1, 32000),
];
const MENU_SHOTS_XBOX_END: &[MenuShot] = &[
    ("stats_endgame", "stats", 0, -1, -1, -1, 1000),
    ("stats_endgame_scrolled_1", "stats", 0, -1, -1, -1, 8000),
    ("stats_endgame_scrolled_2", "stats", 0, -1, -1, -1, 16000),
    ("stats_endgame_scrolled_3", "stats", 0, -1, -1, -1, 24000),
    ("stats_endgame_scrolled_4", "stats", 0, -1, -1, -1, 32000),
    ("unlocked_items_endgame", "stats", 0, 1, -1, -1, 1000),
];

/// A still clip for the selection pulse (set xbox_pulse, with --clips): Display, brightness
/// selected, held 5 s with no input.
const MENU_SHOTS_XBOX_PULSE: &[MenuShot] = &[("pulse_still", "console_display", 0, -1, -1, -1, 1000)];
/// A clip of the PS2 map zoom (set ps2_mapzoom, with --clips): the map page, R1 held 1.5 s, let go
/// 1 s, R2 held 1.5 s, let go 1 s.
const MENU_SHOTS_PS2_MAPZOOM: &[MenuShot] = &[("mapzoom_still", "map", 0, -1, -1, 1, 1000)];

/// Motion tests (REVC_TEST_MOTION=<id>, scripts/motion-tests.sh; research menu-compare/MOTION_TESTS.md,
/// local): one menu animation per run. (id, page, row, context, layout, input, frames.) The start
/// state is set like a menu shot (context/layout as there, -1 leaves them), held 0.5 s
/// (MOTION_INPUT_FRAME), then one input; every frame is written to REVC_TEST_VIDEO_DIR (run at
/// REVC_FIXED_FPS=60) until `frames`. Inputs: down, up, left, right, select, back, none,
/// page:<name> (switch tab as the menu does), context:<n> (Xbox sub-tab / pad context),
/// layout:<n> (PS2 setup / map zoom level), zoom_in:<held frames>, zoom_out:<held frames>.
/// The UI style comes from the config (the id's ps2_/xbox_ prefix in the script).
type Motion = (&'static str, &'static str, i32, i32, i32, &'static str, u32);
const MOTION_INPUT_FRAME: u32 = 30;
const MOTIONS: &[Motion] = &[
    // PS2
    ("ps2_page_change_right", "audio", 0, -1, -1, "page:console_display", 90),
    ("ps2_page_change_left", "console_display", 0, -1, -1, "page:audio", 90),
    ("ps2_page_change_to_map", "brief", 0, -1, -1, "page:map", 90),
    ("ps2_page_change_from_map", "map", 0, -1, -1, "page:brief", 90),
    ("ps2_highlight_down", "audio", 0, -1, -1, "down", 75),
    ("ps2_highlight_up", "audio", 1, -1, -1, "up", 75),
    ("ps2_highlight_wrap", "audio", 0, -1, -1, "up", 75),
    ("ps2_radio_right", "audio", 2, -1, -1, "right", 90),
    ("ps2_radio_left", "audio", 2, -1, -1, "left", 90),
    ("ps2_slider_step", "audio", 0, -1, -1, "right", 75),
    ("ps2_stats_scroll", "stats", 0, -1, -1, "none", 630),
    ("ps2_map_zoom_in", "map", 0, -1, 0, "zoom_in:90", 210),
    ("ps2_map_zoom_out", "map", 0, -1, 2, "zoom_out:90", 210),
    ("ps2_confirm_open", "load", 1, -1, -1, "select", 90),
    ("ps2_setup_switch", "console_controls", 0, 0, 0, "layout:1", 75),
    // Xbox
    ("xbox_bar_pulse", "console_display", 0, -1, -1, "none", 150),
    ("xbox_highlight_down", "console_display", 0, -1, -1, "down", 75),
    ("xbox_highlight_up", "console_display", 1, -1, -1, "up", 75),
    ("xbox_page_change_right", "audio", 0, -1, -1, "page:console_display", 90),
    ("xbox_page_change_left", "console_display", 0, -1, -1, "page:audio", 90),
    ("xbox_subtab_change", "stats", 0, 0, -1, "context:1", 90),
    ("xbox_radio_right", "audio", 2, -1, -1, "right", 90),
    ("xbox_radio_left", "audio", 2, -1, -1, "left", 90),
    ("xbox_list_scroll", "brief", 0, 1, -1, "none", 630),
    ("xbox_confirm_open", "load", 1, -1, -1, "select", 90),
];

/// REVC_TEST_TELEMETRY=<csv>: one line per frame of a pad script (frame, ms, position, heading,
/// speed m/s, vehicle model, the four suspension ratios, gas, steer, the player's main animation and
/// its time), for comparing car and ped motion with the console (read there from emulator memory).
fn telemetry_line(frame: u32) {
    use std::io::Write;
    static F: std::sync::Mutex<Option<std::fs::File>> = std::sync::Mutex::new(None);
    let Some(path) = std::env::var_os("REVC_TEST_TELEMETRY") else { return };
    let mut g = F.lock().unwrap();
    if g.is_none() {
        let Ok(mut f) = std::fs::File::create(&path) else { return };
        let _ = writeln!(f, "frame,ms,x,y,z,heading,speed,vehicle,susp0,susp1,susp2,susp3,gas,steer,anim,anim_time");
        *g = Some(f);
    }
    let mut v = [0f32; 12];
    let mut name = [0 as std::ffi::c_char; 32];
    let mut at = 0f32;
    unsafe { revc_test_telemetry(v.as_mut_ptr(), name.as_mut_ptr(), name.len() as i32, &mut at) };
    let anim = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }.to_string_lossy();
    let vals: Vec<String> = v.iter().map(|x| format!("{x:.4}")).collect();
    if let Some(f) = g.as_mut() {
        let _ = writeln!(f, "{frame},{:.1},{},{anim},{at:.4}", frame as f32 * 1000.0 / 60.0, vals.join(","));
    }
}

/// The pad script of REVC_TEST_PAD_SCRIPT (a file), or of REVC_TEST_MOTION's route
/// (MOTION_ROUTES, also written to REVC_TEST_PAD_OUT/<id>.pad.json), parsed once.
fn pad_script() -> Result<&'static crate::padscript::PadScript, String> {
    static S: std::sync::OnceLock<Result<crate::padscript::PadScript, String>> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        if let Some(r) = motion_route() {
            let p = route_script(r)?;
            if let Some(dir) = std::env::var_os("REVC_TEST_PAD_OUT") {
                let path = PathBuf::from(dir).join(format!("{}.pad.json", p.id));
                std::fs::write(&path, p.to_json()).map_err(|e| format!("{}: {e}", path.display()))?;
            }
            return Ok(p);
        }
        let path = std::env::var("REVC_TEST_PAD_SCRIPT").map_err(|_| "REVC_TEST_PAD_SCRIPT not set".to_string())?;
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
        crate::padscript::PadScript::parse(&text).map_err(|e| format!("{path}: {e}"))
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// Motion tests driven by the console's own button route (PS2 style; navigation measured on the
/// PS2, MenuSkinPS2.cpp): (id, presses from menu_base = in game with the menu closed, the
/// motion's input, frames held after it). Presses are 4-frame taps 30 frames apart (60 after
/// START, for the menu to open); the input comes 60 frames after the last press. Inputs: a tap
/// ("cross"), "+r1:90" (held 90 frames) or "none". Tabs: map brief load stats / controls audio
/// display options, the menu opens on Map with the tab bar focused; cross enters a list at row 0.
/// The same route is written as <id>.pad.json (REVC_TEST_PAD_OUT) for the console captures.
type MotionRoute = (&'static str, &'static str, &'static str, u32);
const MOTION_ROUTES: &[MotionRoute] = &[
    ("ps2_menu_open", "", "start", 60),
    ("ps2_menu_close", "start", "start", 60),
    ("ps2_page_change_right", "start down right", "right", 60),
    ("ps2_page_change_left", "start down right right", "left", 60),
    ("ps2_page_change_right_v140", "start", "right", 60),	// (the v1.40 retakes: Map -> Brief and back)
    ("ps2_page_change_left_v140", "start right", "left", 60),
    ("ps2_page_change_to_map", "start right", "left", 60),
    ("ps2_page_change_from_map", "start", "right", 60),
    ("ps2_tab_brief_to_load", "start right", "right", 60),
    ("ps2_tab_brief_to_audio", "start right", "down", 60),
    ("ps2_tab_audio_to_brief", "start right down", "up", 60),
    ("ps2_confirm_open", "start right right cross down", "cross", 60),
    ("ps2_confirm_close", "start right right cross down cross", "triangle", 60),
    ("ps2_focus_tabs_to_list_display", "start down right right", "cross", 60),
    ("ps2_focus_tabs_to_list_audio", "start down right", "cross", 60),
    ("ps2_focus_tabs_to_list_controls", "start down", "cross", 60),
    ("ps2_focus_list_to_tabs_map", "start cross", "triangle", 60),
    ("ps2_focus_list_to_tabs_audio", "start down right cross down", "triangle", 60),	// (from radio station, as the clip)
    ("ps2_highlight_down", "start down right right cross", "down", 60),	// Display: brightness -> trails
    ("ps2_highlight_up", "start down right right cross down", "up", 60),
    ("ps2_highlight_wrap", "start down right right cross", "up", 60),
    ("ps2_highlight_wrap_down", "start down right right cross up", "down", 60),
    ("ps2_glide_load", "start right right cross", "cross", 60),	// load game -> slot 1
    ("ps2_glide_audio", "start down right cross", "down", 60),	// music -> radio station
    ("ps2_glide_audio_sideways", "start down right cross", "right", 60),	// music -> SFX (outside edit mode)
    ("ps2_radio_right", "start down right cross down cross", "right", 60),	// edit mode on radio station,
    ("ps2_radio_left", "start down right cross down cross", "left", 60),
    ("ps2_slider_step", "start down right cross cross", "right", 60),	// edit mode on music,
    ("ps2_legend_off", "start cross", "cross", 60),
    ("ps2_legend_on", "start cross cross", "cross", 60),
    ("ps2_map_blip_blink", "start", "none", 240),
    ("ps2_map_zoom_in", "start cross", "+r1:90", 150),
    ("ps2_map_zoom_out", "start cross +r1:90", "+r2:90", 150),	// (45 steps in, 45 out: as the console clip)
    ("ps2_stats_scroll", "start right right right", "none", 600),
    ("ps2_setup_switch", "start down cross cross", "right", 60),	// configuration: cross enters its edit mode
    ("ps2_display_screen_pos", "start down right right cross down down down down down down cross", "right", 60),
    // Xbox (pages: load map brief stats controls audio display; START opens Load the first time
    // with the icon bar focused; A goes in, B out; LT/RT switch pages inside)
    ("xbox_menu_open", "", "start", 60),
    ("xbox_menu_close", "start", "start", 60),
    ("xbox_back_to_game", "start", "b", 600),
    ("xbox_page_load_to_map", "start", "right", 60),
    ("xbox_page_map_to_brief", "start right", "right", 60),
    ("xbox_page_brief_to_stats", "start right right", "right", 60),
    ("xbox_page_stats_to_brief", "start right right right", "left", 60),
    ("xbox_page_load_to_display", "start", "left", 60),	// (wraps)
    ("xbox_page_display_to_audio", "start left", "left", 60),
    ("xbox_page_audio_to_controls", "start left left", "left", 60),
    ("xbox_page_controls_to_stats", "start right right right right", "left", 60),
    ("xbox_page_audio_to_display", "start left left", "right", 60),
    ("xbox_highlight_down_load", "start a", "down", 60),	// Load: new game -> load game
    ("xbox_confirm_open", "start a", "a", 60),	// Load: A on "new game" -> the confirm dialog
    ("xbox_page_stats_to_controls", "start right right right", "right", 60),
    ("xbox_page_controls_to_audio", "start right right right right", "right", 60),
    ("xbox_page_change_right", "start left left", "right", 60),	// Audio -> Display
    ("xbox_page_change_left", "start left", "left", 60),	// Display -> Audio
    ("xbox_subtab_change", "start right right a", "right", 60),	// Brief: the sub-tabs (the _v2 reference is Brief, set 2)
    ("xbox_focus_icons_to_list", "start left left", "a", 60),	// Audio
    ("xbox_focus_list_to_icons", "start left left a", "b", 60),
    ("xbox_highlight_down", "start left left a", "down", 60),	// Audio: music -> SFX
    ("xbox_highlight_up", "start left left a down", "up", 60),
    ("xbox_row_display", "start left a", "down", 60),
    ("xbox_radio_right", "start left left a down down", "right", 60),
    ("xbox_radio_left", "start left left a down down", "left", 60),
    ("xbox_bar_pulse", "start left a", "none", 150),
    ("xbox_logo_shimmer", "start", "none", 1200),
    ("xbox_list_scroll", "start right right a right", "none", 600),	// Brief: tutorial briefings
    ("xbox_stats_scroll", "start right right right", "none", 600),
    // Controls (three left of Load): select controls, display controls, invert, vibration, credits
    ("xbox_dropdown_open", "start left left left a", "a", 60),	// select controls: the drop-down
    ("xbox_dropdown_close", "start left left left a a", "b", 60),
    ("xbox_subpage_open", "start left left left a down", "a", 60),	// display controls
    ("xbox_subpage_close", "start left left left a down a", "b", 60),
    ("xbox_subtab_walking_driving", "start left left left a down a", "right", 60),
    ("xbox_row_controls_slow", "start left left left a down down", "down", 60),	// invert -> vibration
    ("xbox_option_toggle", "start left a down down", "right", 60),	// Display: subtitles on -> off
];

/// The pad script for a routed motion (MOTION_ROUTES).
fn route_script(r: &MotionRoute) -> Result<crate::padscript::PadScript, String> {
    let (id, route, input, hold) = *r;
    let full = std::env::var("REVC_TEST_MOTION").unwrap_or_else(|_| id.to_string());
    let mut p = crate::padscript::PadScript::new(&full, id.split('_').next().unwrap_or(""), "menu_base");
    let mut f = 0u32;
    let mut first = true;
    let press = |p: &mut crate::padscript::PadScript, f: u32, op: &str| -> Result<u32, String> {
        // "+btn:N" holds N frames, otherwise a 4-frame tap; returns the frames it takes
        if let Some((b, n)) = op.strip_prefix('+').and_then(|o| o.split_once(':')) {
            let n: u32 = n.parse().map_err(|_| format!("bad hold {op}"))?;
            p.add_step(&format!("{f} +{b}"))?;
            p.add_step(&format!("{} -{b}", f + n))?;
            Ok(n)
        } else {
            p.add_step(&format!("{f} {op}*4"))?;
            Ok(4)
        }
    };
    for op in route.split_whitespace() {
        if !first {
            f += 30;
        }
        first = false;
        let took = press(&mut p, f, op)?;
        f += took.saturating_sub(4);
        if op == "start" {
            f += 30;
        }
    }
    let input_at = if route.is_empty() { 30 } else { f + 60 };
    p.add_step(&format!("{input_at} mark:input"))?;
    if input != "none" {
        press(&mut p, input_at, input)?;
    }
    p.add_step(&format!("{} mark:end", input_at + hold))?;
    Ok(p)
}

/// REVC_TEST_MOTION's route, if it has one.
fn motion_route() -> Option<&'static MotionRoute> {
    let id = std::env::var("REVC_TEST_MOTION").ok()?;
    // (the research catalogue's newer references carry a version suffix: same motion, same route)
    // ("_2": a second sample of the same motion)
    // (a retake that recorded other pages has its own route under its full id)
    if let Some(r) = MOTION_ROUTES.iter().find(|m| m.0 == id) {
        return Some(r);
    }
    let base = id.strip_suffix("_v140").or_else(|| id.strip_suffix("_v2")).or_else(|| id.strip_suffix("_2")).unwrap_or(&id);
    MOTION_ROUTES.iter().find(|m| m.0 == base)
}

fn motion() -> Option<&'static Motion> {
    let id = std::env::var("REVC_TEST_MOTION").ok()?;
    MOTIONS.iter().find(|m| m.0 == id)
}

/// One motion-test input (see MOTIONS), applied on `frame` frames after the input frame.
fn motion_input(input: &str, page: i32, frame: u32) {
    let (kind, arg) = input.split_once(':').unwrap_or((input, ""));
    let n: i32 = arg.parse().unwrap_or(0);
    match kind {
        "zoom_in" | "zoom_out" => {
            let dir = if (frame as i32) < n { if kind == "zoom_in" { 1 } else { -1 } } else { 0 };
            unsafe { revc_test_map_zoom(dir) };
        }
        _ if frame != 0 => {}
        "down" => unsafe { revc_test_menu_key(1, 0, 0, 0, 0) },
        "up" => unsafe { revc_test_menu_key(0, 1, 0, 0, 0) },
        "left" => unsafe { revc_test_menu_key(0, 0, 0, 0, -1) },
        "right" => unsafe { revc_test_menu_key(0, 0, 0, 0, 1) },
        "select" => unsafe { revc_test_menu_key(0, 0, 1, 0, 0) },
        "back" => unsafe { revc_test_menu_key(0, 0, 0, 1, 0) },
        "page" => {
            let name = std::ffi::CString::new(arg).unwrap();
            let p = unsafe { revc_test_menu_page_id(name.as_ptr()) };
            unsafe { revc_test_menu_state(p, -1, -1, -1, -1) };
        }
        "context" => unsafe { revc_test_menu_state(page, -1, n, -1, -1); },
        "layout" => unsafe { revc_test_menu_state(page, -1, -1, n, -1); },
        _ => {}
    }
}

static MENU_SHOT_START: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
static MENU_VIDEO_FRAME: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn menu_shots() -> (&'static str, &'static [MenuShot]) {
    match std::env::var("REVC_TEST_MENU_SET").as_deref() {
        Ok("xbox") => ("xbox", MENU_SHOTS_XBOX),
        Ok("ps2_endgame") => ("ps2", MENU_SHOTS_PS2_END),
        Ok("xbox_endgame") => ("xbox", MENU_SHOTS_XBOX_END),
        Ok("xbox_pulse") => ("xbox", MENU_SHOTS_XBOX_PULSE),
        Ok("ps2_mapzoom") => ("ps2", MENU_SHOTS_PS2_MAPZOOM),
        _ => ("ps2", MENU_SHOTS_PS2),
    }
}
const SCENE_PERIOD_MS: u32 = 5000;

fn scene_list() -> Vec<(String, f32, f32, f32, i32, i32, i32)> {
    let pick = std::env::var("REVC_TEST_SCENES").ok();
    let mut v: Vec<_> = SCENES
        .iter()
        .filter(|s| pick.as_deref().is_none_or(|p| p.split(',').any(|n| s.0 == n || s.0.split('_').next() == Some(n))))
        .map(|s| (s.0.to_string(), s.1, s.2, s.3, s.4, s.5, s.6))
        .collect();
    if let Ok(c) = std::env::var("REVC_TEST_SCENE") {
        for (i, one) in c.split(';').enumerate() {
            let f: Vec<f32> = one.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if f.len() == 6 {
                v.push((format!("custom{i}"), f[0], f[1], f[2], f[3] as i32, f[4] as i32, f[5] as i32));
            }
        }
    }
    v
}

const SPOT_WAIT_MS: u32 = 4000;
const SPOT_PERIOD_MS: u32 = 6000;
/// Game time before each spot's capture over which frame times are measured.
const PERF_WINDOW_MS: u32 = 3000;

#[derive(Debug, Clone, Copy, PartialEq)]
enum SpotStage {
    Travel,
    Settle,
    Capturing,
}

/// (game ms after the script starts, RsKeyCodes key, down). Bindings from tests/config/*.ini:
/// GO_FORWARD = UP, PED_CYCLE_WEAPON_RIGHT = PADENTER, PED_FIREWEAPON = LCTRL.
const AUDIO_SCRIPT: &[(u32, i32, bool)] = &[
    (0, 1019, true),     // rsUP: walk forward (footsteps, reverb-flagged indoors)
    (3000, 1019, false),
    (3500, 1039, true),  // rsPADENTER: cycle weapon
    (3600, 1039, false),
    (4000, 1049, true),  // rsLCTRL: fire
    (4300, 1049, false),
    (5000, 1049, true),
    (5300, 1049, false),
    (6000, 1049, true),
    (6300, 1049, false),
    (8000, 0, false),    // end (let tails ring out)
];

/// Motion scenario: (game ms since the start, key, down). Walk forward, then turn left while
/// walking; frames are captured until MOTION_FRAMES have been taken.
const MOTION_SCRIPT: &[(u32, i32, bool)] = &[
    (0, 1019, true),     // rsUP: walk forward
    (1500, 1021, true),  // rsLEFT: turn while walking
    (2300, 1021, false),
];
const MOTION_FRAMES: usize = 360;
/// Frames before measuring (the walk has to get going).
const MOTION_SKIP: usize = 30;

struct Driver {
    scenario: Scenario,
    slot: i32,
    wait_ms: u32,
    shot: Option<PathBuf>,
    reference: Option<PathBuf>,
    max_diff: f64,
    phase: Phase,
    started: Instant,
    failures: Vec<String>,
    finished: bool,
    pads: crate::padtest::PadTest,
    pad_capture_requested: bool,
    /// Real time of the previous tick and the frame times (ms) collected at the current spot.
    last_tick: Option<Instant>,
    frame_ms: Vec<f32>,
    /// Per spot: (name, mean ms, p99 ms) for the summary.
    perf: Vec<(&'static str, f32, f32)>,
    /// Motion scenario: the player's rendered body and bone positions per captured frame.
    motion: Vec<[f32; 6]>,
    video_dir: Option<PathBuf>,
    /// Scenes with REVC_TEST_FIRE=explosion: the current scene's explosion was set off.
    fire_done: bool,
}

thread_local! {
    static DRIVER: RefCell<Option<Driver>> = const { RefCell::new(None) };
}

static EXIT_CODE: AtomicI32 = AtomicI32::new(0);

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn log(msg: impl AsRef<str>) {
    println!("[revc-test] {}", msg.as_ref());
}

/// Reads REVC_TEST; returns true if a test scenario is active.
pub fn init() -> Result<bool, String> {
    let Ok(name) = std::env::var("REVC_TEST") else { return Ok(false) };
    let scenario = match name.as_str() {
        "boot" => Scenario::Boot,
        "saveload" => Scenario::SaveLoad,
        "audio" => Scenario::Audio,
        "formats" => Scenario::Formats,
        "texmods" => Scenario::TexMods,
        "gamepad" => Scenario::Gamepad,
        "spots" => Scenario::Spots,
        "savegame" => Scenario::SaveGame,
        "motion" => Scenario::Motion,
        "pedhit" => Scenario::PedHit,
        "screenshot" => Scenario::Screenshot,
        "menuwalk" => Scenario::MenuWalk,
        "scanner" => Scenario::Scanner,
        "scenes" => Scenario::Scenes,
        "menus" => Scenario::Menus,
        "padscript" => Scenario::PadScript,
        "menusweep" => Scenario::MenuSweep,
        "cutscenes" => Scenario::Cutscenes,
        other => return Err(format!("unknown REVC_TEST scenario '{other}' (boot, saveload, audio, formats, texmods, gamepad, spots, savegame, motion, pedhit, scanner, scenes, menus, padscript, menusweep, cutscenes)")),
    };
    let timeout: u64 = env("REVC_TEST_TIMEOUT", 180);
    let d = Driver {
        scenario,
        slot: env("REVC_TEST_SLOT", 1i32) - 1,
        wait_ms: env("REVC_TEST_MS", 8000),
        shot: std::env::var_os("REVC_TEST_SHOT").map(PathBuf::from),
        reference: std::env::var_os("REVC_TEST_REF").map(PathBuf::from),
        max_diff: env("REVC_TEST_MAXDIFF", 0.005),
        phase: Phase::WaitFrontend { frames: 0 },
        started: Instant::now(),
        failures: Vec::new(),
        finished: false,
        pads: Default::default(),
        pad_capture_requested: false,
        last_tick: None,
        frame_ms: Vec::new(),
        perf: Vec::new(),
        motion: Vec::new(),
        video_dir: std::env::var_os("REVC_TEST_VIDEO_DIR").map(PathBuf::from),
        fire_done: false,
    };
    log(format!("scenario {scenario:?} started"));
    if std::env::var_os("REVC_TEST_WAV").is_some() {
        crate::audio::start_recording();
    }
    DRIVER.with(|cell| *cell.borrow_mut() = Some(d));

    // Watchdog: also catches hangs where the main loop stops calling us.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(timeout + 30));
        log(format!("FAIL timeout: no result after {}s (hang?)", timeout + 30));
        // _exit, not exit: exit runs the C atexit handlers, and with the main thread stuck inside
        // the GL driver (e.g. waiting for a frame callback of a window nobody can see) their
        // teardown blocks too; the process then hung forever (seen on the N150 with its screen locked).
        unsafe extern "C" {
            fn _exit(code: i32) -> !;
        }
        unsafe { _exit(3) }
    });
    Ok(true)
}

/// Exit code for main(): 0 pass, 1 fail. Only meaningful in test mode.
pub fn exit_code() -> i32 {
    let finished = DRIVER.with(|c| c.borrow().as_ref().is_some_and(|d| d.finished));
    if !finished {
        log("FAIL game exited before the scenario finished");
        return 1;
    }
    EXIT_CODE.load(Ordering::Relaxed)
}

/// Called once per main-loop iteration (from hostPollEvents, after input dispatch).
pub fn tick() {
    let quit = DRIVER.with(|cell| {
        let mut b = cell.borrow_mut();
        let Some(d) = b.as_mut() else { return false };
        d.step()
    });
    if quit {
        unsafe { revc_test_quit() };
    }
}

impl Driver {
    /// REVC_TEST_PROMPTS=KEY,KEY…: logs each key's text as the HUD shows it (controls filled in);
    /// REVC_TEST_EXPECT_PROMPTS=KEY=text;KEY=text…: fails unless KEY's text contains that text.
    /// REVC_TEST_CUT_LOAD=cut,cut…: loads each cut's animations and camera path and logs what came
    /// in (testhooks.cpp revc_test_cut_load).
    fn check_cut_loads(&mut self) {
        let Ok(cuts) = std::env::var("REVC_TEST_CUT_LOAD") else { return };
        for cut in cuts.split(',').filter(|c| !c.is_empty()) {
            let Ok(c) = std::ffi::CString::new(cut) else { continue };
            let mut buf = [0 as std::ffi::c_char; 2048];
            let n = unsafe { revc_test_cut_load(c.as_ptr(), buf.as_mut_ptr(), buf.len() as i32) };
            let text: String = buf[..n.clamp(0, buf.len() as i32 - 1) as usize].iter().map(|&c| c as u8 as char).collect();
            log(format!("cut {cut}: {text}"));
        }
    }

    fn check_prompts(&mut self) {
        let Ok(keys) = std::env::var("REVC_TEST_PROMPTS") else { return };
        let expect = std::env::var("REVC_TEST_EXPECT_PROMPTS").unwrap_or_default();
        for key in keys.split(',').filter(|k| !k.is_empty()) {
            let Ok(k) = std::ffi::CString::new(key) else { continue };
            let mut buf = [0 as std::ffi::c_char; 512];
            let n = unsafe { revc_test_prompt_text(k.as_ptr(), buf.as_mut_ptr(), buf.len() as i32) };
            let text: String = buf[..n.max(0) as usize].iter().map(|&c| c as u8 as char).collect();
            log(format!("prompt {key}: {text}"));
            for want in expect.split(';').filter_map(|e| e.strip_prefix(key)?.strip_prefix('=')) {
                if !text.contains(want) {
                    self.fail(format!("prompt {key}: {text:?} lacks {want:?}"));
                }
            }
        }
    }

    fn fail(&mut self, msg: String) {
        log(format!("FAIL {msg}"));
        self.failures.push(msg);
    }

    /// Advances the scenario; returns true when the game should quit.
    fn step(&mut self) -> bool {
        if matches!(self.phase, Phase::Done) {
            return false;
        }
        let now_real = Instant::now();
        let frame_ms = self.last_tick.map(|t| (now_real - t).as_secs_f32() * 1000.0);
        self.last_tick = Some(now_real);
        let timeout = Duration::from_secs(env("REVC_TEST_TIMEOUT", 180));
        if self.started.elapsed() > timeout {
            self.fail(format!("timeout in phase {:?}", self.phase));
            return self.finish();
        }
        let state = unsafe { revc_test_state() };
        match self.phase {
            Phase::WaitFrontend { frames } => {
                if state != STATE_FRONTEND {
                    return false;
                }
                // Let the menu settle for a few frames.
                if frames < 30 {
                    self.phase = Phase::WaitFrontend { frames: frames + 1 };
                    return false;
                }
                log(format!("front end reached after {:.1}s", self.started.elapsed().as_secs_f32()));
                match self.scenario {
                    Scenario::Boot => {
                        if unsafe { revc_test_check_prompt_menu() } == 0 {
                            self.fail("Controls menu lacks the BUTTON PROMPTS entry (or its text)".into());
                        }
                        if unsafe { revc_test_check_pad_layouts() } == 1 {
                            log("pad layouts: test config is Original PC; Xbox Standard, Xbox Classic and Original PC apply and are recognised");
                        } else {
                            self.fail("pad layouts: a preset is not applied or not recognised".into());
                        }
                        if self.shot.is_none() {
                            return self.finish();
                        }
                        // The Controls page (with revc's BUTTON PROMPTS entry) as the boot screenshot.
                        unsafe { revc_test_show_controls_menu() };
                        self.phase = Phase::MenuShot { frames: 0 };
                        return false;
                    }
                    Scenario::Cutscenes if std::env::var_os("REVC_TEST_CUT_NEW").is_none() => {
                        crate::rwtexture::take_missing(); // (from the front end's loading)
                        if unsafe { revc_test_load_slot(self.slot) } == 0 {
                            self.fail(format!("save slot {} missing or invalid", self.slot + 1));
                            return self.finish();
                        }
                        log(format!("loading slot {}", self.slot + 1));
                        self.phase = Phase::WaitPlaying;
                    }
                    Scenario::Cutscenes => {
                        crate::rwtexture::take_missing(); // (from the front end's loading)
                        if unsafe { revc_test_new_game() } == 0 {
                            self.fail("cutscenes: couldn't start a new game".into());
                            return self.finish();
                        }
                        log("starting a new game");
                        self.phase = Phase::WaitPlaying;
                    }
                    Scenario::SaveLoad | Scenario::Audio | Scenario::Formats | Scenario::TexMods | Scenario::Gamepad | Scenario::Spots | Scenario::SaveGame | Scenario::Motion | Scenario::PedHit | Scenario::Scanner | Scenario::Scenes | Scenario::Menus | Scenario::PadScript | Scenario::MenuSweep | Scenario::Screenshot | Scenario::MenuWalk => {
                        if unsafe { revc_test_load_slot(self.slot) } == 0 {
                            self.fail(format!("save slot {} missing or invalid", self.slot + 1));
                            return self.finish();
                        }
                        log(format!("loading slot {}", self.slot + 1));
                        self.phase = Phase::WaitPlaying;
                    }
                }
            }
            Phase::WaitPlaying => {
                if state == STATE_PLAYING && self.scenario == Scenario::Cutscenes {
                    log(format!("playing after {:.1}s", self.started.elapsed().as_secs_f32()));
                    let t0 = unsafe { revc_test_game_ms() };
                    self.phase = Phase::Cutscenes { t0, last: 0 };
                    return false;
                }
                if state == STATE_PLAYING {
                    log(format!("playing after {:.1}s", self.started.elapsed().as_secs_f32()));
                    crate::audio::take_stats(); // count sound effects and streams from here on
                    crate::audio::take_stream_frames();
                    crate::audio::take_nonzero_sends();
                    self.check_loaded();
                    self.phase = Phase::Playing { start_ms: unsafe { revc_test_game_ms() }, start_frame: unsafe { revc_test_frame() } };
                }
            }
            Phase::Playing { start_ms, start_frame } => {
                if state != STATE_PLAYING {
                    self.fail("left the game unexpectedly".into());
                    return self.finish();
                }
                let elapsed = unsafe { revc_test_game_ms() }.wrapping_sub(start_ms);
                let frames = unsafe { revc_test_frame() }.wrapping_sub(start_frame);
                if elapsed < self.wait_ms || frames < 30 || unsafe { revc_test_fading() } != 0 {
                    return false;
                }
                if self.scenario == Scenario::Audio {
                    log(format!("script starts after {elapsed} ms game time"));
                    crate::audio::take_stats();
                    crate::audio::take_nonzero_sends();
                    self.phase = Phase::Script { t0: unsafe { revc_test_game_ms() }, step: 0 };
                    return false;
                }
                if self.scenario == Scenario::Motion {
                    log(format!("motion capture starts after {elapsed} ms game time"));
                    crate::host::CAPTURE_KEEPS_ALPHA.store(true, Ordering::Relaxed);
                    crate::host::request_capture();
                    self.phase = Phase::Motion { t0: unsafe { revc_test_game_ms() }, step: 0 };
                    return false;
                }
                if self.scenario == Scenario::Scanner {
                    if unsafe { revc_test_scanner_setup() } == 0 {
                        self.fail("scanner: police car model didn't load".into());
                        return self.finish();
                    }
                    log("scanner: player in a police car, wanted level 2; recording 30 s".to_string());
                    self.phase = Phase::Scanner { t0: unsafe { revc_test_game_ms() } };
                    return false;
                }
                if self.scenario == Scenario::MenuWalk {
                    self.phase = Phase::MenuWalk;
                    return false;
                }
                if self.scenario == Scenario::Screenshot {
                    // the game's screenshot key (core/main.cpp): it crashed when librw's camera
                    // readback came back empty; now the host saves the next frame
                    crate::host::inject_key(1012, true);
                    self.phase = Phase::Screenshot { frame: 0 };
                    return false;
                }
                if self.scenario == Scenario::PedHit {
                    let kmh: f32 = std::env::var("REVC_TEST_PEDHIT_KMH").ok().and_then(|v| v.parse().ok()).unwrap_or(20.0);
                    if unsafe { revc_test_pedhit_setup(kmh) } == 0 {
                        self.fail("pedhit: no flat spot found or the car/ped model didn't load".into());
                        return self.finish();
                    }
                    log(format!("pedhit: car at {kmh} km/h towards 5 peds"));
                    self.phase = Phase::PedHit;
                    return false;
                }
                if self.scenario == Scenario::Formats {
                    self.check_formats();
                    return self.finish();
                }
                if self.scenario == Scenario::SaveGame {
                    self.save_game();
                    return self.finish();
                }
                if self.scenario == Scenario::Gamepad {
                    self.phase = Phase::Pads;
                    return false;
                }
                if self.scenario == Scenario::MenuSweep {
                    let (_, x, y, heading, hour, minute, weather) = SCENES[0];
                    unsafe { revc_test_scene(x, y, heading, hour, minute, weather) };
                    unsafe { revc_test_pad(1, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) };
                    self.phase = Phase::MenuSweep { idx: 0, button: 0, frame: 0, flagged: 0 };
                    return false;
                }
                if self.scenario == Scenario::PadScript || (self.scenario == Scenario::Menus && motion_route().is_some()) {
                    self.phase = Phase::PadScript { frame: 0 };
                    return false;
                }
                if self.scenario == Scenario::Menus {
                    self.phase = Phase::Menus { idx: 0, frames: 0 };
                    return false;
                }
                if self.scenario == Scenario::Scenes {
                    let base = (unsafe { revc_test_game_ms() } / 1000 + 1) * 1000;
                    self.phase = Phase::Scene { idx: 0, base, stage: SpotStage::Travel };
                    return false;
                }
                if self.scenario == Scenario::Spots {
                    // A round start time; the save always loads at the same game time.
                    let base = (unsafe { revc_test_game_ms() } / 1000 + 1) * 1000;
                    self.phase = Phase::Spot { idx: 0, base, stage: SpotStage::Travel };
                    return false;
                }
                // REVC_TEST_MENU_INGAME=1 (with REVC_TEST_MENU_PAGE): capture that menu page over
                // the loaded game instead (menu skins, plan task 13g); no reference.
                if std::env::var_os("REVC_TEST_MENU_INGAME").is_some() {
                    self.reference = None;
                    unsafe { revc_test_show_controls_menu() };
                    self.phase = Phase::MenuShot { frames: 0 };
                    return false;
                }
                log(format!("captured after {elapsed} ms game time, {frames} frames"));
                self.check_audio();
                self.check_player();
                if self.scenario == Scenario::TexMods {
                    self.check_texmods();
                }
                if self.shot.is_some() || self.reference.is_some() {
                    crate::host::request_capture();
                    self.phase = Phase::Capturing;
                } else {
                    return self.finish();
                }
            }
            Phase::Script { t0, step } => {
                let t = unsafe { revc_test_game_ms() }.wrapping_sub(t0);
                let mut step = step;
                while let Some(&(at, key, down)) = AUDIO_SCRIPT.get(step) {
                    if t < at {
                        break;
                    }
                    if key == 0 {
                        self.check_audio();
                        return self.finish();
                    }
                    crate::host::inject_key(key, down);
                    step += 1;
                }
                self.phase = Phase::Script { t0, step };
            }
            Phase::Cutscenes { t0, last } => return self.cutscenes_step(t0, last),
            Phase::Scanner { t0 } => {
                if unsafe { revc_test_game_ms() }.wrapping_sub(t0) < 30000 {
                    return false;
                }
                return self.finish();
            }
            Phase::PedHit => {
                if unsafe { revc_test_pedhit_sample() } < 6000 {
                    return false;
                }
                let (mut rise, mut dist) = ([0.0f32; 5], 0.0f32);
                unsafe { revc_test_pedhit_result(rise.as_mut_ptr(), &mut dist) };
                let list: Vec<String> = rise.iter().map(|r| format!("{r:.2}")).collect();
                log(format!("pedhit: car moved {dist:.1} m; highest point above start per ped (m): {}", list.join(" ")));
                let (mut body, mut head, mut hit) = ([0.0f32; 5], [0.0f32; 5], [0i32; 5]);
                unsafe { revc_test_pedhit_drawn(body.as_mut_ptr(), head.as_mut_ptr(), hit.as_mut_ptr()) };
                let f = |v: &[f32]| v.iter().map(|r| format!("{r:.2}")).collect::<Vec<_>>().join(" ");
                log(format!("pedhit: as drawn: body {}; head {}; hit {:?}", f(&body), f(&head), hit));
                // A drawn ped far above where the physics has it: the render interpolation is
                // blending something it shouldn't (peds thrown "two cars high" at 320 Hz).
                for i in 0..5 {
                    if head[i] > rise[i] + 2.5 || body[i] > rise[i] + 0.5 {
                        self.fail(format!("pedhit: ped {i} drawn {:.1} m up (head) while the physics has it at {:.1} m", head[i], rise[i]));
                    }
                }
                if hit.iter().filter(|&&h| h != 0).count() < 3 {
                    self.fail(format!("pedhit: the car hit only {} of 5 peds", hit.iter().filter(|&&h| h != 0).count()));
                }
                if dist < 10.0 {
                    self.fail(format!("pedhit: the car only moved {dist:.1} m"));
                }
                return self.finish();
            }
            Phase::Motion { t0, step } => {
                let t = unsafe { revc_test_game_ms() }.wrapping_sub(t0);
                let mut step = step;
                while let Some(&(at, key, down)) = MOTION_SCRIPT.get(step) {
                    if t < at {
                        break;
                    }
                    crate::host::inject_key(key, down);
                    step += 1;
                }
                self.phase = Phase::Motion { t0, step };
                if let Some((w, h, rgba)) = crate::host::take_capture() {
                    let mut sample = [0.0f32; 6];
                    unsafe { revc_test_motion_sample(sample.as_mut_ptr()) };
                    let n = self.motion.len();
                    self.motion.push(sample);
                    if let Some(dir) = &self.video_dir {
                        let path = dir.join(format!("frame_{n:04}.png"));
                        if let Err(e) = write_png(&path, w, h, &rgba) {
                            self.fail(format!("writing {}: {e}", path.display()));
                            return self.finish();
                        }
                    }
                    if self.motion.len() >= MOTION_FRAMES {
                        crate::host::inject_key(1019, false);
                        crate::host::inject_key(1021, false);
                        self.check_motion();
                        return self.finish();
                    }
                    crate::host::request_capture();
                }
            }
            Phase::Capturing => {
                if let Some((w, h, rgba)) = crate::host::take_capture() {
                    self.check_image(w, h, &rgba);
                    return self.finish();
                }
            }
            Phase::Spot { idx, base, stage } => {
                unsafe { revc_test_clear_help() };
                let now = unsafe { revc_test_game_ms() };
                let visit = base + idx as u32 * SPOT_PERIOD_MS;
                // Frame times over the last 3 s before the capture (streaming has settled).
                if stage == SpotStage::Settle && now + PERF_WINDOW_MS >= visit + SPOT_WAIT_MS {
                    self.frame_ms.extend(frame_ms);
                }
                match stage {
                    SpotStage::Travel if now >= visit => {
                        self.goto_spot(idx);
                        self.phase = Phase::Spot { idx, base, stage: SpotStage::Settle };
                    }
                    SpotStage::Settle if now >= visit + SPOT_WAIT_MS && unsafe { revc_test_fading() } == 0 => {
                        log(format!("{}: captured at game time {now}", SPOTS[idx].0));
                        self.report_perf(SPOTS[idx].0);
                        crate::host::request_capture();
                        self.phase = Phase::Spot { idx, base, stage: SpotStage::Capturing };
                    }
                    SpotStage::Capturing => {
                        if let Some((w, h, rgba)) = crate::host::take_capture() {
                            let name = SPOTS[idx].0;
                            let dir = |var: &str| std::env::var_os(var).map(|d| PathBuf::from(d).join(format!("{name}.png")));
                            let reference = dir("REVC_TEST_REF_DIR").filter(|p| p.exists());
                            if std::env::var_os("REVC_TEST_REF_DIR").is_some() && reference.is_none() {
                                self.fail(format!("{name}: no reference image (run with --bless)"));
                            }
                            self.check_image_at(w, h, &rgba, dir("REVC_TEST_SHOT_DIR"), reference, &format!("{name}: "), None, &[]);
                            if idx + 1 < SPOTS.len() {
                                self.phase = Phase::Spot { idx: idx + 1, base, stage: SpotStage::Travel };
                            } else {
                                return self.finish();
                            }
                        }
                    }
                    _ => {}
                }
            }
            Phase::Menus { frames, .. } if self.video_dir.is_some() && std::env::var_os("REVC_TEST_MOTION").is_some() => {
                // Motion test (MOTIONS): the start state, 0.5 s, one input, every frame written.
                let Some(&(id, page, row, context, layout, input, total)) = motion() else {
                    self.fail(format!("unknown REVC_TEST_MOTION {:?}", std::env::var("REVC_TEST_MOTION")));
                    return self.finish();
                };
                if let Some((w, h, rgba)) = crate::host::take_capture() {
                    let n = MENU_VIDEO_FRAME.fetch_add(1, Ordering::Relaxed);
                    let path = self.video_dir.as_ref().unwrap().join(format!("frame_{n:05}.png"));
                    if let Err(e) = write_png(&path, w, h, &rgba) {
                        self.fail(format!("writing {}: {e}", path.display()));
                        return self.finish();
                    }
                }
                let name = std::ffi::CString::new(page).unwrap();
                let p = unsafe { revc_test_menu_page_id(name.as_ptr()) };
                if frames == 0 {
                    // nothing recorded until the start state is set
                    if unsafe { revc_test_menu_state(p, row, context, layout, -1) } != 0 {
                        MENU_VIDEO_FRAME.store(0, Ordering::Relaxed);
                        self.phase = Phase::Menus { idx: 0, frames: 1 };
                        crate::host::request_capture();
                    }
                    return false;
                }
                if frames < MOTION_INPUT_FRAME {
                    unsafe { revc_test_menu_state(p, row, -1, -1, -1) };	// pages reset the row on entry
                } else {
                    motion_input(input, p, frames - MOTION_INPUT_FRAME);
                }
                if frames >= total {
                    log(format!("motion {id}: {total} frames, input at frame {MOTION_INPUT_FRAME}"));
                    return self.finish();
                }
                self.phase = Phase::Menus { idx: 0, frames: frames + 1 };
                crate::host::request_capture();
            }
            Phase::Menus { idx, frames } if self.video_dir.is_some() => {
                // Walk-through clip (REVC_TEST_VIDEO_DIR, run at REVC_FIXED_FPS=60): every frame
                // written; each state held 1.5 s; on row pages the highlight moves down at 0.5 s
                // and back at 1 s, on the radio row the picker steps right and back.
                let (_, shots) = menu_shots();
                let (id, page, option, context, layout, legend, _) = shots[idx];
                if let Some((w, h, rgba)) = crate::host::take_capture() {
                    let n = MENU_VIDEO_FRAME.fetch_add(1, Ordering::Relaxed);
                    let path = self.video_dir.as_ref().unwrap().join(format!("frame_{n:05}.png"));
                    if let Err(e) = write_png(&path, w, h, &rgba) {
                        self.fail(format!("writing {}: {e}", path.display()));
                        return self.finish();
                    }
                }
                let name = std::ffi::CString::new(page).unwrap();
                let p = unsafe { revc_test_menu_page_id(name.as_ptr()) };
                if frames == 0 {
                    if unsafe { revc_test_menu_state(p, option, context, layout, legend) } != 0 {
                        self.phase = Phase::Menus { idx, frames: 1 };
                    }
                    crate::host::request_capture();
                    return false;
                }
                let radio = id.contains("radio");
                let still = id.ends_with("_still");	// held without input, 5 s
                if id == "mapzoom_still" {
                    let dir = match frames { 1..=90 => 1, 151..=240 => -1, _ => 0 };
                    unsafe { revc_test_map_zoom(dir) };
                }
                match if still { 0 } else { frames } {
                    30 if radio => unsafe { revc_test_menu_input(0, 1) },
                    60 if radio => unsafe { revc_test_menu_input(0, -1) },
                    30 if !id.contains("controls_") && !id.starts_with("map") => unsafe { revc_test_menu_input(1, 0) },
                    _ => {}
                }
                if frames >= if still { 300 } else { 90 } {
                    if idx + 1 < shots.len() {
                        self.phase = Phase::Menus { idx: idx + 1, frames: 0 };
                    } else {
                        return self.finish();
                    }
                } else {
                    self.phase = Phase::Menus { idx, frames: frames + 1 };
                }
                crate::host::request_capture();
            }
            Phase::Menus { idx, frames } => {
                let (set, shots) = menu_shots();
                let (id, page, option, context, layout, legend, wait) = shots[idx];
                if frames == 0 {
                    let name = std::ffi::CString::new(page).unwrap();
                    let p = unsafe { revc_test_menu_page_id(name.as_ptr()) };
                    if unsafe { revc_test_menu_state(p, option, context, layout, legend) } != 0 {
                        *MENU_SHOT_START.lock().unwrap() = Some(std::time::Instant::now());
                        self.phase = Phase::Menus { idx, frames: 1 };
                    }
                } else if MENU_SHOT_START.lock().unwrap().is_none_or(|t| t.elapsed().as_millis() < wait as u128) {
                    // keep the row selected (pages reset it on entry)
                    let name = std::ffi::CString::new(page).unwrap();
                    let p = unsafe { revc_test_menu_page_id(name.as_ptr()) };
                    unsafe { revc_test_menu_state(p, option, -1, -1, -1) };
                    self.phase = Phase::Menus { idx, frames: frames + 1 };
                } else if frames < u32::MAX {
                    // waited long enough (real time: page fades and the highlight have settled)
                    crate::host::request_capture();
                    self.phase = Phase::Menus { idx, frames: u32::MAX };
                } else if let Some((w, h, rgba)) = crate::host::take_capture() {
                    if let Some(dir) = std::env::var_os("REVC_TEST_SHOT_DIR") {
                        let path = PathBuf::from(dir).join(format!("{set}_{id}.png"));
                        if let Err(e) = write_png(&path, w, h, &rgba) {
                            self.fail(format!("writing {}: {e}", path.display()));
                        }
                    }
                    log(format!("menu {set}_{id}: captured {w}x{h}"));
                    if idx + 1 < shots.len() {
                        self.phase = Phase::Menus { idx: idx + 1, frames: 0 };
                    } else {
                        return self.finish();
                    }
                }
            }
            Phase::MenuSweep { idx, button, frame, flagged } => {
                // Every pad button on every menu state of the console UI style (the menu's tab
                // pages, their tab/icon bar and each row, the "advanced" list): set the state,
                // let it settle, press the button for 4 frames, look 40 frames later. One CSV line
                // per press (REVC_TEST_SWEEP_OUT); flagged: a page the console style doesn't
                // have, a fade still running, or the menu closed by a button that shouldn't.
                use std::io::Write;
                const BUTTONS: usize = crate::padscript::BUTTONS.len();
                let (mut page, mut row) = (0i32, 0i32);
                if unsafe { revc_test_sweep_state(idx, &mut page, &mut row) } == 0 {
                    unsafe { revc_test_pad(0, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) };
                    log(format!("menu sweep: {idx} states x {BUTTONS} buttons, {flagged} flagged"));
                    return self.finish();
                }
                let next = |button: u32| if button as usize + 1 < BUTTONS { (idx, button + 1) } else { (idx + 1, 0) };
                if row == -2 {
                    self.phase = Phase::MenuSweep { idx: idx + 1, button: 0, frame: 0, flagged };
                    return false;
                }
                match frame {
                    0 => {
                        unsafe { revc_test_pad(1, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) };
                        if unsafe { revc_test_menu_state(page, row.max(0), -1, -1, -1) } == 0 {
                            return false;	// (opening the menu first)
                        }
                        unsafe { revc_test_menu_set_focus(page, (row >= 0) as i32) };
                    }
                    1..=29 => unsafe { revc_test_menu_set_focus(page, (row >= 0) as i32) },
                    30 => unsafe { revc_test_pad(1, 1 << button, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) },
                    34 => unsafe { revc_test_pad(1, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) },
                    74 => {
                        let (mut open, mut p2, mut r2, mut skinned, mut fading) = (0, 0, 0, 0, 0);
                        unsafe { revc_test_menu_status(&mut open, &mut p2, &mut r2, &mut skinned, &mut fading) };
                        let name = crate::padscript::BUTTONS[button as usize];
                        let mut flag = String::new();
                        if open != 0 && skinned == 0 {
                            flag = "foreign_page".into();
                        } else if fading != 0 {
                            flag = "still_fading".into();
                        } else if open == 0 && name != "start" && !(row < 0 && (name == "triangle" || name == "circle")) {
                            flag = "closed".into();
                        }
                        let flagged = flagged + (!flag.is_empty()) as u32;
                        if let Some(path) = std::env::var_os("REVC_TEST_SWEEP_OUT") {
                            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                                let _ = writeln!(f, "{page},{row},{name},{open},{p2},{r2},{skinned},{fading},{flag}");
                            }
                        }
                        let (ni, nb) = next(button);
                        self.phase = Phase::MenuSweep { idx: ni, button: nb, frame: 0, flagged };
                        return false;
                    }
                    _ => {}
                }
                self.phase = Phase::MenuSweep { idx, button, frame: frame + 1, flagged };
            }
            Phase::PadScript { frame } => {
                // A pad script (padscript.rs) from a named start state: menu_base = scene S1
                // (in game, on foot, menu closed). The start state is set, settles PAD_SETTLE
                // frames, then the script plays on pad 0 at 60 Hz (run with REVC_FIXED_FPS=60).
                // Frames from 30 before mark:input (or the start) to mark:end (or 60 after the last
                // step) go to REVC_TEST_VIDEO_DIR as frame_%05d.png.
                const PAD_SETTLE: u32 = 150;
                let script = match pad_script() {
                    Ok(s) => s,
                    Err(e) => {
                        self.fail(e);
                        return self.finish();
                    }
                };
                if frame == 0 {
                    // Start states (research autopad README): menu_base = scene S1; world motions
                    // on foot / in an Infernus / on a PCJ-600 (testhooks.cpp revc_test_world_start)
                    let (_, x, y, heading, hour, minute, weather) = SCENES[0];
                    match script.start.as_str() {
                        "menu_base" => unsafe { revc_test_scene(x, y, heading, hour, minute, weather); },
                        "foot_open" => unsafe { revc_test_world_start(0, 560.0, -700.0, 0.0) },	// beach sand, flat and clear
                        "car_straight" => unsafe { revc_test_world_start(1, MOONLITE.0, MOONLITE.1, 0.0) },	// Ocean Drive, north
                        "car_open" => unsafe { revc_test_world_start(1, -1500.0, -1200.0, 0.0) },	// the airport tarmac (z 13.8)
                        "bike_straight" => unsafe { revc_test_world_start(2, MOONLITE.0, MOONLITE.1, 0.0) },
                        "bloodrb_straight" => unsafe { revc_test_world_start(3, MOONLITE.0, MOONLITE.1, 0.0) },	// the PS2 tests' cheat car
                        "bloodrb_open" => unsafe { revc_test_world_start(3, -1500.0, -1200.0, 0.0) },
                        "heli_open" => unsafe { revc_test_world_start(4, -1500.0, -1200.0, 0.0) },	// a Maverick on the airport tarmac
                        other => {
                            self.fail(format!("pad script start state {other:?} not known (menu_base, foot_open, car_straight, car_open, bike_straight, bloodrb_straight, bloodrb_open, heli_open)"));
                            return self.finish();
                        }
                    }
                    MENU_VIDEO_FRAME.store(0, Ordering::Relaxed);
                    // the pad is in use from the start (as a player holding one), not from the
                    // first press: the console styles pick the pad's controls by it
                    unsafe { revc_test_pad(1, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) };
                }
                let rec_start = script.mark("input").map_or(0, |f| f.saturating_sub(MOTION_INPUT_FRAME));
                let rec_end = script.mark("end").unwrap_or(script.last_frame() + 60);
                if let Some((w, h, rgba)) = crate::host::take_capture() {
                    if let Some(dir) = &self.video_dir {
                        let n = MENU_VIDEO_FRAME.fetch_add(1, Ordering::Relaxed);
                        if let Err(e) = write_png(&dir.join(format!("frame_{n:05}.png")), w, h, &rgba) {
                            self.fail(format!("writing a frame: {e}"));
                            return self.finish();
                        }
                    }
                }
                if frame >= PAD_SETTLE {
                    let t = frame - PAD_SETTLE;
                    telemetry_line(t);
                    let st = script.state_at(t);
                    let a = st.axes;
                    unsafe { revc_test_pad(1, st.buttons, a[0], a[1], a[2], a[3], a[4], a[5]) };
                    if t > rec_end {
                        unsafe { revc_test_pad(0, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) };
                        let (mut open, mut page, mut row) = (0, 0, 0);
                        let focus = unsafe { revc_test_menu_where(&mut open, &mut page, &mut row) };
                        log(format!("pad script {}: {} frames, ends with menu open {open} page {page} row {row} focus {focus}", script.id, t));
                        return self.finish();
                    }
                    if t >= rec_start && self.video_dir.is_some() {
                        crate::host::request_capture();
                    }
                }
                self.phase = Phase::PadScript { frame: frame + 1 };
            }
            Phase::MenuWalk => {
                // every page reachable in the console style's menu (testhooks.cpp revc_test_menu_walk):
                // none may be a PC menu page outside the listed exceptions
                let (mut page, mut row) = (0i32, 0i32);
                unsafe { revc_test_sweep_state(0, &mut page, &mut row) };
                if unsafe { revc_test_menu_state(page, 0, -1, -1, -1) } == 0 {
                    return false; // (opening the menu first)
                }
                let mut buf = vec![0 as std::ffi::c_char; 4096];
                let bad = unsafe { revc_test_menu_walk(buf.as_mut_ptr(), buf.len() as i32) };
                let text: String = buf.iter().take_while(|&&c| c != 0).map(|&c| c as u8 as char).collect();
                log(format!("menu walk (style {}): {bad} switch(es) to a PC page: {text}", crate::texmods::style()));
                if bad > 0 {
                    self.fail(format!("menu walk: {bad} switch(es) end on a PC menu page"));
                }
                return self.finish();
            }
            Phase::Screenshot { frame } => {
                // held over several frames: the pads update in the 30 Hz step, not every frame
                if frame == 10 {
                    crate::host::inject_key(1012, false);
                }
                let shot = std::fs::read_dir(".").ok().and_then(|d| {
                    d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).find(|n| n.starts_with("screen_") && n.ends_with(".png"))
                });
                if let Some(n) = shot {
                    log(format!("screenshot: F12 saved {n} after {frame} frames"));
                    return self.finish();
                }
                if frame >= 120 {
                    self.fail("screenshot: F12 saved no screen_*.png within 120 frames".into());
                    return self.finish();
                }
                self.phase = Phase::Screenshot { frame: frame + 1 };
            }
            Phase::Scene { idx, base, stage } => {
                let scenes = scene_list();
                if scenes.is_empty() {
                    self.fail("scenes: none selected".into());
                    return self.finish();
                }
                let (name, x, y, heading, hour, minute, weather) = scenes[idx].clone();
                let now = unsafe { revc_test_game_ms() };
                let visit = base + idx as u32 * SCENE_PERIOD_MS;
                match stage {
                    SpotStage::Travel if now >= visit => {
                        let z = unsafe { revc_test_scene(x, y, heading, hour, minute, weather) };
                        // REVC_TEST_HUD_TEXT=n: one kind of HUD text on screen (testhooks.cpp)
                        if let Some(k) = std::env::var("REVC_TEST_HUD_TEXT").ok().and_then(|v| v.parse().ok()) {
                            unsafe { revc_test_hud_text(k) };
                        }
                        // REVC_TEST_FIRE=car: a burning car in front of the player (testhooks.cpp).
                        if std::env::var("REVC_TEST_FIRE").as_deref() == Ok("car") {
                            unsafe { revc_test_fire(0) };
                        }
                        self.fire_done = false;
                        self.check_prompts();
                        self.check_cut_loads();
                        // REVC_TEST_SWAP=x,y,z,from,to: a script-style building swap (testhooks.cpp)
                        if let Ok(v) = std::env::var("REVC_TEST_SWAP") {
                            let f: Vec<&str> = v.split(',').collect();
                            if let [x, y, z, a, b] = f[..] {
                                let (a, b) = (std::ffi::CString::new(a).unwrap(), std::ffi::CString::new(b).unwrap());
                                let ok = unsafe { revc_test_swap_building(x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0), z.parse().unwrap_or(0.0), a.as_ptr(), b.as_ptr()) };
                                log(format!("swap {v}: {}", if ok != 0 { "done" } else { "no such building" }));
                            }
                        }
                        log(format!("scene {name}: ({x}, {y}, {z:.2}) heading {heading} at {hour:02}:{minute:02} weather {weather}"));
                        self.phase = Phase::Scene { idx, base, stage: SpotStage::Settle };
                    }
                    // REVC_TEST_FIRE=explosion: a car explosion REVC_TEST_FIRE_MS (300) before the capture.
                    SpotStage::Settle
                        if !self.fire_done
                            && std::env::var("REVC_TEST_FIRE").as_deref() == Ok("explosion")
                            && now + std::env::var("REVC_TEST_FIRE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(300u32) >= visit + SCENE_WAIT_MS =>
                    {
                        unsafe { revc_test_fire(1) };
                        self.fire_done = true;
                    }
                    SpotStage::Settle if now >= visit + SCENE_WAIT_MS && unsafe { revc_test_fading() } == 0 => {
                        crate::host::request_capture();
                        self.phase = Phase::Scene { idx, base, stage: SpotStage::Capturing };
                    }
                    SpotStage::Settle if now + 1000 >= visit + SCENE_WAIT_MS => {
                        if let Some(k) = std::env::var("REVC_TEST_HUD_TEXT").ok().and_then(|v| v.parse::<i32>().ok()) {
                            unsafe { revc_test_hud_text(k + 100) };
                        }
                    }
                    SpotStage::Capturing => {
                        if let Some((w, h, rgba)) = crate::host::take_capture() {
                            let path = std::env::var_os("REVC_TEST_SHOT_DIR").map(|d| PathBuf::from(d).join(format!("{name}.png")));
                            if let Some(p) = path {
                                if let Err(e) = write_png(&p, w, h, &rgba) {
                                    self.fail(format!("writing {}: {e}", p.display()));
                                }
                            }
                            log(format!("{name}: captured {w}x{h} at game time {now}"));
                            // REVC_TEST_EXPECT_FIRE_FRAMES=n: the car flame's final frame must be n
                            // (29 in the Xbox style, 0 otherwise), also after the save's reload.
                            if let Some(want) = std::env::var("REVC_TEST_EXPECT_FIRE_FRAMES").ok().and_then(|v| v.parse::<i32>().ok()) {
                                let got: Vec<i32> = (0..3).map(|k| unsafe { revc_test_particle_frames(k) }).collect();
                                log(format!("particle final frames (car flame, flame, fireball): {got:?}"));
                                if got[0] != want {
                                    self.fail(format!("car flame final frame {} (want {want})", got[0]));
                                }
                            }
                            if idx + 1 < scenes.len() {
                                self.phase = Phase::Scene { idx: idx + 1, base, stage: SpotStage::Travel };
                            } else {
                                return self.finish();
                            }
                        }
                    }
                    _ => {}
                }
            }
            Phase::MenuShot { frames } => {
                if frames == 15 && std::env::var_os("REVC_TEST_MENU_INGAME").is_some() {
                    unsafe { revc_test_show_controls_menu() }; // the pause menu is open now: pick the page
                }
                if frames < 90 {
                    self.phase = Phase::MenuShot { frames: frames + 1 };
                } else {
                    crate::host::request_capture();
                    self.phase = Phase::Capturing;
                }
            }
            Phase::Pads if self.pads.capture.is_some() => {
                let name = self.pads.capture.unwrap();
                if !self.pad_capture_requested {
                    crate::host::request_capture();
                    self.pad_capture_requested = true;
                } else if let Some((w, h, rgba)) = crate::host::take_capture() {
                    let dir = |var: &str| std::env::var_os(var).map(|d| PathBuf::from(d).join(format!("{name}.png")));
                    let reference = dir("REVC_TEST_REF_DIR").filter(|p| p.exists());
                    if std::env::var_os("REVC_TEST_REF_DIR").is_some() && reference.is_none() {
                        self.fail(format!("{name}: no reference image (run with --bless)"));
                    }
                    // Only the help box with the glyphs counts: the scene behind it has things that
                    // differ between machines (a pickup bobbing by its memory address, the clock).
                    self.check_image_at(w, h, &rgba, dir("REVC_TEST_SHOT_DIR"), reference, &format!("{name}: "), Some(GLYPH_BOX), &[]);
                    self.pads.capture = None;
                    self.pad_capture_requested = false;
                }
            }
            Phase::Pads => match self.pads.tick() {
                Ok(done) => {
                    for line in self.pads.log.drain(..) {
                        log(format!("gamepad: {line}"));
                    }
                    if done {
                        return self.finish();
                    }
                }
                Err(e) => {
                    for line in self.pads.log.drain(..) {
                        log(format!("gamepad: {line}"));
                    }
                    self.fail(format!("gamepad: {e}"));
                    return self.finish();
                }
            },
            Phase::Done => {}
        }
        false
    }

    /// tests/mods: the valid overrides (PNG HUD icon, DDS radar icon, PNG floor) must have been applied and the broken file
    /// rejected once (the original kept); the screenshot shows the replaced weapon icon.
    fn check_texmods(&mut self) {
        let (applied, failed) = crate::texmods::stats();
        log(format!("texmods: {applied} textures replaced, {failed} files rejected"));
        if applied < 3 {
            self.fail(format!("texmods: only {applied} textures replaced (expected fist, radar_save, hotel_floortile2_64)"));
        }
        if failed != 1 {
            self.fail(format!("texmods: {failed} files rejected (expected siterocket.png only)"));
        }
        // radar_save.dds (DXT1, HUD texture without mips) stays a BC texture.
        let compressed = crate::texmods::compressed();
        log(format!("texmods: {compressed} replacements kept compressed (DXT)"));
        if compressed < 1 {
            self.fail("texmods: radar_save.dds was decoded instead of kept as DXT blocks".to_string());
        }
    }

    /// Logs the frame times collected at a spot: mean, median, 99th percentile, worst, fps.
    fn report_perf(&mut self, name: &'static str) {
        let mut t = std::mem::take(&mut self.frame_ms);
        if t.is_empty() {
            return;
        }
        t.sort_by(f32::total_cmp);
        let mean = t.iter().sum::<f32>() / t.len() as f32;
        let pct = |p: f32| t[((t.len() - 1) as f32 * p).round() as usize];
        let p99 = pct(0.99);
        log(format!(
            "perf {name}: {} frames, mean {mean:.2} ms ({:.0} fps), median {:.2}, p99 {p99:.2}, max {:.2} ms",
            t.len(),
            1000.0 / mean,
            pct(0.5),
            t[t.len() - 1]
        ));
        self.perf.push((name, mean, p99));
        if self.perf.len() == SPOTS.len() {
            let mean = self.perf.iter().map(|p| p.1).sum::<f32>() / self.perf.len() as f32;
            let worst = self.perf.iter().map(|p| p.2).fold(0.0, f32::max);
            log(format!("perf all spots: mean {mean:.2} ms ({:.0} fps), worst p99 {worst:.2} ms", 1000.0 / mean));
        }
    }

    fn goto_spot(&mut self, idx: usize) {
        let (name, x, y, dx, dy, dz) = SPOTS[idx];
        let z = unsafe { revc_test_goto_spot(x, y, -100.0, dx, dy, dz, 12, 0) };
        log(format!("spot {name}: ({x}, {y}, {z:.2})"));
    }

    /// Dumps what the C++ loaders hold and compares it with revc-formats (formatcheck.rs).
    fn check_formats(&mut self) {
        let path = std::env::temp_dir().join(format!("revc-formats-{}.txt", std::process::id()));
        let cpath = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
        if unsafe { revc_test_dump_formats(cpath.as_ptr()) } == 0 {
            self.fail(format!("can't write {}", path.display()));
            return;
        }
        match crate::formatcheck::check(&path) {
            Ok((summary, failures)) => {
                log(summary);
                let n = failures.len();
                for f in &failures {
                    log(format!("formats diff: {f}"));
                }
                for f in failures.into_iter().take(10) {
                    self.fail(f);
                }
                if n > 10 {
                    self.fail(format!("... {} more differences (all are in the log as 'formats diff')", n - 10));
                }
            }
            Err(e) => self.fail(e),
        }
        let _ = std::fs::remove_file(&path);
        // REVC_DFF_GOLDEN=tests/golden/models.txt (scripts/test.sh): every model built from the Rust
        // reader must hash as librw's readers built it when the file was recorded.
        if let Some(golden) = std::env::var_os("REVC_DFF_GOLDEN") {
            match std::fs::read_to_string(&golden) {
                Ok(text) => {
                    let want: std::collections::HashMap<&str, &str> = text
                        .lines()
                        .filter(|l| !l.is_empty() && !l.starts_with('#'))
                        .filter_map(|l| l.rsplit_once(' '))
                        .collect();
                    let (mut ok, mut bad) = (0, Vec::new());
                    let mut seen = 0;
                    for (name, data) in all_dffs() {
                        seen += 1;
                        let h = format!("{:016x}", unsafe { revc_test_hash_dff(data.as_ptr(), data.len()) });
                        match want.get(name.as_str()) {
                            Some(w) if *w == h => ok += 1,
                            Some(w) => bad.push(format!("{name}: {h} (golden {w})")),
                            None => bad.push(format!("{name}: {h} (not in the golden file)")),
                        }
                    }
                    let (mut pr, mut pl) = (0, 0);
                    unsafe { revc_dff_plugin_counts(&mut pr, &mut pl) };
                    log(format!("models: {ok} of {seen} DFFs match {} ({} golden entries); plugin chunks read in Rust {pr}, skipped or by librw {pl}", golden.to_string_lossy(), want.len()));
                    for b in bad.iter().take(5) {
                        log(format!("models: {b}"));
                    }
                    if !bad.is_empty() || ok != want.len() {
                        self.fail(format!("models: {} differ from the golden hashes, {} golden entries not seen", bad.len(), want.len().saturating_sub(ok)));
                    }
                }
                Err(e) => self.fail(format!("models: {}: {e}", golden.to_string_lossy())),
            }
        }
        // REVC_TXD_GOLDEN=tests/golden/textures.txt (scripts/test.sh): the Rust reader's textures
        // (every TXD loaded) must hash as librw's readers built them when the file was recorded.
        if let Some(golden) = std::env::var_os("REVC_TXD_GOLDEN") {
            let t0 = std::time::Instant::now();
            let loaded = unsafe { revc_test_load_all_txds() };
            log(format!("textures: {loaded} more TXDs loaded in {:.0} ms", t0.elapsed().as_secs_f64() * 1000.0));
            // with REVC_TXD_CHECK_DIR (e.g. the Xbox disc's files) those TXDs are part of the set:
            // use the matching golden (tests/golden/textures-xbox.txt holds PC and Xbox).
            if let Some(dir) = std::env::var_os("REVC_TXD_CHECK_DIR") {
                let (files, textures, failed) = check_txd_dir(std::path::Path::new(&dir));
                log(format!("textures: {files} TXDs from {} ({textures} textures, {failed} unreadable)", dir.to_string_lossy()));
                if failed != 0 {
                    self.fail(format!("textures: {failed} TXDs under {} not read", dir.to_string_lossy()));
                }
            }
            match crate::txdload::golden_check(std::path::Path::new(&golden)) {
                Ok((seen, missing, extra)) => {
                    log(format!("textures: {seen} distinct textures, {} missing from / {} not in {}", missing.len(), extra.len(), golden.to_string_lossy()));
                    for l in missing.iter().take(5) {
                        log(format!("textures: expected {l}"));
                    }
                    for l in extra.iter().take(5) {
                        log(format!("textures: got {l}"));
                    }
                    if !missing.is_empty() || !extra.is_empty() || seen < 1000 {
                        self.fail(format!("textures: {} expected and {} unexpected texture hashes", missing.len(), extra.len()));
                    }
                }
                Err(e) => self.fail(format!("textures: {e}")),
            }
        }
    }

    /// The game must have driven the Rust mixer (sound effects started). Volume is 0 in the
    /// test settings, so the level isn't checked, only that voices were started and mixed.
    fn check_audio(&mut self) {
        match crate::audio::take_stats() {
            Some(s) => {
                log(format!("audio: {} sound effects started, up to {} at once", s.starts, s.max_active));
                let (mut sup, mut mode, mut level) = (0i32, 0i32, 0f32);
                unsafe { revc_test_audio_fx(&mut sup, &mut mode, &mut level) };
                let mode = ["off", "EAX", "EFX", "EAX3"].get(mode as usize).copied().unwrap_or("?");
                log(format!(
                    "audio: reverb {mode} (effects supported: {}), effects level {level:.2}, send up to {:.2} ({} reverb sends set)",
                    sup != 0,
                    s.max_send,
                    crate::audio::take_nonzero_sends()
                ));
                if s.starts == 0 {
                    self.fail("no sound effects were started during play".into());
                }
            }
            None => log("audio: sound-effect mixer not open"),
        }
        match crate::audio::take_stream_frames() {
            Some(frames) => {
                // Both channels of a stereo stream are separate sources, so /2 for seconds.
                let secs = frames as f64 / 2.0 / revc_audio::mixer::OUTPUT_RATE as f64;
                log(format!("audio: {secs:.1}s of streamed audio mixed"));
                if frames == 0 {
                    self.fail("no streamed audio (music/ambience) was mixed during play".into());
                }
            }
            None => log("audio: no stream sources created"),
        }
    }

    /// The save really loaded: the game timer continues from the one in the file (a failed
    /// GenericLoad restarts the game with the timer at 0, which screenshots of a save made at the
    /// start position don't show).
    fn check_loaded(&mut self) {
        let dir = std::env::var_os("REVC_GAME_DIR").map(PathBuf::from).unwrap_or_default();
        let path = dir.join("userfiles").join(format!("GTAVCsf{}.b", self.slot + 1));
        let saved = std::fs::read(&path).ok().and_then(|d| crate::savegame::SimpleVars::from_bytes(d.get(4..)?)).map(|v| v.time_ms);
        let now = unsafe { revc_test_game_ms() };
        match saved {
            Some(t) if now >= t => log(format!("loaded: game timer {now} ms (saved at {t} ms)")),
            Some(t) => self.fail(format!("the save did not load: game timer {now} ms, the file has {t} ms")),
            None => self.fail(format!("cannot read the header of {}", path.display())),
        }
    }

    /// Saves to REVC_TEST_SAVE_SLOT and checks what the menu would show and the file itself.
    fn save_game(&mut self) {
        let slot: i32 = env("REVC_TEST_SAVE_SLOT", 2i32) - 1;
        let (mut state, mut name, mut date) = (0i32, [0u16; 260], [0u16; 70]);
        let r = unsafe { revc_test_save_slot(slot, &mut state, name.as_mut_ptr(), date.as_mut_ptr()) };
        let text = |w: &[u16]| String::from_utf16_lossy(&w[..w.iter().position(|&c| c == 0).unwrap_or(w.len())]);
        let (name, date) = (text(&name), text(&date));
        log(format!("savegame: slot {} saved (result {r}), listed as state {state}, \"{name}\", \"{date}\"", slot + 1));
        if r != 0 || state != 0 {
            self.fail(format!("save to slot {} failed: SaveSlot {r}, slot state {state}", slot + 1));
        }
        if name.is_empty() || date.len() < 19 {
            self.fail(format!("slot {} lists no name or date (\"{name}\", \"{date}\")", slot + 1));
        }
        let dir = std::env::var_os("REVC_GAME_DIR").map(PathBuf::from).unwrap_or_default();
        let path = dir.join("userfiles").join(format!("GTAVCsf{}.b", slot + 1));
        match std::fs::read(&path) {
            Ok(d) if revc_formats::save::check(&d).is_some() => log(format!("savegame: {} passes the save check ({} bytes)", path.display(), d.len())),
            Ok(d) => self.fail(format!("{} fails the save check ({} bytes)", path.display(), d.len())),
            Err(e) => self.fail(format!("cannot read {}: {e}", path.display())),
        }
    }

    fn check_player(&mut self) {
        let (mut x, mut y, mut z) = (0f32, 0f32, 0f32);
        unsafe { revc_test_player_pos(&mut x, &mut y, &mut z) };
        log(format!("player at ({x:.2}, {y:.2}, {z:.2})"));
        // Vice City spans roughly +-2500 in x/y. The origin or NaN means the player wasn't restored.
        let sane = [x, y, z].iter().all(|v| v.is_finite())
            && x.abs() < 3000.0
            && y.abs() < 3000.0
            && (-100.0..1000.0).contains(&z)
            && (x.abs() > 1.0 || y.abs() > 1.0);
        if !sane {
            self.fail(format!("implausible player position ({x}, {y}, {z})"));
        }
    }

    fn check_image(&mut self, w: u32, h: u32, rgba: &[u8]) {
        let skip: &[[f32; 4]] = match self.scenario {
            Scenario::SaveLoad | Scenario::SaveGame => &LOADED_HUD_SKIP,
            _ => &[],
        };
        self.check_image_at(w, h, rgba, self.shot.clone(), self.reference.clone(), "", None, skip);
    }

    /// Writes the frame to `shot` and compares it with `reference` (both optional); `label`
    /// prefixes the log lines when several images are checked in one run. `region` (fractions of
    /// the width and height: x0, y0, x1, y1) limits the comparison to that part of the image;
    /// pixels in `skip` (the same form) aren't compared.
    #[allow(clippy::too_many_arguments)]
    fn check_image_at(
        &mut self,
        w: u32,
        h: u32,
        rgba: &[u8],
        shot: Option<PathBuf>,
        reference: Option<PathBuf>,
        label: &str,
        region: Option<[f32; 4]>,
        skip: &[[f32; 4]],
    ) {
        let px = |r: [f32; 4]| ((r[0] * w as f32) as u32, (r[1] * h as f32) as u32, (r[2] * w as f32) as u32, (r[3] * h as f32) as u32);
        let (x0, y0, x1, y1) = px(region.unwrap_or([0.0, 0.0, 1.0, 1.0]));
        let skip: Vec<_> = skip.iter().map(|&r| px(r)).collect();
        let inside = |i: usize| {
            let (x, y) = (i as u32 % w, i as u32 / w);
            (x0..x1).contains(&x)
                && (y0..y1).contains(&y)
                && !skip.iter().any(|&(a, b, c, d)| (a..c).contains(&x) && (b..d).contains(&y))
        };
        if let Some(path) = shot.clone() {
            match write_png(&path, w, h, rgba) {
                Ok(()) => log(format!("screenshot {}x{} -> {}", w, h, path.display())),
                Err(e) => self.fail(format!("cannot write {}: {e}", path.display())),
            }
        }
        let Some(reference) = reference else { return };
        let (rw, rh, rref) = match read_png(&reference) {
            Ok(v) => v,
            Err(e) => {
                self.fail(format!("cannot read reference {}: {e}", reference.display()));
                return;
            }
        };
        if (rw, rh) != (w, h) {
            self.fail(format!("size {w}x{h} differs from reference {rw}x{rh}"));
            return;
        }
        // A pixel "differs" if any channel is off by more than 32/255.
        let mut diff_img = vec![0u8; rgba.len()];
        let mut differing = 0usize;
        let mut compared = 0usize;
        for (i, (a, b)) in rgba.chunks_exact(4).zip(rref.chunks_exact(4)).enumerate() {
            let d = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap();
            let counted = inside(i);
            compared += counted as usize;
            if d > 32 && counted {
                differing += 1;
                diff_img[i * 4..i * 4 + 4].copy_from_slice(&[255, 0, 255, 255]);
            } else {
                // Faded copy of the frame for orientation.
                for c in 0..3 {
                    diff_img[i * 4 + c] = a[c] / 4;
                }
                diff_img[i * 4 + 3] = 255;
            }
        }
        let frac = differing as f64 / (compared as f64).max(1.0);
        let what = if region.is_some() {
            format!("of pixels in {}x{} at {x0},{y0}", x1 - x0, y1 - y0)
        } else if !skip.is_empty() {
            format!("of {compared} compared pixels ({} boxes skipped)", skip.len())
        } else {
            "of pixels".into()
        };
        log(format!("{label}image diff: {:.3}% {what} differ (limit {:.3}%)", frac * 100.0, self.max_diff * 100.0));
        if frac > self.max_diff {
            let mut p = shot.clone().unwrap_or_else(|| std::env::temp_dir().join("revc-test.png")).into_os_string();
            p.push(".diff.png");
            let p = PathBuf::from(p);
            let _ = write_png(&p, w, h, &diff_img);
            self.fail(format!("{label}screenshot differs from reference (diff image {})", p.display()));
        }
    }

    /// Smoothness of the player's rendered motion over the captured frames (see the motion
    /// scenario above). A body or bone that is drawn at the simulation's 30 Hz positions while the
    /// rest moves at the display rate shows up as large second differences.
    fn check_motion(&mut self) {
        let limit: f64 = env("REVC_TEST_MAX_JITTER", 0.5);
        let frames = &self.motion[MOTION_SKIP.min(self.motion.len())..];
        let series = |f: &dyn Fn(&[f32; 6]) -> [f64; 3]| -> f64 {
            let p: Vec<[f64; 3]> = frames.iter().map(f).collect();
            let len = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            let mut d1 = 0.0;
            let mut d2 = 0.0;
            for w in p.windows(3) {
                d1 += len([w[2][0] - w[1][0], w[2][1] - w[1][1], w[2][2] - w[1][2]]);
                d2 += len([w[2][0] - 2.0 * w[1][0] + w[0][0], w[2][1] - 2.0 * w[1][1] + w[0][1], w[2][2] - 2.0 * w[1][2] + w[0][2]]);
            }
            if d1 > 1e-9 { d2 / d1 } else { 0.0 }
        };
        let body = series(&|s| [s[0] as f64, s[1] as f64, s[2] as f64]);
        let bone = series(&|s| [s[3] as f64, s[4] as f64, s[5] as f64]);
        let rel = series(&|s| [(s[3] - s[0]) as f64, (s[4] - s[1]) as f64, (s[5] - s[2]) as f64]);
        let first = frames.first().copied().unwrap_or_default();
        let last = frames.last().copied().unwrap_or_default();
        let dist = ((last[0] - first[0]).powi(2) + (last[1] - first[1]).powi(2)).sqrt();
        log(format!("motion: {} frames, walked {dist:.2} m, jitter body {body:.3} bone {bone:.3} bone-vs-body {rel:.3} (limit {limit})", frames.len()));
        if dist < 1.0 {
            self.fail(format!("motion: the player barely moved ({dist:.2} m)"));
        }
        if body > limit || bone > limit {
            self.fail(format!("motion: jerky movement (body {body:.3}, bone {bone:.3}, limit {limit})"));
        }
    }

    /// One frame of the cutscenes scenario (see the module notes).
    fn cutscenes_step(&mut self, t0: u32, last: u32) -> bool {
        let now = unsafe { revc_test_game_ms() };
        let mut buf = [0 as std::ffi::c_char; 160];
        let running = unsafe { revc_test_cutscene(buf.as_mut_ptr(), buf.len() as i32) } != 0;
        let full = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().to_ascii_lowercase();
        // (the name, then the time cycle's state for the log)
        let (name, tc) = full.split_once(' ').map_or((full.clone(), String::new()), |(a, b)| (a.to_string(), b.to_string()));
        let mut st = CUTSCENES.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = if running { name.clone() } else { "(game)".to_string() };
        for m in crate::rwtexture::take_missing() {
            let e = (ctx.clone(), m.to_ascii_lowercase());
            if !st.missing.contains(&e) {
                log(format!("missing texture '{}' in {}", e.1, e.0));
                st.missing.push(e);
            }
        }
        if running {
            st.last_cut = now;
            if st.trips > 0 {
                st.cut_after_trip = true;
            }
        }
        // the script waits for the player: walk into the nearest mission marker
        let quiet = now.wrapping_sub(st.last_cut.max(t0));
        let trips: u32 = env("REVC_TEST_CUT_MARKERS", 1);
        if !running && quiet >= 6000 && st.trips < trips && unsafe { revc_test_fading() } == 0 {
            let (mut x, mut y, mut z) = (0.0f32, 0.0f32, 0.0f32);
            st.trips += 1;
            st.cut_after_trip = false;
            if unsafe { revc_test_goto_marker(&mut x, &mut y, &mut z) } != 0 {
                log(format!("player put on the mission marker at ({x:.1}, {y:.1}, {z:.1})"));
            } else {
                log("no mission marker within 300 m".to_string());
            }
            st.last_cut = now;
        }
        let done = st.trips >= trips && st.cut_after_trip && !running && quiet >= 5000;
        if running && !st.seen.contains(&name) {
            log(format!("cutscene {name} at game time {} (time cycle: {tc})", now.wrapping_sub(t0)));
            st.seen.push(name.clone());
        }
        if let Some((w, h, rgba)) = crate::host::take_capture() {
            if let (Some(dir), Some(file)) = (std::env::var_os("REVC_TEST_SHOT_DIR"), st.pending.take()) {
                let p = PathBuf::from(dir).join(file);
                if let Err(e) = write_png(&p, w, h, &rgba) {
                    drop(st);
                    self.fail(format!("writing {}: {e}", p.display()));
                    st = CUTSCENES.lock().unwrap_or_else(|e| e.into_inner());
                }
            }
        }
        let mut last = last;
        let every: u32 = env("REVC_TEST_CUT_SHOT_MS", 1000);
        // (outside cutscenes a frame every 5 s, to see where the opening stands)
        let gap = if running { every } else { env("REVC_TEST_CUT_GAME_MS", 5000) };
        if st.pending.is_none() && now.wrapping_sub(last) >= gap {
            let what = if running { format!("cut_{name}") } else { "game".to_string() };
            st.pending = Some(format!("{what}_{:06}.png", now.wrapping_sub(t0)));
            crate::host::request_capture();
            last = now;
        }
        if !done && now.wrapping_sub(t0) < env("REVC_TEST_CUT_MS", 240_000u32) {
            self.phase = Phase::Cutscenes { t0, last };
            return false;
        }
        let allowed: Vec<String> = std::fs::read_to_string(std::env::var("REVC_TEST_MISSING_OK").unwrap_or_default())
            .unwrap_or_default()
            .lines()
            .map(|l| l.split('#').next().unwrap_or("").trim().to_ascii_lowercase())
            .filter(|l| !l.is_empty())
            .collect();
        let (seen, missing) = (st.seen.clone(), st.missing.clone());
        drop(st);
        log(format!("cutscenes played: {}", if seen.is_empty() { "none".to_string() } else { seen.join(", ") }));
        if seen.is_empty() {
            self.fail("cutscenes: no cutscene ran".into());
        }
        // (outside cutscenes only logged: there the game's own data lacks many, mostly LOD models
        // whose LOD dictionaries don't have the texture)
        for (ctx, tex) in missing {
            if ctx != "(game)" && !allowed.contains(&tex) {
                self.fail(format!("missing texture '{tex}' in {ctx}"));
            }
        }
        self.finish()
    }

    fn finish(&mut self) -> bool {
        self.phase = Phase::Done;
        self.finished = true;
        // a map placement skipped because a pool was full (FileLoader.cpp): the pool limits are too
        // low for some style's map data again
        unsafe { revc_test_pool_report() };
        // REVC_TEST_IMPORT_GUARD=1: in the PC style, with console imports present, nothing from the
        // import folder may be read but the allowlist (the controller-layout page's pad outline,
        // its only art source; docs/CONSOLE_STYLES.md)
        if std::env::var_os("REVC_TEST_IMPORT_GUARD").is_some() {
            // the pad outline, and the brief lists' key files (read in every style; what the
            // lists collect goes to the save's sidecar, not the save)
            const ALLOW: &[&str] =
                &["xbox/ui/r3_menu_336.dds", "xbox/neo/hints.txt", "xbox/neo/unlockeditems.txt"];
            let read = crate::discimport::accessed();
            log(format!("import guard: style {}, {} import file(s) read: {read:?}", crate::texmods::style(), read.len()));
            if crate::texmods::style() == 0 {
                for f in read.iter().filter(|f| !ALLOW.contains(&f.as_str())) {
                    self.fail(format!("PC style read console data from the import: {f}"));
                }
            }
        }
        let skipped = unsafe { revc_test_map_skipped() };
        if skipped > 0 {
            self.fail(format!("{skipped} map placement(s) skipped: a pool is full (core/config.h)"));
        }
        if let Some(path) = std::env::var_os("REVC_TEST_WAV") {
            let path = PathBuf::from(path);
            match crate::audio::finish_recording(&path) {
                Ok(frames) => log(format!(
                    "recorded {:.1}s of mixer output -> {}",
                    frames as f64 / revc_audio::mixer::OUTPUT_RATE as f64,
                    path.display()
                )),
                Err(e) => self.fail(format!("cannot write {}: {e}", path.display())),
            }
        }
        if self.failures.is_empty() {
            log(format!("PASS {:?} in {:.1}s", self.scenario, self.started.elapsed().as_secs_f32()));
        } else {
            log(format!("FAIL {:?}: {} problem(s)", self.scenario, self.failures.len()));
            EXIT_CODE.store(1, Ordering::Relaxed);
        }
        true
    }
}

/// Cutscenes scenario state: cutscenes seen, (context, texture) missing, the frame file pending.
#[derive(Default)]
struct CutsceneState {
    seen: Vec<String>,
    missing: Vec<(String, String)>,
    pending: Option<String>,
    /// game time a cutscene last ran (or the player was last moved)
    last_cut: u32,
    /// marker trips made; whether a cutscene ran after the last one
    trips: u32,
    cut_after_trip: bool,
}
static CUTSCENES: std::sync::Mutex<CutsceneState> =
    std::sync::Mutex::new(CutsceneState { seen: Vec::new(), missing: Vec::new(), pending: None, last_cut: 0, trips: 0, cut_after_trip: false });

fn write_png(path: &PathBuf, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(rgba).map_err(|e| e.to_string())
}

fn read_png(path: &PathBuf) -> Result<(u32, u32, Vec<u8>), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(format!("expected 8-bit RGBA, got {:?} {:?}", info.color_type, info.bit_depth));
    }
    buf.truncate(info.buffer_size());
    Ok((info.width, info.height, buf))
}

/// Every .txd file under `dir` and every TXD inside its .img archives, through the game's reader
/// (revc_test_read_txd). Returns (TXDs read, textures, TXDs that failed).
fn check_txd_dir(dir: &std::path::Path) -> (usize, usize, usize) {
    let (mut files, mut textures, mut failed) = (0, 0, 0);
    let mut read = |data: &[u8], what: &str| {
        let n = unsafe { revc_test_read_txd(data.as_ptr(), data.len()) };
        if n < 0 {
            log(format!("textures: can't read {what}"));
            failed += 1;
        } else {
            files += 1;
            textures += n as usize;
        }
    };
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut paths: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if p.is_dir() {
                stack.push(p);
            } else if ext == "txd" {
                if let Ok(data) = std::fs::read(&p) {
                    read(&data, &p.to_string_lossy());
                }
            } else if ext == "img" {
                let Ok(arc) = revc_formats::img::Archive::open(&p) else { continue };
                for e in arc.entries.iter().filter(|e| e.extension() == "txd") {
                    if let Ok(data) = arc.read(e) {
                        let what = format!("{}:{}", p.to_string_lossy(), e.name);
                        // the Xbox gta3.img stores its TXDs LZO-compressed
                        if revc_formats::xboximg::is_wrapped(&data) {
                            match revc_formats::xboximg::unwrap(&data) {
                                Ok(d) => read(&d, &what),
                                Err(err) => read(&[], &format!("{what} ({err})")),
                            }
                        } else {
                            read(&data, &what);
                        }
                    }
                }
            }
        }
    }
    (files, textures, failed)
}

/// Every DFF of the game: the entries of models/gta3.img and the loose ones in models/ and
/// models/generic (name as the golden files use it, bytes).
fn all_dffs() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    if let Ok(arc) = revc_formats::img::Archive::open("models/gta3.img") {
        // a few names occur twice in gta3.img (different models): the second is "name#2"
        let mut count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for e in arc.entries.iter().filter(|e| e.extension() == "dff") {
            if let Ok(data) = arc.read(e) {
                let n = count.entry(e.name.to_ascii_lowercase()).or_default();
                *n += 1;
                let name = if *n == 1 { e.name.clone() } else { format!("{}#{n}", e.name) };
                out.push((name, data));
            }
        }
    }
    for dir in ["models", "models/generic", "models/Generic"] {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        let mut paths: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dff")) {
                if let Ok(data) = std::fs::read(&p) {
                    out.push((p.to_string_lossy().into_owned(), data));
                }
            }
        }
    }
    out
}
