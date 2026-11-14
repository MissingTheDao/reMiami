// Automation hooks for the regression harness (rust/revc/src/testdrive.rs, scripts/test.sh).
// Plain C entry points over game state; they do nothing unless the Rust host is in test mode.
#if defined LIBRW_HOST

#include "common.h"
#include "rwcore.h"
#include "skeleton.h"
#include "platform.h"
#include "crossplatform.h"
#include "Frontend.h"
#include "ControllerConfig.h"
extern "C" int PadLayoutCurrent(void);
void PadPictureContextPress(int8 action);	// core/PadPicture.cpp
extern "C" void PadLayoutApply(int layout);
#include "Timer.h"
#include "World.h"
#include "Camera.h"
#include "PlayerInfo.h"
#include "GenericGameStorage.h"
#include "Script.h"
#include "ModelInfo.h"
#include "SimpleModelInfo.h"
#include "TimeModelInfo.h"
#include "ClumpModelInfo.h"
#include "TxdStore.h"
#include "Text.h"
#include "ColModel.h"
#include "Pad.h"
#include "Game.h"
#include "Streaming.h"
#include "Clock.h"
#include "Weather.h"
#include "Timecycle.h"
#include "Population.h"
#include "CarCtrl.h"
#include "PlayerPed.h"
#include "ColStore.h"
#include "Hud.h"
#include "Font.h"
#include "GenericGameStorage.h"
#include "Messages.h"
#include "Pools.h"
#include "Zones.h"
#include "Building.h"
#include "Automobile.h"
#include "Explosion.h"
#include "ParticleMgr.h"
#include "ModelIndices.h"
#include "Bike.h"
#include "RpAnimBlend.h"
#include "AnimBlendAssociation.h"
#include "AnimBlendHierarchy.h"
#include "AnimBlendAssocGroup.h"
#include "AnimBlendAssociation.h"
#include "OnscreenTimer.h"
#include "CutsceneMgr.h"
#include "Radar.h"
#include "User.h"

#ifdef AUDIO_OAL
// Audio effects state (sampman_oal.cpp globals).
extern bool _effectsSupported;
extern bool _usingEFX;
extern int usingEAX, usingEAX3;
extern float _fEffectsLevel;
#endif

class CBuilding;
#ifdef FIX_BUGS
void SwapBuildingLod(CBuilding *building, int32 from, int32 to);	// control/Script4.cpp
#endif

