# Console styles (plan task 13)

The PS2 and Xbox visual styles must look like the original games on those consoles: world look,
draw distance, UI, menus and their animations, scaled to the output resolution. The references
(emulator captures of the user's own discs, the research notes and measurements) are Rockstar
content and stay outside the repo; this file records what the code does and why.

## Steps

| Step | What | State |
|---|---|---|
| 13a | Reference scenes: the `scenes` test scenario stands the player at the console capture spots (time, weather, follow camera, HUD on) and writes 640x480 frames; `scripts/scene_colours.py` measures them | done |
| 13b | World colours per style (lighting, colour filter, PS2 highlight clipping), checked against the captures with the same measurements | PS2 checked; Xbox lightmaps and gamma open |
| 13c | Trails: console persistence, frame-rate and resolution independent, on in both console styles | done |
| 13d | Style settings: fixed LOD 1.0, Classic camera and movement, 4:3 option, PS2 time cycle from an imported disc | LOD, camera, time cycle done |
| 13e | HUD per style (Xbox safe-area shift and rounder radar), console UI scaled by screen height | done (measured elements) |
| 13f | Button glyph variants per UI style | done |
| 13g | PS2 menu look and animations | done |
| 13h | Xbox menu (Maestro) look and animations, controller page | done: layout per the Xbox's data, save screen and dialogs, controller page; Xbox flying controls and vehicle look |
| 13i | Console data from an imported disc (Xbox maps and flames, PS2 animations, PS2 world textures as a style pack), texture packs from zips | zip packs and per-style packs done |

## Styles need their disc

The PS2 and Xbox styles (the Visual Style presets and the UI style option) are offered only when
that console's disc is imported (Options > CONSOLE DISCS): the pickers skip them otherwise, and a
saved console style whose import is gone falls back to PC at start (MenuScreensCustom.cpp
UiStyleAvailable / ConsoleStyleCheck; test runs keep any style). An import makes them selectable
again. What each style takes from its disc:

| Style | From the disc |
|---|---|
| Xbox | menu sprites, fonts, the menu's animation data (bar outlines), the controller page's pad and button sprites, the logo and glints, radio station logos, the map, flames, vehicle lightmaps |
| PS2 | menu textures and fonts, the controller picture, the time cycle, animations (ped.ifp), world textures as a style pack |

## 13a: reference scenes

`REVC_TEST=scenes` (`scripts/test.sh scenes`, not in the default set) runs the scenes in
`rust/revc/src/testdrive.rs` (`SCENES`): the middle of Ocean Drive in front of the Moonlite Hotel facing
north (noon, night, rain, fog, dawn, east at 07:00) and the Ocean Beach lighthouse. No peds or
traffic, so runs repeat. Frames go to `rust/target/test-output/scenes/`. `REVC_TEST_SCENES=S1,S4`
picks scenes; `REVC_TEST_SCENE=x,y,heading,hour,minute,weather[;...]` adds others (finding a spot).

`tests/config/scenes.ini` is 640x480 with the PS2 filter and the **Classic** control method. With the
PC default (Standard, the mouse camera) the follow camera pitches down: the horizon sits at 31 % of
the frame height instead of 47 % as on both consoles. So the console styles need the Classic camera (13d).

First measurement (S1, noon, region means; `scene_colours.py`):

| | sky RGB | road RGB, V | white wall RGB |
|---|---|---|---|
| PS2 capture | 209 242 244 | 82 60 46, 0.32 | 220 221 214 |
| Xbox capture (PC textures) | 146 199 206 | 104 81 59, 0.41 | |
| reMiami, no filter | 171 208 233 | 27 33 28, 0.13 | 161 171 181 |
| reMiami, PS2 filter | 249 250 205 | 45 41 26, 0.20 | 212 213 159 |

Those reMiami numbers were wrong for a test reason: the save starts inside the hotel, and the
teleport kept the interior's time-cycle colours (the "extra colour" rows: ambient 32 instead of
70 outside), so the street was lit like a room. `revc_test_scene` and `revc_test_goto_spot` now
stop the extra colour as leaving through a door does (the `spots` references were re-made).

## 13d: style settings

- Draw distance: the PS2 and Xbox presets set the LOD scale to 1.0, as both consoles use with no
  slider (PS2 `m_PrefsLOD`, Xbox XBE `ms_lodDistScale` at 0x2c34b8); the PC preset sets the PC
  release's default 1.2. Moving the slider afterwards shows the style as Custom, like the other options.

## 13f: UI style and button glyphs

Graphics menu **UI STYLE** (`[Graphics] UiStyle`: 0 PC, 1 PS2, 2 Xbox; the visual style presets set
it, Mobile leaves it). Prompts keep the glyphs of the pad in use (any controller in any style); the UI
style only picks the variant of that pad's set (`rust/revc/src/host/glyphs.rs`, `styled_file`):

| UI style | Face buttons | D-pad | Why |
|---|---|---|---|
| Xbox | `*_button_color_*` (filled, coloured) | `*_dpad_round_*` | the Xbox menus' buttons are filled coloured balls, its D-pad sprite is round |
| PS2 | `*_button_*_outline` | `*_dpad_*_outline` | the PS2 prints button symbols as plain white glyphs in its font |
| PC | plain | plain | unchanged |

A set without the variant (Steam Deck has no coloured buttons) uses the plain glyph.

## 13e: HUD per UI style

