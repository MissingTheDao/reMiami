# In-person test checklist

Things the automated tests (`scripts/test.sh`) can't judge: they need a person at the machine,
real hardware or a second machine. Tick items off here (or tell Claude the result) and the plan
task in brackets can be marked Done.

The checks are grouped by play session: what can be cleared in one sitting, in the order you
reach it. Start the game with `cargo run` in `rust/` (settings in `game/reVC.ini`). Sessions 3
and 4 need the PS2 or Xbox disc imported (Options > Import).

Two renderers, both on wgpu: **retro-wgpu** (default since 2026-11-14) and the older wgpu back
end (`REVC_RENDERER=wgpu`; it also presents by itself, with a `WARNING` in the log, if retro-wgpu
fails to start). Either runs on `REVC_RENDERER_BACKEND=vulkan` (default) or `gl`. Untested:
Windows and DX12.

## Session 1: menus only (no driving; about 15 min)

From the title screen and the pause menu of a loaded save.

- [ ] Load Game lists your saves with names and dates; a damaged save shows as corrupted. [15]
- [ ] Options > Language: switch through the languages; menus and in-game text change and look
      right (accents in French/German/Spanish/Italian), then switch back. [15]
- [ ] Display Setup: change the screen resolution in the menu (not only at start-up). [7]
- [ ] Display Setup > Colour Filter: each of None, Simple, PS2/XBOX, Mobile, PC changes the look
      at once and is kept after a restart (both renderers). [12]
- [ ] Display Setup > Antialiasing: step through all 9 levels in game (SMAA smooths edges without
      blurring HUD text as much as FXAA); the picture changes at once, edges get smoother the
      higher the level, frame rate drops at SSAA; the level is kept after a restart. [12]
- [ ] Graphics > Visual Style: left/right cycles PC, PS2, XBOX, MOBILE; each changes colour
      filter, PS2 alpha test and (with game/neo) vehicle pipeline, rim light, lightmaps, road
      gloss at once (those rows follow); changing one of them by hand shows CUSTOM; kept after a
      restart. [12]
- [ ] Controls menu, BUTTON PROMPTS: each style shows its glyphs; AUTO follows the pad again. [9]
- [ ] Menus, HUD, text and radar look like the normal build. [12]
- [ ] After `scripts/upscale_hud.py`, at your monitor's resolution (1440p/4K): HUD text, radar,
      menus and the map screen look sharper; no halos around letters, no seams between radar
      tiles, nothing upside down; menu background acceptable (or `--clean` and compare).
      [docs/HUD_UPSCALING.md]

## Session 2: PC style, a drive around Ocean Beach (about 45 min)

On a 120/144 Hz display, frame limiter off (Display Setup), VSync on. Have a pad and the
keyboard at hand; for the texture-mod checks, put the files in place before starting.

Before starting:
- [ ] A PNG (any size) at `game/mods/textures/<txd>/<texture>.png` for a texture you will see
      (list names with `cargo run -p revc-formats --example txdlist -- ../game/models/gta3.img:<model>.txd`),
      and one as DDS (DXT1 or DXT5); also one broken file. [11]

At the start, on foot:
- [ ] The modded textures show, correct way up, not blurry up close, fine in the distance; the
      broken file prints `texmods: ... keeping the original` and the original shows. [11]
- [ ] Walk, run and turn the camera with the mouse: smooth, no judder or "double images"
      compared with 30/60 fps. [8]
- [ ] Peds walk and animate normally (skinning); no stretched limbs; other peds look smooth. [8, 12]
- [ ] Plug in each pad you have (Xbox, DualSense / DualShock, Switch Pro, Steam Deck, Steam
      Controller): detected when plugged in after the game started, and when unplugged /
      replugged. [9]
- [ ] Prompts with buttons (a weapon pickup, tutorial tips) show the glyphs of that pad; a Switch
      pad shows B on the bottom button. Use another pad, or the keyboard then a pad: prompts
      switch to the pad just used. [9]
- [ ] Two pads: the second one doesn't steal player 1's input. [9]

In a car:
- [ ] Drive fast along a straight road and through turns: car, wheels and camera smooth; no
      jumps when the frame rate dips; other cars smooth too; handling feels as before (the
      simulation still runs at 30 Hz). [8]
- [ ] Cars have their shiny reflections (MatFX environment map), on both renderers; nothing
      looks inside out or too bright. [12]
- [ ] Display Setup > Vehicle pipeline NEO, rim light, lightmaps, road gloss on: cars get the
      Xbox-style reflections and specular highlights, peds a rim light, roads a gloss; nothing
      flickers when turning the camera. [12]
- [ ] Rumble: with `Vibration = 1` under `[Controller]` in `game/reVC.ini`, gunfire, explosions
      and crashes vibrate, and it stops afterwards (no menu toggle on Linux yet). [9]

A mission and the safehouse:
- [ ] Start a mission: objectives and subtitles show and go away on time, no "... missing"; the
      mission title and "MISSION PASSED" big messages appear. [15]