extern "C" {

// Reverb state: whether effects are available/enabled and the current effects (reverb send) level.
void
revc_test_audio_fx(int *supported, int *mode, float *level)
{
#ifdef AUDIO_OAL
	*supported = _effectsSupported;
	*mode = usingEAX3 ? 3 : usingEAX ? 1 : _usingEFX ? 2 : 0; // 0 off, 1 EAX, 2 EFX, 3 EAX3
	*level = _fEffectsLevel;
#else
	*supported = 0; *mode = 0; *level = 0.0f;
#endif
}

// 0 = other (loading, init), 1 = front-end menu, 2 = playing.
int
revc_test_state(void)
{
	switch (gGameState) {
	case GS_FRONTEND: return 1;
	case GS_PLAYING_GAME: return 2;
	default: return 0;
	}
}

unsigned int
revc_test_frame(void)
{
	return CTimer::GetFrameCounter();
}

// Game time in ms (pauses with the game, unlike the frame counter it isn't tied to fps).
unsigned int
revc_test_game_ms(void)
{
	return CTimer::GetTimeInMilliseconds();
}

// Counts as player 1 input now: the player's idle animations (CPlayerPed::PlayIdleAnimations,
// random, after 30 s without input) stay off, so screenshots don't depend on them.
void
revc_test_touch_input(void)
{
	CPad::GetPad(0)->LastTimeTouched = CTimer::GetTimeInMilliseconds();
}

// Saves to slot (0-based) like the pause menu's Save Game, then rebuilds the slot list. Returns
// SaveSlot's result (0 = saved); state = Slots[slot], name/date = the slot's lines (260/70 units).
int
revc_test_save_slot(int slot, int *state, wchar *name, wchar *date)
{
	int r = PcSaveHelper.SaveSlot(slot);
	PcSaveHelper.PopulateSlotInfo();
	*state = Slots[slot];
	memcpy(name, SlotFileName[slot], sizeof(SlotFileName[slot]));
	memcpy(date, SlotSaveDate[slot], sizeof(SlotSaveDate[slot]));
	return r;
}

// Non-zero while the screen is fading or faded (e.g. the fade-in after loading a save).
int
revc_test_fading(void)
{
	return TheCamera.GetFading() || TheCamera.GetScreenFadeStatus() != FADE_0;
}

void
revc_test_quit(void)
{
	RsGlobal.quit = TRUE;
}

// Same steps as Start New Game in the menu (cutscenes scenario: the opening plays as for a
// player). Returns 0 if not in the front end.
int
revc_test_new_game(void)
{
	if (gGameState != GS_FRONTEND)
		return 0;
	FrontEndMenuManager.DoSettingsBeforeStartingAGame();
	return 1;
}

// Cutscenes scenario: puts the player on the nearest plain radar blip (a mission marker; within
// 300 m, sprite-less, not one following an entity) like walking into it. Returns 0 if none.
int
revc_test_goto_marker(float *x, float *y, float *z)
{
	CVector me = FindPlayerCoors();
	int best = -1;
	float bestDist = 300.0f;
	for (int i = 0; i < NUMRADARBLIPS; i++) {
		const sRadarTrace &b = CRadar::ms_RadarTrace[i];
		if (!b.m_bInUse || b.m_eRadarSprite != 0 || (b.m_eBlipType != BLIP_COORD && b.m_eBlipType != BLIP_CONTACT_POINT))
			continue;
		float d = (CVector2D(b.m_vecPos) - CVector2D(me)).Magnitude();
		if (d < bestDist) {
			bestDist = d;
			best = i;
		}
	}
	if (best < 0)
		return 0;
	CVector pos = CRadar::ms_RadarTrace[best].m_vecPos;
	// (a marker in the building the player is in, like the hotel's: walked to as it is, the
	// interior's area and colours kept; farther ones are outside: the main map, no room colours)
	if (bestDist > 50.0f && CGame::currArea != AREA_MAIN_MAP) {
		CGame::currArea = AREA_MAIN_MAP;
		CStreaming::RemoveBuildingsNotInArea(AREA_MAIN_MAP);
		CTimeCycle::StopExtraColour(false);
	}
	CTimer::Suspend();
	CStreaming::LoadScene(pos);
	CTimer::Resume();
	CColStore::EnsureCollisionIsInMemory(CVector2D(pos));
	if (pos.z <= -100.0f || pos.z == 0.0f)
		pos.z = CWorld::FindGroundZForCoord(pos.x, pos.y);
	FindPlayerPed()->Teleport(pos + CVector(0.0f, 0.0f, 1.0f));
	*x = pos.x;
	*y = pos.y;
	*z = pos.z;
	return 1;
}

// The running cutscene's name into name (size n, "" when none; with the time cycle's state:
// extra colours, ambient and the colour filter's colours); 1 while one runs.
int
revc_test_cutscene(char *name, int n)
{
	bool on = CCutsceneMgr::IsRunning();
	if (on)
		snprintf(name, n, "%s extra %d/%d amb %.2f %.2f %.2f blur %d %d %d", CCutsceneMgr::GetCutsceneName(),
			CTimeCycle::m_bExtraColourOn, CTimeCycle::m_ExtraColour, CTimeCycle::GetAmbientRed(),
			CTimeCycle::GetAmbientGreen(), CTimeCycle::GetAmbientBlue(), CTimeCycle::GetBlurRed(),
			CTimeCycle::GetBlurGreen(), CTimeCycle::GetBlurBlue());
	else
		snprintf(name, n, "%s", "");
	return on ? 1 : 0;
}

// Same steps as choosing a slot in the Load Game menu (Frontend.cpp, MENUPAGE_LOADING_IN_PROGRESS).
// slot is 0-based (GTAVCsf1.b = 0). Returns 0 if not in the front end or the slot is invalid.
int
revc_test_load_slot(int slot)
{
	if (gGameState != GS_FRONTEND)
		return 0;
	FrontEndMenuManager.m_nCurrSaveSlot = slot;
	if (!CheckSlotDataValid(slot))
		return 0;
	FrontEndMenuManager.DoSettingsBeforeStartingAGame();
	FrontEndMenuManager.m_bWantToLoad = true;
	return 1;
}

void
revc_test_player_pos(float *x, float *y, float *z)
{
	CVector v = FindPlayerCoors();
	*x = v.x;
	*y = v.y;
	*z = v.z;
}

// Pads the game uses as player 1 / player 2 (host joystick slots, -1 = none).
void
revc_test_joy_ids(int *joy1, int *joy2)
{
	*joy1 = PSGLOBAL(joy1id);
	*joy2 = PSGLOBAL(joy2id);
}

// Shakes pad 0 like the game does (CPad::StartShake), with vibration enabled in the settings.
void
revc_test_shake(int durationMs, int freq)
{
	FrontEndMenuManager.m_PrefsUseVibration = true;
	CPad::GetPad(0)->StartShake(durationMs, freq);
}

// Console reference scenes (plan task 13, Claude outputs/world-captures/SCENES.md): the player stands
// at (x, y) on the ground facing `heading` degrees (0 = north, counter-clockwise like the game), the
// normal follow camera directly behind him, HUD and radar on, the clock at hour:minute and the
// weather forced (0 sunny, 1 cloudy, 2 rainy, 3 foggy). No peds or traffic, so runs repeat.
float
revc_test_scene(float x, float y, float heading, int hour, int minute, int weather)
{
	CPopulation::PedDensityMultiplier = 0.0f;
	CCarCtrl::CarDensityMultiplier = 0.0f;
	CGame::currArea = AREA_MAIN_MAP;
	CStreaming::RemoveBuildingsNotInArea(AREA_MAIN_MAP);
	// The save starts inside the hotel: drop the interior's time-cycle colours (as leaving
	// through a door does), or the outside is lit like a room.
	CTimeCycle::StopExtraColour(false);
	CClock::SetGameClock(hour, minute);
	CWeather::ForceWeatherNow(weather);

	CVector pos(x, y, 20.0f);
	CTimer::Suspend();
	CStreaming::LoadScene(pos);
	CTimer::Resume();
	CColStore::EnsureCollisionIsInMemory(CVector2D(x, y));
	pos.z = CWorld::FindGroundZForCoord(x, y);

	CHud::m_Wants_To_Draw_Hud = true;
	CHud::m_HideRadar = false;
	CHud::SetHelpMessage(nil, true);

	CPlayerPed *ped = FindPlayerPed();
	CVector pedPos = pos;
	pedPos.z += ped->GetDistanceFromCentreOfMassToBaseOfModel();
	ped->Teleport(pedPos);
	float h = DEGTORAD(heading);
	ped->m_fRotationCur = ped->m_fRotationDest = h;
	ped->SetHeading(h);
	CWorld::ClearExcitingStuffFromArea(pedPos, 120.0f, true);

	TheCamera.RestoreWithJumpCut();
	TheCamera.SetCameraDirectlyBehindForFollowPed_CamOnAString();
	return pos.z;
}

// HUD text comparison shots (plan task 13, REVC_TEST_HUD_TEXT): puts one kind of HUD text on
// screen in a scene, with the game's own text keys and calls, as the scripts do: 0 zone name,
// 1 vehicle name, 2 help box, 3 mission title, 4 odd-job title (TAXI DRIVER), 5 mission clock +
// counter (taxi fares), 6 mission passed, 7 mission failed, 8 odd-job over + earnings, 9 speed
// bonus, 10 wanted level, 11 subtitle line. The clock and counter use the last two global script
// variables.
void
revc_test_hud_text(int kind)
{
	// kind + 100: the late part, called each frame from 1 s before the capture (once is enough):
	// the odd-job result line slides in, holds 1.5 s and slides out, so it starts late.
	static bool lateDone;
	if (kind >= 100) {
		if (lateDone)
			return;
		lateDone = true;
		if (kind == 108)
			CMessages::AddBigMessageWithNumber(TheText.Get("TSCORE"), 8000, 5, 0, -1, -1, -1, -1, -1);
		return;
	}
	lateDone = false;
	TheText.LoadMissionText((char*)"TAXI1");
	int32 var = CTheScripts::GetSizeOfVariableSpace() - 8;
	switch (kind) {
	case 0: CHud::SetZoneName(TheText.Get("DTOWN")); break;
	case 1: {	// the name shows while the player drives: an Infernus at his spot, him inside
		CStreaming::RequestModel(MI_INFERNUS, STREAMFLAGS_DEPENDENCY);
		CStreaming::LoadAllRequestedModels(false);
		CPlayerPed *ped = FindPlayerPed();
		CAutomobile *car = new CAutomobile(MI_INFERNUS, MISSION_VEHICLE);
		CVector pos = ped->GetPosition();
		pos.z = CWorld::FindGroundZForCoord(pos.x, pos.y) + car->GetDistanceFromCentreOfMassToBaseOfModel();
		car->SetPosition(pos);
		car->SetHeading(ped->m_fRotationCur);
		car->SetStatus(STATUS_ABANDONED);
		car->bEngineOn = false;
		CWorld::Add(car);
		ped->SetObjective(OBJECTIVE_ENTER_CAR_AS_DRIVER, car);
		ped->WarpPedIntoCar(car);
		TheCamera.RestoreWithJumpCut();
		break;
	}
	case 2: CHud::SetHelpMessage(TheText.Get("TIMER"), false); break;
	case 3: CMessages::AddBigMessage(TheText.Get("TAXI_1"), 8000, 1); break;
	case 4: CMessages::AddBigMessage(TheText.Get("TAXI_M"), 8000, 4); break;
	case 5:
		*CTheScripts::GetPointerToScriptVariable(var) = 155000;
		*CTheScripts::GetPointerToScriptVariable(var + 4) = 3;
		CUserDisplay::OnscnTimer.AddClock(var, (char*)"SEG3_1", true);	// "TIME:"
		CUserDisplay::OnscnTimer.AddCounter(var + 4, 0, (char*)"FARES", 0);
		break;
	case 6: CMessages::AddBigMessageWithNumber(TheText.Get("M_PASS"), 8000, 0, 1000, -1, -1, -1, -1, -1); break;
	case 7: CMessages::AddBigMessage(TheText.Get("M_FAIL"), 8000, 0); break;
	case 8:
		CMessages::AddBigMessage(TheText.Get("TAXI6"), 8000, 4);	// + the earnings line, late
		break;
	case 9: CMessages::AddBigMessage(TheText.Get("TAXI5"), 8000, 4); break;
	case 10: FindPlayerPed()->SetWantedLevel(2); break;
	case 11: CMessages::AddMessage(TheText.Get("NOTAXI"), 8000, 0); break;
	case 12: {	// a help text with button prompts (fire, target, sprint, change camera), as the prompts render per style
		static wchar help[128];
		AsciiToUnicode("Fire ~k~~PED_FIREWEAPON~ target ~k~~PED_LOCK_TARGET~ run ~k~~PED_SPRINT~ camera ~k~~CAMERA_CHANGE_VIEW_ALL_SITUATIONS~", help);
		CHud::SetHelpMessage(help, false);
		break;
	}
	}
}

// Menu comparison shots (plan task 13, REVC_TEST=menus): with the pause menu open over a loaded
// game, sets one state: the page (switched as the menu does), the selected row, the controller
// picture's context (0 foot, 1 car, 2 flying; -1 leave), the pad layout (-1 leave), the map legend
// (-1 leave). Returns 0 if the menu isn't open yet.
extern "C" void PadPictureSetContext(int context);
// Menu walk-through clips: one menu input as the player gives it (down: next row; change: left
// or right on the row, e.g. the radio picker).
void
revc_test_menu_input(int down, int change)
{
	if (FrontEndMenuManager.m_bMenuActive)
		FrontEndMenuManager.ProcessUserInput(down, 0, 0, 0, change);
}

// World start states for pad scripts (research autopad README): kind 0 on foot, 1 an Infernus,
// 2 a PCJ-600, 3 a Bloodring Banger, engine on, the player driving; at (x, y) on the ground facing `heading` degrees
// (0 = north), noon, clear, no traffic or peds (revc_test_scene).
void
revc_test_world_start(int kind, float x, float y, float heading)
{
	revc_test_scene(x, y, heading, 12, 0, 0);
	if (const char *layout = getenv("REVC_TEST_PAD_LAYOUT"))	// (0/1: Standard/Classic pad bindings, as for the menus)
		PadLayoutApply(atoi(layout));
	if (kind == 0)
		return;
	int mi = kind == 2 ? MI_PCJ600 : kind == 3 ? MI_BLOODRB : kind == 4 ? MI_MAVERICK : MI_INFERNUS;	// (3: the Bloodring Banger, the car the PS2 tests can spawn; 4: a heli, flying tests)
	CStreaming::RequestModel(mi, STREAMFLAGS_DEPENDENCY);
	CStreaming::LoadAllRequestedModels(false);
	CPlayerPed *ped = FindPlayerPed();
	CVehicle *veh = kind == 2 ? (CVehicle*)new CBike(mi, MISSION_VEHICLE) : (CVehicle*)new CAutomobile(mi, MISSION_VEHICLE);
	CVector pos = ped->GetPosition();
	pos.z = CWorld::FindGroundZForCoord(pos.x, pos.y) + veh->GetDistanceFromCentreOfMassToBaseOfModel();
	veh->SetPosition(pos);
	veh->SetHeading(DEGTORAD(heading));
	veh->SetStatus(STATUS_ABANDONED);
	veh->bEngineOn = true;
	CWorld::Add(veh);
	ped->SetObjective(OBJECTIVE_ENTER_CAR_AS_DRIVER, veh);
	ped->WarpPedIntoCar(veh);
	TheCamera.RestoreWithJumpCut();
	TheCamera.SetCameraDirectlyBehindForFollowPed_CamOnAString();
}

// Telemetry (REVC_TEST_TELEMETRY): out[0..11] = pos x y z, heading (degrees, 0 = north,
// counter-clockwise), speed (m/s), vehicle model (-1 on foot), suspension spring ratio of the
// four wheels (cars and bikes, -1 otherwise), gas pedal, steer angle; anim: the player's main
// animation's name, *animTime its time in seconds.
void
revc_test_telemetry(float *out, char *anim, int animLen, float *animTime)
{
	CPlayerPed *ped = FindPlayerPed();
	CVehicle *veh = ped->InVehicle() ? ped->m_pMyVehicle : nil;
	CPhysical *e = veh ? (CPhysical*)veh : (CPhysical*)ped;
	CVector p = e->GetPosition(), f = e->GetForward();
	out[0] = p.x; out[1] = p.y; out[2] = p.z;
	out[3] = veh ? RADTODEG(Atan2(-f.x, f.y)) : RADTODEG(ped->m_fRotationCur);	// (a ped's matrix lags its rotation)
	out[4] = e->m_vecMoveSpeed.Magnitude() * 50.0f;
	out[5] = veh ? veh->GetModelIndex() : -1.0f;
	for (int i = 0; i < 4; i++)
		out[6 + i] = veh && veh->IsCar() ? ((CAutomobile*)veh)->m_aSuspensionSpringRatio[i] :
			veh && veh->IsBike() ? ((CBike*)veh)->m_aSuspensionSpringRatio[i] : -1.0f;
	out[10] = veh ? veh->m_fGasPedal : 0.0f;
	out[11] = veh ? veh->m_fSteerAngle : RADTODEG(ped->m_fRotationDest);	// (on foot: the heading it turns to)
	anim[0] = '\0';
	*animTime = 0.0f;
	CAnimBlendAssociation *a = RpAnimBlendClumpGetMainAssociation(ped->GetClump(), nil, nil);
	if (a && a->hierarchy) {
		strncpy(anim, a->hierarchy->name, animLen - 1);
		anim[animLen - 1] = '\0';
		*animTime = a->currentTime;
	}
}

// Pad scripts (REVC_TEST=padscript, rust padscript.rs): pad 0's state for this frame, as a
// controller would send it. buttons: bits in padscript::BUTTONS order (cross circle square
// triangle l1 r1 l2 r2 l3 r3 select start up down left right); axes lx ly rx ry -1..1, l2 r2 0..1.
}	// extern "C"
bool gRevcTestPadOn = false;
CControllerState gRevcTestPad;
extern "C" {
void
revc_test_pad(int on, unsigned int b, float lx, float ly, float rx, float ry, float l2, float r2)
{
	gRevcTestPadOn = on != 0;
	CControllerState &s = gRevcTestPad;
	s.Clear();
	auto bit = [&](int i) { return (int16)((b >> i) & 1 ? 255 : 0); };
	s.Cross = bit(0); s.Circle = bit(1); s.Square = bit(2); s.Triangle = bit(3);
	s.LeftShoulder1 = bit(4); s.RightShoulder1 = bit(5);
	s.LeftShoulder2 = Max(bit(6), (int16)(l2 * 255.0f)); s.RightShoulder2 = Max(bit(7), (int16)(r2 * 255.0f));
	s.LeftShock = bit(8); s.RightShock = bit(9); s.Select = bit(10); s.Start = bit(11);
	s.DPadUp = bit(12); s.DPadDown = bit(13); s.DPadLeft = bit(14); s.DPadRight = bit(15);
	s.LeftStickX = (int16)(Clamp(lx, -1.0f, 1.0f) * 128.0f); s.LeftStickY = (int16)(Clamp(ly, -1.0f, 1.0f) * 128.0f);
	s.RightStickX = (int16)(Clamp(rx, -1.0f, 1.0f) * 128.0f); s.RightStickY = (int16)(Clamp(ry, -1.0f, 1.0f) * 128.0f);
}

extern "C++" bool MenuSkinPS2PageFocus(void);
// Where the menu is (pad-script routes): open, page, row; returns the PS2 style's page focus.
int
revc_test_menu_where(int *open, int *page, int *row)
{
	*open = FrontEndMenuManager.m_bMenuActive;
	*page = FrontEndMenuManager.m_nCurrScreen;
	*row = FrontEndMenuManager.m_nCurrOption;
	return MenuSkinPS2PageFocus();
}

// Button sweep (REVC_TEST=menusweep): the states to visit, in order. idx -> page, row (-1 the
// tab/icon bar focused), returns 0 past the end. Every tab page of the current UI style, its tab
// bar and each selectable row, then the "advanced" options list's rows.
}	// extern "C"
extern int8 gUiStyle;
bool MenuSkinPS2Active(int page, bool gameNotLoaded);
bool MenuSkinXboxActive(int page, bool gameNotLoaded);
int MenuSkinPS2TabPage(int i);
int MenuSkinXboxTabPage(int i);
void MenuSkinPS2SetFocus(int page, bool list);
extern "C" void MenuSkinXboxSetBarFocus(int bar);
extern "C" {
int
revc_test_sweep_state(int idx, int *page, int *row)
{
	static int pages[16], rows[16], n = -1, total;
	if (n < 0) {
		n = 0;
		for (int i = 0; i < 12 && n < 15; i++) {
			int p = gUiStyle == 1 ? MenuSkinPS2TabPage(i) : MenuSkinXboxTabPage(i);
			if (p < 0)
				break;
			pages[n++] = p;
		}
		pages[n++] = MENUPAGE_CONSOLE_OPTIONS;
		total = 0;
		for (int k = 0; k < n; k++) {
			int r = 0;
			while (r < NUM_MENUROWS && aScreens[pages[k]].m_aEntries[r].m_Action != MENUACTION_NOTHING &&
			       aScreens[pages[k]].m_aEntries[r].m_EntryName[0] != '\0')
				r++;
			rows[k] = Min(r, 12);	// (the options list is long: its first 12 rows)
			total += 1 + rows[k];
		}
	}
	for (int k = 0; k < n; k++) {
		if (idx == 0) {
			*page = pages[k];
			*row = -1;
			return 1;
		}
		idx--;
		if (idx < rows[k]) {
			if (aScreens[pages[k]].m_aEntries[idx].m_Action == MENUACTION_LABEL)
				*row = -2;	// (a heading: skipped)
			else
				*row = idx;
			*page = pages[k];
			return 1;
		}
		idx -= rows[k];
	}
	return 0;
}

}	// extern "C"
bool MenuSkinXboxSave(int page, bool gameNotLoaded);
bool MenuConsolePcPage(int page);
extern "C" {

static void
WalkAppend(char *out, int cap, int *pos, const char *text)
{
	if (*pos >= cap - 1)
		return;
	int n = snprintf(out + *pos, cap - *pos, "%s", text);
	*pos = n < 0 ? *pos : Min(cap - 1, *pos + n);
}

// REVC_TEST=menuwalk: every page reachable in the console style's menu through the real page
// switch (each row's target and "back" from every page reached, breadth first, plus the switches
// the game makes in code, e.g. after a failed load). Returns the number of switches that ended on a
// page the console style mustn't show (not drawn by its skin and not one of the listed PC pages,
// Frontend.cpp MenuConsolePcPage); the report lists them as "from:row->to", "from:back->to" or
// "from:direct->to", then " | visited N".
int
revc_test_menu_walk(char *out, int cap)
{
	CMenuManager &m = FrontEndMenuManager;
	auto ok = [](int page) {
		if (page == MENUPAGE_NONE || MenuConsolePcPage(page))
			return true;
		if (gUiStyle == 1)
			return MenuSkinPS2Active(page, false);
		return MenuSkinXboxActive(page, false) || MenuSkinXboxSave(page, false);
	};
	int pos = 0, bad = 0, head = 0, tail = 0, visitedN = 0;
	if (cap > 0)
		out[0] = '\0';
	static int queue[MENUPAGES];
	static bool visited[MENUPAGES];
	memset(visited, 0, sizeof(visited));
	auto enqueue = [&](int page) {
		if (page >= 0 && page < MENUPAGES && !visited[page]) {
			visited[page] = true;
			visitedN++;
			queue[tail++] = page;
		}
	};
	int first = gUiStyle == 1 ? MenuSkinPS2TabPage(0) : MenuSkinXboxTabPage(0);
	for (int i = 0; ; i++) {
		int t = gUiStyle == 1 ? MenuSkinPS2TabPage(i) : MenuSkinXboxTabPage(i);
		if (t < 0)
			break;
		enqueue(t);
	}
	enqueue(MENUPAGE_CONSOLE_OPTIONS);
	char item[48];
	auto check = [&](int from, const char *how, int to) {
		if (!ok(to)) {
			bad++;
			snprintf(item, sizeof(item), "%d:%s->%d ", from, how, to);
			WalkAppend(out, cap, &pos, item);
		} else if (to != MENUPAGE_NONE && to != MENUPAGE_OUTRO)
			enqueue(to);
	};
	while (head < tail) {
		int p = queue[head++];
		for (int r = 0; r < NUM_MENUROWS; r++) {
			if (aScreens[p].m_aEntries[r].m_Action != MENUACTION_CHANGEMENU || aScreens[p].m_aEntries[r].m_EntryName[0] == '\0')
				continue;
			m.m_nCurrScreen = p;
			m.m_nCurrOption = r;
			m.SwitchToNewScreen(0);
			char how[8];
			snprintf(how, sizeof(how), "%d", r);
			check(p, how, m.m_nCurrScreen);
		}
		m.m_nCurrScreen = p;
		m.m_nCurrOption = 0;
		m.SwitchToNewScreen(-2);
		check(p, "back", m.m_nCurrScreen);
	}
	static const int direct[] = { MENUPAGE_NEW_GAME, MENUPAGE_OPTIONS, MENUPAGE_CONTROLLER_PC,
		MENUPAGE_DISPLAY_SETTINGS, MENUPAGE_LANGUAGE_SETTINGS, MENUPAGE_CHOOSE_DELETE_SLOT };
	for (int target : direct) {
		m.m_nCurrScreen = first;
		m.m_nCurrOption = 0;
		m.SwitchToNewScreen(target);
		check(first, "direct", m.m_nCurrScreen);
	}
	snprintf(item, sizeof(item), " | visited %d", visitedN);
	WalkAppend(out, cap, &pos, item);
	m.m_nCurrScreen = m.m_nPrevScreen = first;
	m.m_nCurrOption = 0;
	return bad;
}

// Sets the focus for the sweep: the tab/icon bar (list 0) or the page's list (1).
void
revc_test_menu_set_focus(int page, int list)
{
	if (gUiStyle == 1)
		MenuSkinPS2SetFocus(page, list != 0);
	else
		MenuSkinXboxSetBarFocus(list == 0);
}

// Where the menu ended up: open, page, row, whether the page belongs to the console style
// (its skin draws it), and whether a fade or page change is still running.
void
revc_test_menu_status(int *open, int *page, int *row, int *skinned, int *fading)
{
	CMenuManager &m = FrontEndMenuManager;
	*open = m.m_bMenuActive;
	*page = m.m_nCurrScreen;
	*row = m.m_nCurrOption;
	*skinned = gUiStyle == 1 ? MenuSkinPS2Active(m.m_nCurrScreen, m.m_bGameNotLoaded) : MenuSkinXboxActive(m.m_nCurrScreen, m.m_bGameNotLoaded);
	*fading = m.m_bMenuActive && (m.m_nMenuFadeAlpha < 255 || m.m_firstStartCounter < 255);
}

// Motion tests: one menu input of any kind (down, up, select, back, left/right change).
void
revc_test_menu_key(int down, int up, int select, int back, int change)
{
	if (FrontEndMenuManager.m_bMenuActive)
		FrontEndMenuManager.ProcessUserInput(down, up, select, back, change);
}

int
revc_test_menu_page_id(const char *name)
{
	static const struct { const char *n; int p; } pages[] = {
		{ "stats", MENUPAGE_STATS }, { "brief", MENUPAGE_BRIEFS }, { "audio", MENUPAGE_SOUND_SETTINGS },
		{ "display", MENUPAGE_DISPLAY_SETTINGS }, { "graphics", MENUPAGE_GRAPHICS_SETTINGS }, { "map", MENUPAGE_MAP },
		{ "new_game_confirm", MENUPAGE_NEW_GAME_RELOAD }, { "load", MENUPAGE_CHOOSE_LOAD_SLOT },
		{ "load_confirm", MENUPAGE_LOAD_SLOT_CONFIRM }, { "quit_confirm", MENUPAGE_EXIT }, { "save", MENUPAGE_CHOOSE_SAVE_SLOT },
		{ "controls", MENUPAGE_CONTROLLER_PC }, { "pad", MENUPAGE_PAD_PICTURE }, { "pause", MENUPAGE_PAUSE_MENU },
		{ "console_display", MENUPAGE_CONSOLE_DISPLAY }, { "console_controls", MENUPAGE_CONSOLE_CONTROLS },
		{ "new_game", MENUPAGE_NEW_GAME },
		{ "console_options", MENUPAGE_CONSOLE_OPTIONS },
		{ "save_overwrite", MENUPAGE_SAVE_OVERWRITE_CONFIRM }, { "save_success", MENUPAGE_SAVE_SUCCESSFUL },
	};
	for (auto &e : pages)
		if (strcmp(e.n, name) == 0)
			return e.p;
	return -1;
}
extern "C++" bool MenuSkinPS2Active(int page, bool gameNotLoaded);
extern "C++" void MenuSkinPS2SetFocus(int page, bool list);	// (C++ linkage inside this file's extern "C" block)
// Clips: the map zoom held (1 R1, -1 R2, 0 nothing), as the player would hold the shoulders.
int gRevcTestMapZoom = 0;
void
revc_test_map_zoom(int dir)
{
	gRevcTestMapZoom = dir;
}

int
revc_test_menu_state(int page, int option, int context, int layout, int legend)
{
	if (!FrontEndMenuManager.m_bMenuActive) {
		if (!FrontEndMenuManager.m_bGameNotLoaded)
			FrontEndMenuManager.m_bStartUpFrontEndRequested = true;
		return 0;
	}
	{	// REVC_TEST_BRIEF_LISTS: Xbox list entries for the shots (core/BriefLists.cpp), once
		static bool filled = false;
		void BriefListsTestFill(void);
		if (!filled) { filled = true; BriefListsTestFill(); }
	}
	// layout: the pad layout, or on the Map page a zoom level (0 as opened, 1 twice, 2 the maximum)
	if (layout >= 0 && page == MENUPAGE_MAP) {
		float base = SCREEN_SCALE_Y(162.0f);
		FrontEndMenuManager.m_fMapSize = layout == 0 ? base : layout == 1 ? base * 2.0f : SCREEN_SCALE_Y(1000.0f);
	} else if (layout >= 0 && page == MENUPAGE_CONSOLE_CONTROLS && FrontEndMenuManager.m_nCurrScreen >= 0 && MenuSkinPS2Active(page, false)) {
		extern int8 gPs2Setup;	// the PS2 style's controller setup 1-4
		gPs2Setup = layout;
	} else if (layout >= 0)
		PadLayoutApply(layout);
	// context: the controller picture's context, or on Stats/Brief the Xbox sub-tab
	void MenuSkinXboxSetSubTab(int page, int tab);
	void MenuSkinXboxOptionsFor(int section);
	if (context >= 0 && (page == MENUPAGE_STATS || page == MENUPAGE_BRIEFS))
		MenuSkinXboxSetSubTab(page, context);
	else if (context >= 0 && page == MENUPAGE_CONSOLE_OPTIONS)	// the Xbox's "advanced" list of a section
		MenuSkinXboxOptionsFor(context);
	else if (context >= 0)
		PadPictureSetContext(context);
	if (legend >= 0)
		FrontEndMenuManager.m_PrefsShowLegends = legend;
	if (FrontEndMenuManager.m_nCurrScreen != page)
		FrontEndMenuManager.SwitchToNewScreen(SWITCH_TO_PAGE(page));
	// PS2 style: rows are only highlighted with the list focused (the Map page keeps its tab bar)
	if (option >= 0 || page != MENUPAGE_MAP)
		MenuSkinPS2SetFocus(page, page != MENUPAGE_MAP);
	void MenuSkinXboxSetBarFocus(int bar);	// Xbox style: shots show the page's rows focused
	MenuSkinXboxSetBarFocus(0);
	if (FrontEndMenuManager.m_nCurrOption != option) {
		// from the top, moving down as the player does (so long pages scroll the row into view)
		FrontEndMenuManager.m_nCurrOption = 0;
		FrontEndMenuManager.m_nFirstVisibleRowOnList = 0;
		for (int k = 0; k < 16 && FrontEndMenuManager.m_nCurrOption < option; k++)
			FrontEndMenuManager.ProcessUserInput(1, 0, 0, 0, 0);
		FrontEndMenuManager.m_nCurrOption = option;
	}
	return 1;
}

// Fixed-spot screenshots (plan task 3): puts the player at (x, y) on the main map (on the ground
// when z <= -100) with nothing random around (no peds or traffic, fixed time and weather) and
// points a fixed camera from player + (dx, dy, dz) at the player, with the HUD hidden, like the mission scripts do
// (SET_AREA_VISIBLE, LOAD_SCENE, SET_PLAYER_COORDINATES, SET_FIXED_CAMERA_POSITION,
// POINT_CAMERA_AT_POINT). Returns the ground z used.
float
revc_test_goto_spot(float x, float y, float z, float dx, float dy, float dz, int hour, int minute)
{
	CPopulation::PedDensityMultiplier = 0.0f;
	CCarCtrl::CarDensityMultiplier = 0.0f;
	CGame::currArea = AREA_MAIN_MAP;
	CStreaming::RemoveBuildingsNotInArea(AREA_MAIN_MAP);
	// The save starts inside the hotel: drop the interior's time-cycle colours (as leaving
	// through a door does), or the outside is lit like a room.
	CTimeCycle::StopExtraColour(false);
	CClock::SetGameClock(hour, minute);
	// REVC_TEST_RAIN=1 (not in the regression set): rain at the spots, for the screen droplet check
	CWeather::ForceWeatherNow(getenv("REVC_TEST_RAIN") ? WEATHER_RAINY : WEATHER_SUNNY);

	CVector pos(x, y, z <= -100.0f ? 20.0f : z);
	CTimer::Suspend();
	CStreaming::LoadScene(pos);
	CTimer::Resume();
	if (z <= -100.0f) {
		CColStore::EnsureCollisionIsInMemory(CVector2D(x, y));
		pos.z = CWorld::FindGroundZForCoord(x, y);
	}
	// World only: no HUD, radar, help boxes or zone names (they depend on timing).
	CHud::m_Wants_To_Draw_Hud = false;
	CHud::m_HideRadar = true;
	CHud::SetHelpMessage(nil, true);

	CPlayerPed *ped = FindPlayerPed();
	CVector pedPos = pos;
	pedPos.z += ped->GetDistanceFromCentreOfMassToBaseOfModel();
	ped->Teleport(pedPos);
	CWorld::ClearExcitingStuffFromArea(pedPos, 120.0f, true);

	TheCamera.SetCamPositionForFixedMode(pos + CVector(dx, dy, dz), CVector(0.0f, 0.0f, 0.0f));
	// With REVC_TEST_RAIN the camera looks up into the rain (the game only sprays screen drops
	// when the camera looks level or up), at a point high above the player.
	TheCamera.TakeControlNoEntity(getenv("REVC_TEST_RAIN") ? pedPos + CVector(0.0f, 0.0f, 80.0f) : pedPos, JUMP_CUT, CAMCONTROL_SCRIPT);
	return pos.z;
}

// Shows a help box with every controller button token (CFont BUTTON_ICONS order), to check the
// glyphs of the pad in use (gamepad scenario).
void
revc_test_show_buttons(void)
{
	static wchar text[128];
	AsciiToUnicode("~X~ ~O~ ~Q~ ~T~  ~K~ ~M~ ~A~  ~J~ ~V~ ~C~  ~U~ ~D~ ~<~ ~>~  ~H~ ~L~ ~(~ ~)~", text);
	CHud::SetHelpMessage(text, false, true);	// not "quick": those fade out after 1.5 s
}

// Controls menu "BUTTON PROMPTS" setting (skel/host/glyphs.cpp), as if chosen in the menu.
extern int8 gButtonPromptsOverride;
void
revc_test_set_prompt_style(int style)
{
	gButtonPromptsOverride = style;
}

// Opens the Controls page of the front-end menu (boot scenario screenshot).
void
revc_test_show_controls_menu(void)
{
	// REVC_TEST_MENU_PAGE=n / REVC_TEST_MENU_OPTION=k: another page/entry (checking menu layouts).
	const char *page = getenv("REVC_TEST_MENU_PAGE"), *option = getenv("REVC_TEST_MENU_OPTION");
	// REVC_TEST_PAD_LAYOUT=0/1: Standard/Classic pad bindings first.
	if (const char *layout = getenv("REVC_TEST_PAD_LAYOUT")) {
		PadLayoutApply(atoi(layout));
	}
	// REVC_TEST_PAD_CONTEXT=1: the pad picture's In Car side, 2 Flying.
	// (2: the Flying side; pressed once per call, the in-game capture calls this twice)
	static bool contextDone;
	if (getenv("REVC_TEST_PAD_CONTEXT") && !contextDone) {
		contextDone = true;
		for (int n = atoi(getenv("REVC_TEST_PAD_CONTEXT")); n > 0; n--)
			PadPictureContextPress(FEOPTION_ACTION_SELECT);
	}
	int revc_test_menu_page_id(const char *name);
	int named = page ? revc_test_menu_page_id(page) : -1;
	int target = !page ? MENUPAGE_CONTROLLER_PC : named >= 0 ? named : atoi(page);
	// In a game with the menu open: switch as the menu does (its page transition sets the panel)
	if (FrontEndMenuManager.m_bMenuActive && !FrontEndMenuManager.m_bGameNotLoaded && FrontEndMenuManager.m_nCurrScreen != target)
		FrontEndMenuManager.SwitchToNewScreen(target);
	else
		FrontEndMenuManager.m_nCurrScreen = target;
	FrontEndMenuManager.m_nCurrOption = option ? atoi(option) : 0;
	// In a game (REVC_TEST_MENU_INGAME): open the pause menu first; the driver calls this again
	// once it is open to pick the page.
	if (!FrontEndMenuManager.m_bMenuActive && !FrontEndMenuManager.m_bGameNotLoaded)
		FrontEndMenuManager.m_bStartUpFrontEndRequested = true;
}

// 1 if the Controls page has the BUTTON PROMPTS entry and its label text resolves (boot scenario).
int
revc_test_check_prompt_menu(void)
{
	bool found = false;
	for (int i = 0; i < NUM_MENUROWS; i++)
		if (strcmp(aScreens[MENUPAGE_CONTROLLER_PC].m_aEntries[i].m_EntryName, "RVC_GLY") == 0)
			found = true;
	char text[64];
	strncpy(text, UnicodeToAscii(TheText.Get("RVC_GLY")), sizeof(text) - 1);
	text[sizeof(text) - 1] = '\0';
	return found && strcmp(text, "BUTTON PROMPTS") == 0;
}

// Loads mission text `table` like the LOAD_MISSION_TEXT script command and returns the string for
// `key` (formats scenario); `restore` reloads the mission text that was loaded before.
const wchar *
revc_test_mission_text(const char *table, const char *key)
{
	char name[8];
	strncpy(name, table, sizeof(name));
	TheText.LoadMissionText(name);
	return TheText.Get(key);
}

void
revc_test_restore_mission_text(const char *previous)
{
	if (previous[0]) {
		char name[8];
		strncpy(name, previous, sizeof(name));
		TheText.LoadMissionText(name);
	}
}

void
revc_test_current_mission_text(char *out8)
{
	TheText.GetNameOfLoadedMissionText(out8);
}

// Removes help text (the save's tutorial script keeps posting tips); called every frame at a spot.
void
revc_test_clear_help(void)
{
	CHud::SetHelpMessage(nil, true);
}

// Writes what the C++ loaders hold after loading (model info from IDE, collision, GXT, geometry of
// loaded models, texture sizes of loaded TXDs) as tab-separated lines (names may contain spaces), for the Rust formats check
// (rust/revc/src/formatcheck.rs) to compare against revc-formats. Returns 0 if the file can't be
// written.
static void
DumpGeometry(FILE *f, const char *model, RpAtomic *atomic)
{
	RpGeometry *geo = RpAtomicGetGeometry(atomic);
	if (geo == nil)
		return;
	int numMats = geo->matList.numMaterials; // fakerw declares RpGeometryGetNumMaterials but has no body
	fprintf(f, "GEO\t%s\t%d\t%d\t%d\t", model, RpGeometryGetNumVertices(geo), RpGeometryGetNumTriangles(geo), numMats);
	bool any = false;
	for (int i = 0; i < numMats; i++) {
		RwTexture *tex = RpMaterialGetTexture(RpGeometryGetMaterial(geo, i));
		if (tex) {
			fprintf(f, "%s%s", any ? "|" : "", RwTextureGetName(tex));
			any = true;
		}
	}
	fprintf(f, "%s\n", any ? "" : "-");
}

struct DumpCtx { FILE *f; const char *name; };

static RpAtomic *
DumpClumpAtomic(RpAtomic *atomic, void *data)
{
	DumpCtx *ctx = (DumpCtx*)data;
	DumpGeometry(ctx->f, ctx->name, atomic);
	return atomic;
}

static RwTexture *
DumpTexture(RwTexture *tex, void *data)
{
	DumpCtx *ctx = (DumpCtx*)data;
	RwRaster *ras = RwTextureGetRaster(tex);
	fprintf(ctx->f, "TEX\t%s\t%s\t%d\t%d\n", ctx->name, RwTextureGetName(tex),
	        ras ? RwRasterGetWidth(ras) : 0, ras ? RwRasterGetHeight(ras) : 0);
	return tex;
}

int
revc_test_dump_formats(const char *path)
{
	FILE *f = fopen(path, "w");
	if (f == nil)
		return 0;
	for (int id = 0; id < MODELINFOSIZE; id++) {
		CBaseModelInfo *mi = CModelInfo::GetModelInfo(id);
		if (mi == nil)
			continue;
		int type = mi->GetModelType();
		int slot = mi->GetTxdSlot();
		const char *txd = slot >= 0 && CTxdStore::GetSlot(slot) ? CTxdStore::GetTxdName(slot) : "-";
		fprintf(f, "MI\t%d\t%d\t%s\t%s", id, type, mi->GetModelName(), txd);
		if (type == MITYPE_SIMPLE || type == MITYPE_TIME) {
			CSimpleModelInfo *smi = (CSimpleModelInfo*)mi;
			fprintf(f, "\t%d\t%.9g\t%.9g\t%.9g", smi->m_numAtomics, smi->m_lodDistances[0],
			        smi->m_lodDistances[1], smi->m_lodDistances[2]);
			if (type == MITYPE_TIME)
				fprintf(f, "\t%d\t%d", ((CTimeModelInfo*)mi)->GetTimeOn(), ((CTimeModelInfo*)mi)->GetTimeOff());
		}
		fprintf(f, "\n");

		CColModel *col = mi->GetColModel();
		if (col && mi->DoesOwnColModel())
			fprintf(f, "COL\t%s\t%d\t%d\t%d\t%d\t%.9g\t%d\n", mi->GetModelName(), col->level, col->numSpheres,
			        col->numBoxes, col->numTriangles, col->boundingSphere.radius, type);

		if (mi->IsSimple()) {
			CSimpleModelInfo *smi = (CSimpleModelInfo*)mi;
			// m_atomics[2] doubles as the related-LOD model / weapon id, so only 0 and 1 are dumped.
			for (int i = 0; i < smi->m_numAtomics && i < 2; i++)
				if (smi->m_atomics[i])
					DumpGeometry(f, mi->GetModelName(), smi->m_atomics[i]);
		} else if (mi->IsClump() && mi->GetRwObject()) {
			DumpCtx ctx = { f, mi->GetModelName() };
			RpClumpForAllAtomics((RpClump*)mi->GetRwObject(), DumpClumpAtomic, &ctx);
		}
	}
	for (int slot = 0; slot < CTxdStore::GetNumSlots(); slot++) {
		TxdDef *def = CTxdStore::GetSlot(slot);
		if (def == nil || def->texDict == nil)
			continue;
		DumpCtx ctx = { f, def->name };
		RwTexDictionaryForAllTextures(def->texDict, DumpTexture, &ctx);
	}
	for (int i = 0; i < TheText.GetNumKeys(); i++) {
		const char *key = TheText.GetKeyName(i);
		fprintf(f, "GXT\t%.8s", key);
		for (wchar *c = TheText.Get(key); *c; c++)
			fprintf(f, "\t%x", *c);
		fprintf(f, "\n");
	}
	fclose(f);
	return 1;
}

}