Measured on the console stills (research notes, `CONSOLE_LOOK_SPEC.md` 4b, local only):

| | PC (reMiami default) | PS2 UI style | Xbox UI style |
|---|---|---|---|
| Vertical layout | the user's ProperScaling | PC constants in 448 lines (`CDraw::ScaleY`) | the same pixel constants in 480 lines |
| Money, weapon, health, armour, wanted, clock | PC positions | PC positions | moved 22 px left and 22 px down (`HUD_FROM_RIGHT`/`HUD_Y` in Hud.cpp) |
| Radar | 82x82 (FixRadar) or 94x76 | 94x76 at left 40, bottom 40 | 102x94 at left 55, bottom 42 |
| Wide screens | stretched to the edges | Graphics menu HUD AREA: CORNERS (default: the console layout anchored to the screen's edges) or 4:3 (the console's 4:3 area scaled by the screen height, centred) | same |

The user (2026-11-11): the HUD doesn't need the 4:3 area in this port; corners by default, 4:3 kept
as an option. The 4:3 area: `ConsoleHudBegin`/`ConsoleHudEnd` in `core/main.cpp` give the HUD code a 4:3 screen
(width = height x 4/3, aspect 4:3) and the device moves every 2D draw to the centre
(`revc_device_set_im2d_offset`). Text queued before (pickup texts placed in the 3D view) is drawn
first. Not measured yet, so left at the PC positions: zone and vehicle names, subtitles, help and
mission texts, timers and counters, the pager. Checking them needs console captures with those on screen.

## 13b: world colours

With outdoor lighting, reMiami's PS2 filter (re3's colourfilterVC emulation, `ColourFilter=2`) is
already close to the PS2 capture (S1/S2 region means, `scene_colours.py`):

| | PS2 capture | reMiami PS2 filter | Xbox capture |
|---|---|---|---|
| Noon road | 82 60 46 | 83 68 51 | 104 81 59 |
| Noon white wall | 220 221 214 | 222 223 220 | |
| Noon sky | 209 242 244 | 249 252 244 | 146 199 206 |
| Night road | 68 46 34 | 80 54 39 | 93 58 47 |
| Night building | 72 79 69 | 79 79 63 | 71 73 55 |
| Night sky | 51 42 37 | 59 43 38 | 52 40 33 |

Open: the noon sky is too red; night is about 15 % bright.

How the Xbox draws it (reverse-engineered from the Xbox executable; local notes
`xbox-research/XBOX_COLOUR_RE.md`):
- Lighting as on PC: buildings get prelight + ambient (Amb_bl with trails, Amb without), no
  directional, no x2; peds, cars and objects get Amb_Obj(_bl) + directional.
- Every world material has a per-model lightmap (`name@` textures in the Xbox `generic.txd`),
  combined as `V0*T0*(1 + LM*(T1-1))` with LM from `neo/worldTweakingTable.dat`. This is re3's
  Neo world pipeline (`neoWorldVC`), i.e. reMiami's World Lightmaps option with the Xbox's own data.
- The colour filter is one pass over the previous displayed frame (feedback):
  `out = (1-k)*dst + col*T1 + (1+2k)*col*T0`, k = 30/255, col = BlurRGB/255, T1 the previous
  frame at the pixel, T0 one row up (PC: three passes with a +2,+2 px offset). Trails off draws
  nothing.
- A hardware gamma ramp: at the default brightness the output is x^0.909.

## 13c: trails

Both consoles default to trails on, and both draw them as the PC does with its Trails option (the
Xbox in one shader pass with the same weights, see 13b): the previous displayed frame, shifted,
blended in with the time cycle's blur colour, feeding back. The per-channel persistence is about
(2 + 2k) x BlurRGB/255 (noon: R 0.49, G 0.32, B 0.14), which matches the 0.25-0.32 measured on the
console videos. reMiami's Motion Blur option draws exactly this (`CPostFX::RenderOverlayBlur`); for
the console look:

- The PS2 and Xbox presets turn it on (`MotionBlur`).
- The previous frame is kept once per game step (30 Hz, `CTimer::GetFrameCounter`), as on the
  consoles: frames drawn in between blend with the same image, so trail length and colour are the
  same at 30, 60 or 144 fps.
- The shift is 2 px at 448 lines, scaled with the screen height (was 2 px at any resolution and
  only with the debug menu's Blur switch).

## 13g/13h: console menus (approach, agreed)

re3's PS2 menu (`core/Frontend_PS2.cpp`, `PS2_MENU`) is a separate 3,000-line `CMenuManager`
chosen at compile time: it can't be switched at runtime and lacks the PC/revc options (graphics,
controls, button prompts, console discs). The Xbox menu (Maestro `R3_Menu.anm`) has no code in
re3 at all.

Proposal: a **menu skin** over the existing menu logic. Pages, entries and actions stay the PC
menu's (so every reMiami option stays reachable); with the PS2 or Xbox UI style the drawing is
replaced:
- PS2: background, page frames, tabs and highlight from the PS2's own sprites (`FRONTEN2.TXD`,
  imported from the player's disc; `ps2-ui-remake` upscales), layout from `Frontend_PS2.cpp`'s
  draw code, timings from the measured PS2 animations (open fade 234 ms, page cross-fade 167 ms,
  highlight slide 133-200 ms, radio logo fade 367 ms).