- [ ] Pause menu > Brief: lists the last objectives / subtitles, newest first. [15]
- [ ] Pager messages (e.g. when a phone mission becomes available): the text scrolls through the
      pager at the top, restarts after cutscenes, and the next queued message follows. [15]
- [ ] Teleports and respawns (death, Busted, cutscene starts, loading a save): no streak or
      sliding from the old position. [8]
- [ ] Save in a safehouse, then load that slot. [15]

SilentPatch fixes (all styles):
- [ ] Mouse aim: up/down feels as fast as left/right (the vertical sensitivity equals the
      horizontal now; a change for anyone used to the old feel), and the saved sensitivity is kept
      after a restart. Mouse look keeps working while the screen fades in. [SP 16/57/17]
- [ ] With Classic controls (pad), looking towards a shopkeeper doesn't count as aiming at them;
      aiming does. [SP 61]
- [ ] Tommy shakes his fist at stopped traffic with brass knuckles too, and drivers react to
      Vice City's weapons. [SP 64/65]
- [ ] Busted with the free-arrest perk: Tommy keeps his weapons. [SP 104]
- [ ] Muggers on the street rob their victims instead of giving up at once (every style). [SP 71]

At the end:
- [ ] Restart with `REVC_RENDERER=wgpu` and play a few minutes: it looks the same as the
      default. [12]

Known limits, not bugs: shadows, lights and particles still move in 30 Hz steps; ped limbs
animate at 30 Hz (only the body moves smoothly).

## Session 3: PS2 style (about 10 min; PS2 disc imported, menu language English)

- [ ] Ocean Beach Ammu-Nation: the shop lists items in arrows ("< Hammer >", "< Grenades >"). [R4]
- [ ] Stats shows "Best INSANE stunt so far:" with a colon (needs a save with stunts). [R4]
- [ ] Cutscenes with PS2 data (needs saves before those missions; cut names as in
      docs/CONSOLE_STYLES.md): in `law_4` the office door opens and stays open, in `int_d` the
      dealer and in `stripa` the dancer's hands move naturally, `tex_3` holds its last shot about
      3 s longer than on PC; `cok_4a`, `fin`, `rok_2` play their PS2 camera moves. [R4]

- [ ] PS2 style: tear gas (e.g. a thrown canister near Tommy and a mission character) hurts both;
      in the PC or Xbox style the same scene leaves Tommy and mission characters unhurt. [SP 68]
- [ ] Map data (start the game in the PS2 style): walk or drive past the spots whose collision
      follows the PS2's rebuilt models (map coordinates): the mansion grounds and hall
      (-380, -570; stand where the floor reflects, jump on the banister), Little Havana's car
      showroom and two pieces near it (-990, -850 / -900, -575 / -1130, -450), a house in
      Little Haiti (-1105, 346), the
      hotel garden bush (219, -1284), the building site (305, -247), the chariot sculpture
      (569, -5). Nothing to walk through or get stuck on. The bank, the mansion pool and the mall
      show the PS2's extra floors, water and tree shadows. [R4]

## Session 4: Xbox style (about 30 min; Xbox disc imported, menu language English)

With an Xbox-type pad first, then the keyboard, then a PlayStation pad.

On foot near the start:
- [ ] Help lines use the short Xbox wording with button words: entering a car, the horn,
      steering, a weapon pickup. Remap a control in the Controls menu and the line follows
      it. [R4]
- [ ] Same with the keyboard: the lines print the bound keys. With a PlayStation pad: its usual
      button names. In another menu language: the PC lines. [R4]

Around Ocean Beach:
- [ ] A property for sale and the Ammu-Nation counter show the Xbox wording too. [R4]
- [ ] Burning cars and explosions show white-hot, animated flames; switching to the PC style in
      the menu brings back the orange ones. [R4]
- [ ] The fireball particle (frames 30-43 of the Xbox flame), from car backfires and the
      flamethrower; no test scene reaches it. [R4]
- [ ] Xbox style, cutscenes: the characters' hands are the Xbox's (skin-coloured, posed). On a
      real Xbox, are the characters' toes shrunk in cutscenes? (reMiami leaves them alone.) [R4]
- [ ] PS2 style: the radio stations, the hotel phone call's speech and the ambience sound like the
      PS2 (from the PS2 import); switching to the PC style mid-drive keeps the radio going. [R5b]
- [ ] In the PS2 style, burning cars and explosions look like on a PS2 (orange, as the PC); only
      compared against stills from an unreliable savestate so far. [R4]

- [ ] Map data (start in the Xbox style): the building at (390, -481) (`deco_polbuild`) has no
      invisible walls; North Beach's shop windows are there; billboards and Ocean Drive
      buildings stay drawn from further away. Then switch to PC in the menu: it says "map data
      changes on next start", and after a restart the PC's map is back. [R4]

## Needs other hardware or a capture

- [ ] Side by side with the real Xbox (xemu, ideally recorded at full frame rate): a burning car
      at 10-15 m, the blast and the smoke after it, in reMiami's Xbox style and on the Xbox. Not
      compared to hardware yet (no capture could be staged). [R4]
- [ ] N150: record frame times at the fixed spots (the `spots` test places are a good set). [2]