#endif

// Controller layouts (boot scenario): the test config is recognised as ORIGINAL PC, and each preset
// applies and is recognised again (Fire on B for original PC and Xbox Classic, on the right trigger
// for Xbox Standard). Returns 1 if so; leaves Standard applied.
extern "C" int
revc_test_check_pad_layouts(void)
{
	// The test config holds re3's old pad bindings: ORIGINAL PC.
	bool original = PadLayoutCurrent() == 2;
	PadLayoutApply(2);
	original = original && PadLayoutCurrent() == 2 && ControlsManager.GetControllerKeyAssociatedWithAction(PED_FIREWEAPON, JOYSTICK) == 1 &&
		ControlsManager.GetControllerKeyAssociatedWithAction(GO_FORWARD, JOYSTICK) == 13;
	PadLayoutApply(1);
	bool classic = PadLayoutCurrent() == 1 && ControlsManager.GetControllerKeyAssociatedWithAction(PED_FIREWEAPON, JOYSTICK) == 1;
	PadLayoutApply(0);
	bool standard = PadLayoutCurrent() == 0 && ControlsManager.GetControllerKeyAssociatedWithAction(PED_FIREWEAPON, JOYSTICK) == 6 &&
		ControlsManager.GetControllerKeyAssociatedWithAction(VEHICLE_ACCELERATE, JOYSTICK) == 6;
	return original && classic && standard;
}