- Xbox: panels, icon bar, selection bar and dialogs as in `R3_Menu.anm` (decoded keyframes:
  0.2 s linear, nested clips; item pulse 0.8 s; logo glint 3.8 s), sprites from the imported
  `neo/UserInterface` files; the controller page draws the current pad in the Xbox style with
  callout lines.
- Without an imported disc, the skin falls back to the PC menu.

Steps: (1) the skin hook in `CMenuManager::DrawFrontEnd` and an animation clock; (2) PS2 skin;
(3) Xbox skin; (4) captures of each page vs the console references.

## Decisions

- **Camera (user 2026-11-11):** the console styles use the console camera. The PS2 and Xbox presets
  set the Classic control method (level follow camera, Tommy turns toward the stick, as on the
  consoles); the PC preset sets Standard (the PC's mouse camera). Changing the control method
  afterwards shows the style as Custom.
- **Menus (user 2026-11-11):** a skin over the PC menu is fine as long as it feels right.

## 13g: PS2 menu skin (in progress)

`core/MenuSkinPS2.cpp`, drawn from `CMenuManager::DrawBackground` with UI STYLE = PS2 during a
game, on the pages the PS2 pause menu has (map, brief, load, stats, controls, audio, display or
graphics, language). Done: the two rows of page tabs with the current one on the slanted green box,
the green help box (Select / Back / Resume / Navigate with the pad's glyphs), and the list shortened
to 7 rows so it stays above them. Measured on the PS2 Display capture (positions in capture pixels
in the file). `REVC_TEST_MENU_SKIN=1` shows it before a game is loaded (boot screenshots).

Also done: options with a value are drawn as "LABEL : VALUE" lined up on the colon (label ends at
269, colon at 282, value from 302 in 640 wide, as measured); L1/R1 step through the tabs; the page
cross-fade takes 167 ms (measured on the PS2) instead of the PC's ~390 ms; the D-pad and Select/Start
glyphs now load from the game's button TXDs too (they were never set, so they were missing without
a glyph pack).

Then: sliders start right after their label (x 311) with the label right-aligned before them,
other labels without a value are centred, long labels no longer wrap.

Finished (2026-11-11):
- Help box: always on the background panel's bottom-right corner, covering and overhanging it as on
  the PS2 (the user noted this is deliberate); placed relative to the panel corner of each page, so
  it follows the panel's shape per page and on wide screens. It has the PS2's thick black outline
  (about 8 capture pixels) and black text on the green. All four rows use the pad's outline-only glyphs
  tinted black, for one style (user 2026-11-11): Select, Back, Start ("<glyph> button") and the D-pad
  up, down, right, left for Navigate. The PS2 UI style now also takes the outline variant of
  Select/Start. Without the glyph pack: the game's own button textures untinted, "START button" as
  text and the PS2's black arrows. The rows are centred in the box as one block around a common
  dash column (widths measured at run time, so other glyphs and languages stay centred), and
  vertically on the box's centre. (Glyphs: see below.)
- Audio page: the radio picker sits lower left as on the PS2 (moved with the 2D offset, hover areas
  moved with it); the PC's helper line ("LEFT, RIGHT ... TO ADJUST") is hidden under the skin.
- Fades and highlight (measured): opening the menu 234 ms, a page change 167 ms, the highlight
  slide 167 ms (PS2 133-200 ms); the PC keeps ~390 ms and ~100 ms.
- Nothing is drawn in the tab area (the Map page's BACK; the triangle button goes back).
- Tab names MAP, BRIEF, STATS from the game's own texts; LOAD, CONTROLS, AUDIO, DISPLAY, LANGUAGE
  built-in English (the PC text has no short names for those).
- All eight tab pages checked in game (`REVC_TEST_MENU_INGAME=1 REVC_TEST_MENU_PAGE=n` with the
  saveload scenario opens the pause menu on that page).
- Kept from the PC menu on purpose: row spacing (25 px at 448 lines vs the PS2's ~30, so 7 rows fit
  above the tabs) and the PC page contents (reMiami's options), which the PS2 menu doesn't have.

## 13h: Xbox menu skin (in progress)

`core/MenuSkinXbox.cpp`, with UI STYLE = Xbox during a game, on the seven pages the Xbox pause menu
has (load, map, brief, stats, controls, audio, display/graphics), when the Xbox menu sprites are
imported from the player's disc (`neo/UserInterface`); otherwise the PC look stays. Measured on
native 640x480 xemu captures:
- Page title top left (52, 48), the Xbox's Vice City logo (`r3_menu_228`) top right.
- Items: labels at x 139, values at x 331 (pushed right after a longer label), rows 44.5 px apart
  from y 150, 5 rows (3 on Audio, whose radio picker sits under them at y 283); no scrollbar.
  The menu's own y units are 448 lines, so the row positions are converted.
- Mint selection bar (0, 210, 141) behind the current label with the Xbox's pulse: a white glow
  growing 0.9 to 1.157 x 1.081 while fading 0.5 to 0.059, every 0.8 s (R3_Menu.anm).
- Icon bar: disk, compass, tape, computer, pad, boombox, TV (`r3_menu_238` ... `255`) at the
  measured x, the current page's icon enlarged on a mint box.
- Help line in pink under it with the pad's glyphs (Xbox UI style: coloured): Start resume game,
  left/right select, A select, B back. BACK entries are hidden (B goes back); LT/RT switch pages.
- Sprites go through the texture mods: a loose file or pack entry `textures/ui/r3_menu_241.png`
  replaces the imported `ui/r3_menu_241.dds` (the 4x remake pack works this way); the controller
  page's picture too. `cargo run -p revc-import --example import -- <disc> <dir>` imports a disc
  outside the game (with `REVC_IMPORT_DIR=<dir>` for tests).

- Logo glint: a white flash over "city" (`r3_menu_230`) then "Vice" (`232`), 200 ms each from
  3.4 s after a page starts, every 3.8 s (anim 4); placed from the menu video, slightly fainter than
  the Xbox's. `REVC_TEST_GLINT=1|2` shows a stage for screenshots.
- Panel: per page the Xbox's panel corners (estimated from the extreme points of the camo area in
  each page's capture; bottom edge under the icon bar at y 378), moved on a page change by the PC
  menu's ~380 ms corner animation (Xbox ~400 ms). The exact values are in R3_Menu.anm anim 0 (45
  states), not decoded; not yet compared at 640x480 (the in-game menu capture runs at 1280x720).

- Dialogs (new game, load, delete confirmations): the Load page dimmed behind, the Xbox's layered
  blue dialog (steel-blue layer, dark edge, light-blue panel as measured), the question centred
  in pink capitals, the two choices side by side with the mint box; the Load page's title stays.
  The PC menu's own choice texts (NO/YES) are kept.

- Load (and delete/save) slot lists: the Xbox's light-blue, slightly tilted list box, eight rows 35 px
  apart, each slot name in black in the Xbox menu font with its date under it, the mint bar across
  the selected one (all measured).

- Controller picture page (Controls): keeps its own pad drawing and callouts with the Xbox title,
  logo and icon bar; the layout name moves under the title, the SHOW/context row sits between the
  pad and the icon bar on the mint bar, BACK is hidden.

- Controller page contexts (user 2026-11-11): ON FOOT / IN CAR / FLYING like the Xbox's Walking /
  Driving / Flying. Flying shows what the Xbox page lists: steer, accelerate, decelerate (brake),
  fire, sub-missions, radio, camera, enter/exit. The Xbox's separate "Fire missiles" has no
  action of its own in the controls found so far: not shown.

- Xbox menu font: the help line and the controller page's callout labels use the Xbox's
  RWSFArborcrestHeavy (imported texture + METRICS1 boxes, drawn by `XboxFontPrint`); callouts in
  the Xbox's green with teal lines (measured). Titles and items stay in the game's heading font,
  which is what the Xbox shows there too.

- Stats: the Xbox's box (92..540 x 118..295) and rating positions, the same scrolling lines and
  standard font in the Xbox's columns (110 / right at 527). The Xbox's Unlocked Items tab is not
  shown: the PC game keeps no unlocked-items list.
- Panel shapes checked at 640x480 (`REVC_TEST_WINDOW=640x480` with a 640x480 config): within a few
  pixels of the captures (Display: reMiami 84,84 / 578,366 vs Xbox 84,84 / 580,378).

Still to do: the Xbox's "Fire missiles" callout, every page checked in play, the comparison shot set
(research request: `REVC_TEST=menus`, every page per style, see below).

## Console text (R4)

The import lifts each disc's `american.gxt` (`ps2/text`, `xbox/text`); `rust/revc/src/text.rs`
(`revc_text_console_get`, `console_source`) answers some keys from it in that UI style, with the
menu language English:

- **Xbox:** the pad names (`C_…`) and the skin's own keys that have an exact Xbox string
  (`RVC_XSF` = `FEA_SFX`, `RVC_XOK` = `FEM_OK`, `RVC_XNG` = `T_0914C`).
- **PS2:** the Ammu-Nation items, which the PS2 shows in arrows ("< Hammer >": `ARMOUR`, `BASEBAT`,
  `CHAINSA`, `CLEVER`, `GRENADE`, `HAMMER`), and the stunt stats line with its colon (`BSTSTU`).

- **Xbox gameplay prompts:** a help line takes the Xbox's shorter wording when its action codes
  stand for the same presses as the PC line's (`rust/revc/src/prompts.rs`). Each Xbox action
  code is replaced by the PC action it stands for, so the line still shows the player's own
  binding; on an Xbox-type pad that binding is named in the Xbox's own button words (its `C_…`
  text; LB and RB, which the Xbox pad doesn't have, keep their usual names), on other pads and
  the keyboard as usual. A key qualifies by one of three rules: every code pairs with a listed
  synonym in the same order (53 keys), its one stick code stands for the PC's pair of directions
  (part of those 53), or it was reviewed by hand and is on the allowlist (50 keys: the property
  and weapon pickup prompts, the shop exit, Ammu-Nation buy and exit). 103 keys in all; a test on
  the imported text pins that set, so a table change can't widen it unreviewed.

  Known places where the Xbox style keeps the PC sentence, on purpose:
  - `HELP37`, `HELP60` (car-jacking without getting in): the Xbox does it with the stick, the PC
    with the sprint key; the Xbox sentence would point at the wrong control.
  - `HELP50`-`HELP52`: the Xbox names its camera control there, the PC line the phone key.
  - `HELP58`, `HELP59` (cycling targets): the two lines name right and left in opposite orders,
    so pairing codes by position would swap them.
  - `BOLLOX` (RC plane): the codes don't mean the same controls.
  - `HELP15`, `HELP53`, `PLANE_H`: the lines name a different number of controls.
  - Lines whose PC text names no control, and the mission and Xbox-only keys (not in the PC's
    main table): later, if at all.

  The Xbox prints the target and look-behind prompts with no button name (its action table has
  no entry for those two codes); reMiami prints the bound key instead.

Left out on purpose: the PS2's button prompts (they name the PS2's default buttons and would be
wrong once controls are remapped), the memory card and bonus-disc texts, and the menu labels (the skins draw their own, checked against
the captures: the Xbox titles and rows in the heading font's lowercase, the PS2's "subtitles :
off" rows).

## PS2 cutscenes (R4)

The PS2's `ANIM/CUTS.IMG` has the PC's 148 entries. Most that differ do so only in padding;
seven differ in substance and the PS2 style plays the PS2's versions (imported into
`ps2/cuts/`, read by `animation/CutsceneMgr.cpp` in place of the PC's entry):

- animations: `int_d` (the dealer re-keyed), `law_4` (the door: fewer keys, same swing, held
  from 15 s instead of stored to 61 s; never looped by the script, so the cut's length and sound
  are unchanged), `stripa` (finger bones);
- camera paths: `cok_4a`, `fin`, `rok_2`, `tex_3`. Only `tex_3` changes the cut's length: its PS2
  path ends about 3 s later (45 s instead of 42 s) with the same animations, as on the PS2.

Left out: the cuts with characters only the PS2 has (crowd fans in `cub_1`-`cub_3`, a character
in `bud_2`, the player's own animation in `col_1`). The PS2's mission script spawns them; with the
PC's script nothing would. Tied to the console-missions question (roadmap R4 item 6).

Check: `REVC_TEST_CUT_LOAD=cut,cut…` (any scenario that visits scenes) loads each cut's data and
logs its camera path end and animation lengths; the log says which parts came from the PS2.

## Map data per style (R4)

In the PS2 and Xbox styles, while the style draws the console's own world models
(`Ps2WorldModels` / `XboxWorldModels`, default on), the level loads the console's map definitions
and placements instead of the PC's (`rust/revc/src/mapstyle.rs`, `mapstyle_patch.rs`;
`core/FileLoader.cpp`, `collision/ColStore.cpp`). It applies when the level loads, i.e. at start:
changing the style in the menu shows "map data changes on next start".

- **Objects (IDE)** match the PC's by model name and keep the PC's ids (the mission script names
  objects by id): draw distances, flags and texture dictionaries come from the console. Objects only
  the console has get free ids; their converted models (and empty dictionaries the console's texture
  pack fills, for three PS2 dictionaries the PC lacks) come in a small extra image written next to
  the import. `default.ide` (weapons, wheels, the Xbox's hand model entry) stays the PC's: it isn't
  map data, so the Xbox's ten longer wheel draw distances aren't used.
- **Placements (IPL)** match by model name and position rounded to a metre. A matched placement takes
  the console's line (interior, exact position, rotation) with the PC's id; a PC placement the
  console doesn't have is still created (the pools keep their order, which saves refer to) but
  hidden and without collision; the console's extra placements load after all of the PC's. A moved
  object counts as one removed and one added.
- **Collision follows what the player sees** (the user's rule). The console's collision is taken
  only where the console's drawn model changed with it, each checked to fit the drawn console mesh:
  9 PS2 models (the mansion grounds `man_grnd5`, the Little Havana pieces and a Little Haiti house, the mansion hall's
  reflection, which has none on the PS2, the hotel bush, the building site, the Ocean Drive
  chariot) and 1 Xbox model (`deco_polbuild`); plus the PS2's collision for three objects it placed
  differently, and the new objects' own collision. Everything else keeps the PC's collision, also
  where the console's differs: about 64 PS2 and 28 Xbox models whose drawn mesh is the PC's, among
  them every Xbox road and ground piece, the North Beach promenade fence (its Xbox hull reaches
  73 m past the fence), an airport wall and an Ocean Drive alley. Never taken from a console:
  vehicles, script props, gates and roadblocks (`generic.col`, `vehicles.col`), and collision
  entries without an object.

Counts (tests pin them, `mapstyle.rs` `real_data_counts`, on the local imports):

| | objects added / removed | texture dict. | draw dist. | flags | placements added / removed | interior changes | collision taken |
|---|---|---|---|---|---|---|---|
| PS2 v1.40 | 19 / 39 (6 of them time objects) | 4 | 21 | 75 | 30 / 50 | 195 | 9 + 3, and 19 new |
| Xbox | 4 / 1 | 0 | 96 | 3 | 17 / 7 | 134 | 1, and 4 new |

The import writes the console's map files under `<import>/<ps2|xbox>/maps/` and `coll/`; the
patched files go to `maps-patched/` there at start. Nothing from the discs is in the repo.

## The PC style with console discs imported

The PC style loads exactly what it loads without imports: every reader of the import folder is
limited to its console's style (models, map data, cutscenes, text, particles, time cycle, ped
animations, texture packs, the Xbox lightmaps). Two exceptions, on purpose, both reMiami's own
pages rather than game data:
- the controller-layout page draws the Xbox pad outline from the import (its only art source)
  and asks for the Xbox disc when it's missing;
- the brief's tutorial-hint and unlocked-item lists read which text keys belong to them from the
  Xbox disc's two list files (`neo/hints.txt`, `neo/unlockeditems.txt`), in every style, the first
  time a mission text is printed. What the lists collect is kept in a sidecar file next to the
  save (`<save>.lists`), never in the save itself, so a PC-style save stays byte-compatible with
  the PC game.

A test guards this: `REVC_TEST_IMPORT_GUARD=1` records every file read from the import folder and
fails a PC-style run that read anything but those three files (`rust/revc/src/discimport.rs`
note_access, `testdrive.rs`).

## Behaviour per style

Most of what the styles change is looks. The PS2 style also carries one behaviour difference (a
SilentPatch VC fix the user wanted as on the PS2 only), decided by the style at the moment it
happens, no restart: tear gas hurts Tommy and mission characters (#68; `core/World.cpp`
SetPedsChoking). The PC and Xbox styles keep the PC's behaviour. Map data per style also changes
what is where (above), but it takes effect at the next start.

## Xbox cutscene hands (R4, in progress)

In the Xbox style, cutscene characters get the Xbox's separate hand models in place of their own
hands, with the colour, size, position and finger pose the Xbox's `data/cutscenehands.xml` gives
per cutscene and character (read from the import; `extras/CutsceneHands.cpp`).

Open point: whenever a hand is shown, the Xbox also shrinks the character's bones 54-63, which are
the toes in many of its cutscene models. reMiami leaves that out until a capture of an Xbox
cutscene with a character's feet in view shows what the console really does (PLAYTEST). A model
without the hand bone gets no replacement hand (the Xbox's would drift).

Checked against the console (an Xbox recording of int_b, the hotel phone call, 2026-11-14): the
Xbox's hand doesn't enclose the handset either; it rests against it with the fingers curled over
the top, as in reMiami, and the left hand is posed too.

## Xbox models with native skins

The Xbox's cutscene character models (all 74 `cs*.dff` on the disc, the cutscene hands among
them) store their skin in the Xbox's own layout, not the PC's. The model reader converts it when
it finds that layout and reads every other model exactly as before (`is_xbox_native` and
`xbox_native_skin` in `rust/revc-formats/src/xboxskin.rs`, called from `rust/revc/src/dffload.rs`).
A skin it doesn't recognise is refused with the reason in the log, and the model isn't loaded, as
with any other bad file. Only the cutscene hands use these models today. The import copies the
files unchanged; nothing is converted at import.

Later: the import has no format version (`IMPORTED.txt` records only the disc image and the file
count), so the first change to what an import writes will need one, to tell an old import from a
current one.

## PS2 radio and cutscene audio (R5b)

In the PS2 style the radio stations, the ambience tracks and the cutscene speech play from the
player's PS2 import (`ps2/audio/music/*.vb`, `ps2/audio/cutscene/<group>/*.vb`, R5a) instead of
the PC's ADF/MP3 files: reVC's PS2 stream table and VB decoder (`PS2_AUDIO_PATHS`, `CVbFile`:
headerless PS-ADPCM, stereo, 0x2000 bytes per channel per block) with the paths resolved into the
import (`audio/oal/stream.cpp` Ps2StreamPath). Rates: 32 kHz, and 16 kHz for the two talk stations
and the police scanner (measured: their lengths match the PC files at those rates). A stream the
import lacks falls back to the PC's. The stations' lengths differ from the PC's (Flash FM about
3 minutes longer), so the radio positions use the PS2 lengths in the PS2 style, measured the first
time that style plays (`sampman_oal.cpp`), and wrap when the style changes. Other styles never read
the PS2 audio. The PS2 sound effects (R5c) are a separate item.

## Xbox fire (R4)

The Xbox's flames burn white-hot and are animated; the PC and PS2 use one orange flame texture.

- **Values:** the import lifts the Xbox's `data/particle.cfg`. In the Xbox style its values replace
  the PC's for the particle types both games have, matched by name (`merge_by_name` in
  `rust/revc/src/particlecfg.rs`; the file's line order differs, so a whole-file load would shift
  every later type). In practice that is the flame, fireball, car flame and car-fire smoke. The
  PC and PS2 styles keep the PC table unchanged (the PS2's file is the PC's). That PS2 = PC rests
  on the identical file; the look was only compared against PS2 stills from an emulator
  savestate that later proved unreliable (a partly loaded world), so it is not verified on the
  PS2 (PLAYTEST).
- **Animation speed:** the Xbox's file gives the flames a speed of 0.5, a fraction the PC's integer
  column can't hold. The Xbox steps such a flame one frame every second update; reMiami stores
  speed 0 plus a flag (`ANIM_EVERY_OTHER`) for that. Flames and car flames play their first 30
  frames once and hold the last, fireballs frames 30-43.
- **Frames:** the import lifts the Xbox's `models/particle.txd`; its 45 flame frames go into one
  array that the flame, car flame and fireball draw from in the Xbox style. A missing frame
  stays the PC's single flame, and without the Xbox textures the frame columns fall back to the
  single flame, so nothing reads past a raster.
- **Style switch:** `CParticle::ApplyStyle` reloads the PC values, applies the style's, and points
  the flame types at the frames; it runs at start and when the UI style changes.
- **Test:** `REVC_TEST_FIRE=car|explosion` in the scenes scenario (a burning car or an explosion
  12 m in front of the player, `REVC_TEST_FIRE_MS` before the shot for the explosion).
- **Not done:** the Xbox's extra flame for moving burning cars (a particle type the PC game doesn't
  have); a later item.