// Loads and frees every texture dictionary in the CD image that isn't loaded yet (as the PC
// game's CreateTxdImageForVideoCard walks them), so REVC_TXD=check compares every game texture.
// Returns how many were loaded.
extern "C" int
revc_test_load_all_txds(void)
{
	int n = 0;
	CStreaming::FlushRequestList();
	for (int i = 0; i < TXDSTORESIZE; i++) {
		if (CTxdStore::GetSlot(i) == nil || !CStreaming::IsObjectInCdImage(i + STREAM_OFFSET_TXD) ||
		    CStreaming::HasTxdLoaded(i))
			continue;
		CStreaming::RequestTxd(i, STREAMFLAGS_KEEP_IN_MEMORY);
		CStreaming::RequestModelStream(0);
		CStreaming::FlushChannels();
		if (CTxdStore::GetSlot(i)->texDict)
			n++;
		CStreaming::RemoveTxd(i);
		CStreaming::FlushRequestList();
	}
	return n;
}

static RwTexture *
DestroyTex(RwTexture *tex, void *count)
{
	(*(int*)count)++;
	RwTextureDestroy(tex);
	return tex;
}

// Reads one texture dictionary from memory through the game's reader (RwTexDictionaryGtaStreamRead,
// so REVC_TXD=check compares it) and frees it. Returns its texture count, -1 if it failed.
extern "C" int
revc_test_read_txd(const unsigned char *data, size_t len)
{
	RwMemory mem;
	mem.start = (RwUInt8*)data;
	mem.length = len;
	RwStream *s = RwStreamOpen(rwSTREAMMEMORY, rwSTREAMREAD, &mem);
	if (s == nil)
		return -1;
	RwTexDictionary *dict = nil;
	if (RwStreamFindChunk(s, rwID_TEXDICTIONARY, nil, nil))
		dict = RwTexDictionaryGtaStreamRead(s);
	RwStreamClose(s, nil);
	if (dict == nil)
		return -1;
	int n = 0;
	RwTexDictionaryForAllTextures(dict, DestroyTex, &n);
	RwTexDictionaryDestroy(dict);
	return n;
}

// Ped-hit check (REVC_TEST=pedhit): a car rolls at a fixed speed into a row of standing peds on
// the airport tarmac; for each ped the highest point it reaches above where it stood is recorded.
// The physics runs in fixed 30 Hz steps, so the result must not depend on the display rate
// (REVC_FIXED_FPS) or on render interpolation (REVC_NO_BONE_INTERP).
#include "Automobile.h"
#include "CivilianPed.h"
#include "Script.h"

#define PEDHIT_NUM 5
static CVehicle *phCar;
static CPed *phPeds[PEDHIT_NUM];
static float phBaseZ[PEDHIT_NUM], phMaxRise[PEDHIT_NUM];
// As drawn (render interpolation applied): highest body position and head bone above their start.
static float phHeadBase[PEDHIT_NUM], phDrawnRise[PEDHIT_NUM], phDrawnHeadRise[PEDHIT_NUM];
static bool phHit[PEDHIT_NUM];
static CVector phCarSpeed, phCarStart;
static uint32 phStartMs;
static void PedHitStep(void);
static void PedHitRender(void);
extern void (*gTestStepHook)(void);
extern void (*gTestRenderHook)(void);

extern "C" int
revc_test_pedhit_setup(float kmh)
{
	// The first candidate whose ground is flat (within 0.3 m) and clear for 25 m towards +y:
	// beach sand, the airport runway, roads.
	static const float cand[][2] = {
		{ 520.0f, -1100.0f }, { 560.0f, -700.0f }, { 480.0f, -1400.0f }, { -1600.0f, -1250.0f },
		{ -1300.0f, -1250.0f }, { 290.0f, -1000.0f },
	};
	float x = 0.0f, y = 0.0f;
	bool found = false;
	for (int c = 0; c < (int)(sizeof(cand)/sizeof(cand[0])) && !found; c++) {
		x = cand[c][0]; y = cand[c][1];
		revc_test_goto_spot(x - 15.0f, y, -100.0f, -10.0f, -12.0f, 8.0f, 12, 0);
		float z0 = CWorld::FindGroundZForCoord(x, y), lo = z0, hi = z0;
		for (float d = 0.0f; d <= 25.0f; d += 1.0f)
			for (float s = -1.5f; s <= 1.5f; s += 1.5f) {
				float z = CWorld::FindGroundZForCoord(x + s, y + d);
				lo = Min(lo, z); hi = Max(hi, z);
			}
		printf("[revc-test] pedhit: candidate (%.0f, %.0f) ground %.2f..%.2f\n", x, y, lo, hi);
		found = hi - lo < 0.3f && z0 > 0.0f;
	}
	if (!found)
		return 0;
	CStreaming::RequestModel(MI_ADMIRAL, STREAMFLAGS_DEPENDENCY);
	CStreaming::RequestModel(MI_MALE01, STREAMFLAGS_DEPENDENCY);
	CStreaming::LoadAllRequestedModels(false);
	if (!CStreaming::HasModelLoaded(MI_ADMIRAL) || !CStreaming::HasModelLoaded(MI_MALE01))
		return 0;

	CAutomobile *car = new CAutomobile(MI_ADMIRAL, MISSION_VEHICLE);
	CVector pos(x, y, CWorld::FindGroundZForCoord(x, y));
	pos.z += car->GetDistanceFromCentreOfMassToBaseOfModel();
	car->SetPosition(pos);
	car->SetHeading(0.0f);	// facing +y
	CTheScripts::ClearSpaceForMissionEntity(pos, car);
	car->SetStatus(STATUS_ABANDONED);
	car->bIsLocked = true;
	car->AutoPilot.m_nCarMission = MISSION_NONE;
	car->bEngineOn = false;
	CWorld::Add(car);
	phCar = car;
	phCarStart = car->GetPosition();
	phCarSpeed = car->GetForward() * (kmh / 180.0f);	// game speed unit: 50 m/s

	static const float ahead[PEDHIT_NUM] = { 8.0f, 11.0f, 14.0f, 17.0f, 20.0f };
	static const float side[PEDHIT_NUM] = { 0.0f, 0.5f, -0.5f, 0.8f, -0.8f };
	for (int i = 0; i < PEDHIT_NUM; i++) {
		CPed *ped = new CCivilianPed(PEDTYPE_CIVMALE, MI_MALE01);
		ped->CharCreatedBy = MISSION_CHAR;
		ped->bRespondsToThreats = false;
		CVector p(x + side[i], y + ahead[i], 0.0f);
		p.z = CWorld::FindGroundZForCoord(p.x, p.y) + 1.0f;
		ped->SetPosition(p);
		ped->SetOrientation(0.0f, 0.0f, 0.0f);
		CWorld::Add(ped);
		CPopulation::ms_nTotalMissionPeds++;
		phPeds[i] = ped;
		phBaseZ[i] = p.z;
		phMaxRise[i] = phDrawnRise[i] = phDrawnHeadRise[i] = -100.0f;
		phHeadBase[i] = 0.0f;	// set on the first drawn frame
		phHit[i] = false;
	}
	phStartMs = CTimer::GetTimeInMilliseconds();
	gTestStepHook = PedHitStep;
	gTestRenderHook = PedHitRender;
	return 1;
}