## 13i: texture packs per UI style

`mods/packs/pc-*.zip`, `ps2-*.zip`, `xbox-*.zip` apply only in that UI style; other packs in every
style. Order: loose `mods/textures`, then the style's packs, then the general packs (each in name
order). One texture index per style (`texmods.rs`); when the UI style changes, `HostUpdateButtonGlyphs`
reloads the fonts and `hud.txd` (`CHud::ReloadTextures`, radar sprites included). Imported console
art (`ui/r3_menu_*`) is looked up in the same index.

PS2 disc textures (roadmap R2): the game can't read PS2-native TXDs (GS-swizzled, palettised), so a
PS2 import decodes them once (`rust/revc-import/src/ps2tex.rs`, a port of the research reader,
pixel-identical to it on the disc's FRONTEN2.TXD) into `<import cache>/ps2/packs/ps2-native.zip`
(`rust/revc/src/ps2pack.rs`): the HUD (`hud`), the menu art (`frontend1`, `frontend2`) and the
loading, splash and intro screens. texmods also reads the packs in the import cache, after the
player's own packs and under the same style rule, so the PS2 style uses the disc's art (the PS2
controller picture included). The fonts stay out: their glyph sheets are laid out differently.
An import made before this builds the pack the first time the textures are indexed.
Xbox disc textures (roadmap R3) work the same way: `<import cache>/xbox/packs/xbox-native.zip`
from the Xbox HUD, fonts (the PC's layout in DXT5; its [ ] slots hold the Xbox's arrow icons),
wheels and the loading, splash and intro screens, decoded with revc-formats' Xbox texture reader.
With these packs alone the Xbox menu shots score as with the full hand-made comparison pack.

PS2 world textures (roadmap R3, "exact PS2 textures"): a PS2 import also writes every TXD of the
split GTA3.IMG to `<import cache>/ps2/world/` (the `generic` slot from the standalone
MODELS/GENERIC.TXD, which the game loads instead of the archive's older copy) and decodes them into
`<import cache>/ps2/packs-world/ps2-world.zip` (about 12,000 textures, 130 MB). texmods uses it in
the PS2 style while `[Graphics] Ps2WorldTextures` is on (default; read when the settings load),
after the player's own mods and packs. It has an index of its own, consulted per texture, because
the main index is built at the first texture, before the settings load.

PS2 models (roadmap R4, "exact PS2 models"): a PS2 import also converts the disc's models from
GTA3.IMG (PS2 native geometry: VIF unpacks of position, UV, prelit colour and normal in strip
batches) to ordinary geometry (`rust/revc-import/src/ps2geom.rs`, after librw's PS2 uninstance)
into `<import cache>/ps2/models-world/` (4,326 of 4,584; skinned peds and vehicles use broken-out
attributes and stay PC models). In the PS2 style, while `[Graphics] Ps2WorldModels` is on
(default), CStreaming::ConvertBufferToObject loads those instead of the PC ones (map and cutscene
objects; not vehicles or peds): same geometry, the PS2's own baked vertex lighting. Textures the
PS2 models use that a PC dictionary lacks (e.g. `hot_bottles1`) are added from the PS2 world pack
when the dictionary loads.

Xbox models (roadmap R4, "exact Xbox models"): an Xbox import converts the disc's models from
gta3.img (Xbox native geometry, RenderWare 3.5: index buffers and one vertex buffer of position,
[packed normal], [colour B,G,R,A] and two coordinate sets) to ordinary geometry
(`rust/revc-import/src/xboxgeom.rs`, after librw's Xbox uninstance) into
`<import cache>/xbox/models-world/` (4,613 models). In the Xbox style, while
`[Graphics] XboxWorldModels` is on (default), they load instead of the PC ones, and
XboxLightmaps.cpp gives their dual passes the lightmaps (their second set is the lightmap's
already). The Xbox rebuilt and re-lit some map models (e.g. the shelf in Sonny's office, moved
2.8 cm, int_b), so the PC models with the lightmaps grafted on can't look the same.

Some Xbox models use textures their PC dictionary lacks (138 names in 57 dictionaries, used by 289
models; e.g. the dock props in the opening's int_d). The Xbox import also keeps the Xbox's world
dictionaries (`<import cache>/xbox/world/`, about 190 MB), and `rust/revc/src/xboxworld.rs` builds
from them, once, `<import cache>/xbox/packs-world/xbox-world.zip` with exactly those textures
(1.1 MB), from the Xbox's dictionary of the same name or its shared generic.txd. In the Xbox style,
while `XboxWorldModels` is on, texmods adds them to the PC dictionary when it loads; nothing of the
PC's is replaced, and the PC style never reads them. A name neither has stays missing (logged; the
cutscene test fails on it). The count is pinned in the module's test (local data).

Per-style tests: `scripts/test.sh` runs everything in the PC style. `scripts/styles.sh` runs the
scenarios that differ per style in all three (saveload and savegame against each style's own local
references, the new game's opening, and the PC-style import guard); it needs the console imports.

## Menu comparison shots

`scripts/menu-shots.sh [out dir] [ps2|xbox ...]` runs `REVC_TEST=menus` (`rust/revc/src/testdrive.rs`,
`MENU_SHOTS_PS2` / `MENU_SHOTS_XBOX`): a loaded game, the pause menu open, then each state (page,
selected row, controller context, pad layout, map legend) captured one second after it is set,
as `<set>_<id>.png` at the console size (PS2 640x448, shot at 640x480 and scaled since the game
has no 448-line mode; Xbox 640x480) and `<set>_<id>_hi.png` at 1440x1080. The ids follow the
research session's comparison list (`menu-compare/REQUEST.md`, local); states reMiami doesn't have
are left out: the PS2's four controller setups (reMiami has Xbox Standard/Classic/Original PC
layouts), DTS, widescreen and screen-position rows, the safehouse save flow, the Xbox tape-deck
help and the controls-overwrite dialog. The Xbox set needs `REVC_IMPORT_DIR` (an import of the
player's disc). Test hooks: `revc_test_menu_state`, `revc_test_menu_page_id`; `REVC_TEST_CONFIG`
and `REVC_TEST_WINDOW` in `scripts/test.sh`.

Goal (research session, from the user): the console menus at high resolution without changing
their presentation (`menu-compare/HIRES_PRESENTATION.md`, local): mipmaps and linear-mip filtering
for upscaled UI and font replacements (the HUD and fonts are drawn NEAREST and had no mips),
rectangle edges and thin lines snapped to output pixels, every console offset scaled, optional
30 Hz pacing of menu animations and optional UI softness. `compare.py` checks that the 1440x1080
shot scaled down matches the native one.

## Menu fix list, round 1 (research comparison 2026-11-11)

The research session compared the shots with the consoles (`menu-compare/MENU_FIX_LIST.md`,
local). Done:
- Both: no PC cursor in the console skins; confirm dialogs start on No.
- PS2 frame: logo 21 px right and 11 down, title 20 px left and 22 down of the PC's places; the
  help box at the console's place, about (438,303)-(626,376); the console's panel shape per page
  (640x448 table in `Frontend.cpp`); the tab bar in the EU v3.00 layout (reMiami keeps eight tabs,
  language too) with the active box just around its label; no PC scrollbar.
- PS2 highlight: a box just around the selected label (not the PC's full-width bar).

Round 2:
- Xbox: the highlight is a mint bar tilted with the panel, from the label's left edge (at least 150
  px), with a dark tail; other pages' icons grey (about 55 %); the green card only while the icon
  bar has focus (after LT/RT, until the player moves in the list); the help line per state (list,
  top level, display controls, map); titles "load", "controls", "display controls"; larger title.
- PS2: the confirmations get the PS2 frame and the load tab; back entries hidden with their
  highlight (no double highlights, no "back" on brief/stats/map); the criminal rating reads
  "Citizen [0]"; the load page is titled "load".

Page contents, part 1:
- New pages `MENUPAGE_CONSOLE_DISPLAY` (brightness, trails = the motion blur setting, subtitles,
  wide screen (PS2 only), radar mode, hud mode) and `MENUPAGE_CONSOLE_CONTROLS` (controller layout,
  display controls = the controller picture, invert, vibration); the PS2 tabs and Xbox icons point
  to them.
- The Audio page shows music, sfx and radio station in the console styles (the PC's MP3 boost,
  dynamic acoustics, restore defaults and back are taken out while a console style is shown:
  `ConsoleAudioRows`/`FilterRows` in `MenuScreensCustom.cpp`).
- reMiami's other settings (the PC Display, Graphics, Audio and Controls pages) stay reachable through
  the pause menu's Options entry until the console-style option lists (PS2 options tab, Xbox
  advanced sub-tab) exist.

Part 2 (round 4): the Xbox wedge slider (x 325-485, faint wedge with bright bars to the value)
for brightness and volumes; Xbox labels "music", "sfx", "select controls", "invert up/down"; Xbox
value columns 331 / 378 (radio) / 435 (controls); PS2 Display rows 30 px apart from y 92.

Part 3 (round 5): PS2 Display rows from y 77; Xbox "select controls" without a value, the radio
row's bar across the whole row. PS2 Controls page as on the PS2: configuration, controller display
(on foot / in car switches the picture), vibration, invert as "label : value" from x 160, 20 px
apart; under them the DualShock 2 picture (`fe_controller`, from a PS2 texture pack such as
ps2-ui-remake.zip: the game can't read PS2 TXDs yet) with black callout lines and white labels
following the current bindings (`DrawPadPicturePS2`, core/PadPicture.cpp). `menu-shots.sh`
installs game/mods for the shots (REVC_TEST_MODS).

Part 4 (round 6): the PS2 Audio page in the console's two columns (music/sfx headings over taller
bars at x 191/336, "radio station" with the name under it, label boxes drawn there); the Xbox
whole-row bar without a tail; font widths of the FONT1 "*I" slots (", *, /, ;, =) set to those
characters' widths (they had the console icons' widths: "up/ down").

Next: the Xbox full logo strip, "screen position", credits, the console-style option lists; Xbox confirm dialog (dimmed page, OK/Cancel); stats/brief
sub-tabs; load page layouts; map legend item; save page; the round-3 details (help-line items per
page, the highlight's lighter tail and length). Plan for the extras (research, from the user's "add options without
breaking cohesion", `menu-compare/OPTIONS_PLACEMENT.md`): console pages stay as on the console;
reMiami's extra options go into patterns the consoles used: PS2 an "options" eighth tab (language
becomes a row) with a Stats-style list; Xbox an "advanced" sub-tab on Display, Audio and
Controls; merged settings (motion blur into trails, aspect ratio into wide screen, map legend into
the map's legend item, user music into the station strip); console discs as a third Load entry;
no back/restore rows.