// Once per game step (gTestStepHook, before CGame::Process): keeps the car at its speed for the
// first 2 s, then records the peds' heights.
static void
PedHitStep(void)
{
	uint32 t = CTimer::GetTimeInMilliseconds() - phStartMs;
	if (phCar && t < 4000) {
		phCar->SetIsStatic(false);	// resting physics objects sleep until disturbed
		phCar->SetMoveSpeed(phCarSpeed);
	}
	for (int i = 0; i < PEDHIT_NUM; i++) {
		CPed *ped = phPeds[i];
		if (ped == nil)	// mission peds stay for the few seconds the check runs
			continue;
		float rise = ped->GetPosition().z - phBaseZ[i];
		if (rise > phMaxRise[i])
			phMaxRise[i] = rise;
		if (ped->m_fHealth < 100.0f || ped->GetPedState() == PED_FALL || ped->GetPedState() == PED_DIE ||
		    ped->GetPedState() == PED_DEAD)
			phHit[i] = true;
	}
}

static void
PedHitRender(void)
{
	for (int i = 0; i < PEDHIT_NUM; i++) {
		CPed *ped = phPeds[i];
		if (ped == nil || ped->GetClump() == nil)
			continue;
		float body = ped->GetPosition().z - phBaseZ[i];
		if (body > phDrawnRise[i])
			phDrawnRise[i] = body;
		float head = ped->GetNodePosition(PED_HEAD).z;
		if (phHeadBase[i] == 0.0f)
			phHeadBase[i] = head;
		if (head - phHeadBase[i] > phDrawnHeadRise[i])
			phDrawnHeadRise[i] = head - phHeadBase[i];
	}
}

// Called every drawn frame by the test driver; returns the game ms since setup.
extern "C" int
revc_test_pedhit_sample(void)
{
	return (int)(CTimer::GetTimeInMilliseconds() - phStartMs);
}

extern "C" void
revc_test_pedhit_drawn(float *bodyRise, float *headRise, int *hit)
{
	for (int i = 0; i < PEDHIT_NUM; i++) {
		bodyRise[i] = phDrawnRise[i];
		headRise[i] = phDrawnHeadRise[i];
		hit[i] = phHit[i];
	}
}

extern "C" void
revc_test_pedhit_result(float *maxRise, float *carDist)
{
	*carDist = phCar ? (phCar->GetPosition() - phCarStart).Magnitude() : -1.0f;
	for (int i = 0; i < PEDHIT_NUM; i++)
		maxRise[i] = phMaxRise[i];
}

// Scanner check (REVC_TEST=scanner): the player in a police car with a wanted level, so the
// police scanner talks (with its radio crackle); the mixer output is recorded (REVC_TEST_WAV).
extern "C" int
revc_test_scanner_setup(void)
{
	const float x = 480.0f, y = -1400.0f;	// beach (flat, see revc_test_pedhit_setup)
	revc_test_goto_spot(x - 10.0f, y, -100.0f, -10.0f, -12.0f, 8.0f, 22, 0);
	CStreaming::RequestModel(MI_POLICE, STREAMFLAGS_DEPENDENCY);
	CStreaming::LoadAllRequestedModels(false);
	if (!CStreaming::HasModelLoaded(MI_POLICE))
		return 0;
	CAutomobile *car = new CAutomobile(MI_POLICE, MISSION_VEHICLE);
	CVector pos(x, y, CWorld::FindGroundZForCoord(x, y));
	pos.z += car->GetDistanceFromCentreOfMassToBaseOfModel();
	car->SetPosition(pos);
	CTheScripts::ClearSpaceForMissionEntity(pos, car);
	car->SetStatus(STATUS_ABANDONED);
	CWorld::Add(car);
	CPlayerPed *player = FindPlayerPed();
	player->SetObjective(OBJECTIVE_ENTER_CAR_AS_DRIVER, car);
	player->WarpPedIntoCar(car);
	player->SetWantedLevel(2);
	// the normal follow camera behind the car (as in play), not goto_spot's fixed one: sounds
	// attached to the car are then centred, as the player hears them
	TheCamera.RestoreWithJumpCut();
	CHud::m_Wants_To_Draw_Hud = true;
	return 1;
}

// Cutscene data check (REVC_TEST_CUT_LOAD): loads a cut's animations and camera path as a mission
// does, describes them into out (camera end in ms, each animation's length) and unloads them.
extern "C" int
revc_test_cut_load(const char *cut, char *out, int len)
{
	CCutsceneMgr::LoadCutsceneData(cut);
	// the camera path's end as the fly-by camera computes it (Cam.cpp Process_FlyBy)
	float *path = TheCamera.m_arrPathArray[2].m_arr_PathData;
	int points = path ? (int)path[0] : 0;
	int n = snprintf(out, len, "camera %d points, end %.2f s;", points, points > 0 ? path[10 * (points - 1) + 1] : 0.0f);
	// the cut's animations (the associations need the cut's models, which the script loads)
	if (CAnimBlock *block = CAnimManager::GetAnimationBlock(cut)) {
		for (int i = 0; i < block->numAnims && n < len - 40; i++) {
			CAnimBlendHierarchy *h = CAnimManager::GetAnimation(block->firstIndex + i);
			n += snprintf(out + n, len - n, " %s %.2f s", h->name, h->totalLength);
		}
	}
	CCutsceneMgr::DeleteCutsceneData();
	return n;
}

// Prompt check (REVC_TEST_PROMPTS): key's text as the HUD shows it, the controls filled in by the
// prompt filler (Messages.cpp), in ASCII ('?' for the rest); returns its length.
extern "C" int
revc_test_prompt_text(const char *key, char *out, int len)
{
	wchar buf[256];
	CMessages::WideStringCopy(buf, TheText.Get(key), 256);
	CMessages::InsertPlayerControlKeysInString(buf);
	int i = 0;
	for (; buf[i] && i < len - 1; i++)
		out[i] = buf[i] < 128 ? (char)buf[i] : '?';
	out[i] = '\0';
	return i;
}

// Fire comparison shots (REVC_TEST_FIRE): 0 a burning car about 12 m in front of the player (health
// below the engine-fire threshold, as a badly damaged car burns before it blows up; facing the
// camera, so its burning engine is in view), 1 a car explosion at that spot (the fireball).
extern "C" void
revc_test_fire(int kind)
{
	CPlayerPed *ped = FindPlayerPed();
	if (ped == nil)
		return;
	CVector spot = ped->GetPosition() + ped->GetForward() * 12.0f;
	spot.z = CWorld::FindGroundZForCoord(spot.x, spot.y) + 1.0f;
	if (kind == 1) {
		CExplosion::AddExplosion(nil, nil, EXPLOSION_CAR, spot, 0, true);
		return;
	}
	int model = MI_ADMIRAL;
	CStreaming::RequestModel(model, STREAMFLAGS_DONT_REMOVE);
	CStreaming::LoadAllRequestedModels(false);
	if (CStreaming::ms_aInfoForModel[model].m_loadState != STREAMSTATE_LOADED) {
		debug("revc_test_fire: no car model\n");
		return;
	}
	CAutomobile *car = new CAutomobile(model, RANDOM_VEHICLE);
	car->SetPosition(spot);
	car->SetOrientation(0.0f, 0.0f, ped->m_fRotationCur + PI);
	car->SetStatus(STATUS_ABANDONED);
	car->m_nDoorLock = CARLOCK_UNLOCKED;
	car->m_fHealth = 240.0f;
	CWorld::Add(car);
}

// The live particle table's final animation frame for `kind` (0 car flame, 1 flame, 2 fireball):
// the fire scenes check that the UI style's values hold after the game reloaded its particles.
extern "C" int
revc_test_particle_frames(int kind)
{
	const tParticleType types[3] = { PARTICLE_CARFLAME, PARTICLE_FLAME, PARTICLE_FIREBALL };
	if (kind < 0 || kind > 2)
		return -1;
	return mod_ParticleSystemManager.m_aParticles[types[kind]].m_nFinalAnimationFrame;
}

// The pools' use when a test run ends (testdrive.rs finish).
extern "C" void
revc_test_pool_report(void)
{
	CPools::Report("at the end");
}

// Building swap check (REVC_TEST_SWAP=x,y,z,from,to; SilentPatch #62): what the script's
// SWAP_NEAREST_BUILDING_MODEL does, on the nearest building of model `from` within 80 m.
extern "C" int
revc_test_swap_building(float x, float y, float z, const char *from, const char *to)
{
	int mi1, mi2;
	if(!CModelInfo::GetModelInfo(from, &mi1) || !CModelInfo::GetModelInfo(to, &mi2))
		return 0;
	CVector pos(x, y, z);
	int16 total;
	CEntity *found[16];
	CWorld::FindObjectsOfTypeInRange(mi1, pos, 80.0f, true, &total, 16, found, true, false, false, false, false);
	if(total == 0)
		CWorld::FindObjectsOfTypeInRangeSectorList(mi1, CWorld::GetBigBuildingList(CTheZones::GetLevelFromPosition(&pos)), pos, 80.0f, true, &total, 16, found);
	if(total == 0)
		return 0;
	((CBuilding*)found[0])->ReplaceWithNewModel(mi2);
#ifdef FIX_BUGS
	SwapBuildingLod((CBuilding*)found[0], mi1, mi2);
#endif
	CTheScripts::AddToBuildingSwapArray((CBuilding*)found[0], mi1, mi2);
	// REVC_TEST_SWAP_BACK=1: and undone again, as the script does (the LOD must come back too)
	if(getenv("REVC_TEST_SWAP_BACK"))
		CTheScripts::UndoBuildingSwaps();
	return 1;
}
