// revc (plan task 13h): Xbox skin for the PC menu, with UI STYLE = Xbox.
//
// As the PS2 skin (MenuSkinPS2.cpp): the menu logic, pages and options stay the PC menu's; this
// draws the Xbox pause menu's look: the page title top left and the Vice City logo top right, the
// row of page icons at the bottom (the current one enlarged on a mint box), the pink help line
// under it, items in two columns with the mint selection bar. The sprites are the Xbox's own
// (neo/UserInterface, imported from the player's disc); without an import the PC look stays.
// Positions are measured on native 640x480 xemu captures (docs/CONSOLE_STYLES.md 13h) and given
// in those pixels; the Xbox UI style lays out in 480 lines.
#include "common.h"
#include "Frontend.h"
#include "DMAudio.h"
#include "Font.h"
#include "Sprite2d.h"
#include "Text.h"
#include "Timer.h"
#include "Pad.h"
#include "GenericGameStorage.h"

extern int8 gUiStyle;
extern "C" int revc_import_rgba(int kind, const char *rel, int *width, int *height, unsigned char **rgba);
extern "C" void revc_texmod_free(unsigned char *rgba, int width, int height);
extern "C" ssize_t revc_import_read(int kind, const char *rel, uint8 *buf, size_t cap);
extern "C" int revc_pad_button_name(int pos, char *buf, int len);

static float X(float x) { return SCALE_AND_CENTER_X(x); }
static float Y(float y) { return SCREEN_SCALE_Y(y); }
static float W(float w) { return SCREEN_SCALE_X(w); }
static float H(float h) { return SCREEN_SCALE_Y(h); }

static bool HasSubTabs(int page);
static bool IsTabPage(int page);
void ConsoleAudioRows(bool);	// (core/MenuScreensCustom.cpp)
static bool SettingsSubTabs(int page);
int MenuSkinXboxSubTab(int page);
const CRGBA XBOX_MINT(0, 207, 133, 255);	// (the Xbox's own colours; research on the Xbox build)
const CRGBA XBOX_PINK(255, 150, 225, 255);

// The Xbox pause menu's seven pages in icon order, the PC page shown for each, the icon sprite.
struct XboxTab { int page; int alt; const char *icon; };
static const XboxTab tabs[] = {
	{ MENUPAGE_NEW_GAME, MENUPAGE_CHOOSE_LOAD_SLOT, "ui/r3_menu_238.dds" },	// disk: load ("new game / load game", then the slots)
	{ MENUPAGE_MAP, -1, "ui/r3_menu_241.dds" },			// compass
	{ MENUPAGE_BRIEFS, -1, "ui/r3_menu_243.dds" },		// tape
	{ MENUPAGE_STATS, -1, "ui/r3_menu_246.dds" },			// computer
	{ MENUPAGE_CONSOLE_CONTROLS, MENUPAGE_PAD_PICTURE, "ui/r3_menu_249.dds" },	// pad: the console's controls page (and the controller picture)
	{ MENUPAGE_SOUND_SETTINGS, -1, "ui/r3_menu_252.dds" },	// boombox
	{ MENUPAGE_CONSOLE_DISPLAY, -1, "ui/r3_menu_255.dds" },	// TV: the console's display page
};
// icon centres on the bar (measured: the selection square's centre, which sits on its icon, on
// autopad sets 2-3, 0.5 left of it; uneven spacing as on the console)
static const float iconX[7] = { 112.0f, 176.5f, 258.5f, 323.0f, 395.0f, 466.5f, 530.0f };
static const float ICON_Y = 395.0f, ICON_SIZE = 48.0f, ICON_SELECTED = 80.0f;	// (64 x 0.75 and x 1.25; research on the Xbox build)

// Heap-allocated so nothing runs at program exit (after RenderWare is gone); freed with the menu's
// own textures by MenuSkinXboxUnload.
static CSprite2d *icons, *logoSprite, *glint, *tapeDeck;
// Focus on the icon bar (the Xbox's top level) or inside the page (its list, or the sub-tab bar of
// Brief/Stats). The current icon's green card and the help line's items follow it. Navigation as
// measured on the Xbox (autopad probes in xemu, US Rev 1): see MenuSkinXboxInput.
static bool barFocus = true;
// The page START opens: Load the first time, then the last page used.
static int lastPage = MENUPAGE_NEW_GAME;
#define logo (*logoSprite)
static int spritesState;	// 0 not tried, 1 loaded, -1 no import

static rw::Texture *
ImportTexture(const char *rel)
{
	int w, h;
	unsigned char *px;
	if (!revc_import_rgba(0, rel, &w, &h, &px))
		return nil;
	rw::Image *img = rw::Image::create(w, h, 32);
	img->pixels = px;
	img->stride = w * 4;
	rw::Raster *ras = rw::Raster::create(w, h, 32, rw::Raster::C8888 | rw::Raster::TEXTURE | rw::Raster::MIPMAP | rw::Raster::AUTOMIPMAP);
	rw::Texture *tex = nil;
	if (ras && ras->setFromImage(img)) {
		tex = rw::Texture::create(ras);
		tex->setFilter(rw::Texture::LINEARMIPLINEAR);
	} else if (ras)
		ras->destroy();
	img->pixels = nil;
	img->destroy();
	revc_texmod_free(px, w, h);
	return tex;
}

// The Xbox menu's bitmap fonts (neo/UserInterface/fonts: RWFont, RWSFArborcrestHeavy): a 256 px
// texture and a METRICS1 file with each character's box (code, left, top, right, bottom) in those
// 256 px, so a higher-resolution remake works as fractions of the texture.
struct XboxFont {
	CSprite2d *sprite;
	struct Box { int16 l, t, r, b; } box[256];
	bool ok;
};
static XboxFont fontRW;

static bool
LoadFont(XboxFont &f, const char *tex, const char *met)
{
	f.ok = false;
	if (f.sprite == nil)
		f.sprite = new CSprite2d;
	f.sprite->m_pTexture = ImportTexture(tex);
	static uint8 buf[16384];
	ssize_t len = revc_import_read(0, met, buf, sizeof(buf) - 1);
	if (f.sprite->m_pTexture == nil || len <= 0)
		return false;
	buf[len] = 0;
	memset(f.box, 0, sizeof(f.box));
	char *p = (char *)buf;
	for (int line = 0; *p; line++) {
		char *end = strchr(p, '\n');
		if (end)
			*end = 0;
		int c, l, t, r, b;
		if (line >= 3 && sscanf(p, "%d %d %d %d %d", &c, &l, &t, &r, &b) == 5 && c >= 0 && c < 256)
			f.box[c] = { (int16)l, (int16)t, (int16)r, (int16)b };
		if (!end)
			break;
		p = end + 1;
	}
	f.ok = true;
	return true;
}

// Width of `text` at `height` px (the font's 28 px cells scaled).
static float
XboxFontWidth(const XboxFont &f, const wchar *text, float height, float spacing = 0.0f)
{
	float w = 0.0f, s = height / 28.0f;
	for (; *text; text++) {
		const XboxFont::Box &b = f.box[*text < 256 ? *text : '?'];
		w += (b.r - b.l + 1) * s + spacing;
	}
	return w;
}

static void
XboxFontPrint(const XboxFont &f, float x, float y, float height, const wchar *text, const CRGBA &col, float spacing = 0.0f)
{
	if (!f.ok)
		return;
	float s = height / 28.0f;
	for (; *text; text++) {
		const XboxFont::Box &b = f.box[*text < 256 ? *text : '?'];
		float w = (b.r - b.l + 1) * s;
		if (b.r > b.l && *text != ' ') {
			float u0 = b.l / 256.0f, u1 = (b.r + 1) / 256.0f, v0 = b.t / 256.0f, v1 = (b.b + 1) / 256.0f;
			f.sprite->Draw(CRect(x, y, x + w, y + (b.b - b.t + 1) * s), col, u0, v0, u1, v0, u0, v1, u1, v1);
		}
		x += w + spacing;
	}
}

// Text in the Xbox menu font for other menu code (the controller page's callouts): align 0 left,
// 1 right of x, with a dark drop shadow; 2 left without one (the slot lists' black rows). False
// when the font isn't loaded (then draw as before).
bool
MenuSkinXboxText(float x, float y, float height, const wchar *text, const CRGBA &col, int align)
{
	if (!fontRW.ok)
		return false;
	if (align == 1)
		x -= XboxFontWidth(fontRW, text, height);
	float d = height / 10.0f;
	if (align != 2)
		XboxFontPrint(fontRW, x + d, y + d, height, text, CRGBA(0, 0, 0, col.a));
	XboxFontPrint(fontRW, x, y, height, text, col);
	return true;
}

float
MenuSkinXboxTextWidth(const wchar *text, float height)
{
	return fontRW.ok ? XboxFontWidth(fontRW, text, height) : 0.0f;
}

static bool
LoadSprites(void)
{
	if (spritesState == 0) {
		spritesState = -1;
		if (icons == nil) {
			icons = new CSprite2d[7];
			logoSprite = new CSprite2d;
			glint = new CSprite2d[2];
			tapeDeck = new CSprite2d;
		}
		bool ok = true;
		for (int i = 0; i < 7; i++)
			ok = ok && (icons[i].m_pTexture = ImportTexture(tabs[i].icon)) != nil;
		logo.m_pTexture = ImportTexture("ui/r3_menu_228.dds");
		glint[0].m_pTexture = ImportTexture("ui/r3_menu_230.dds");
		glint[1].m_pTexture = ImportTexture("ui/r3_menu_232.dds");
		tapeDeck->m_pTexture = ImportTexture("ui/r3_menu_616.dds");	// the user's music in the radio strip
		LoadFont(fontRW, "ui/fonts/rwsfarborcrestheavy.dds", "ui/fonts/rwsfarborcrestheavy.met");	// the help line's font
		if (ok && logo.m_pTexture)
			spritesState = 1;
	}
	return spritesState == 1;
}

// Called from CMenuManager::UnloadTextures: frees the sprites; they load again when needed.
void
MenuSkinXboxUnload(void)
{
	if (icons == nil)
		return;
	for (int i = 0; i < 7; i++)
		icons[i].Delete();
	logo.Delete();
	glint[0].Delete();
	glint[1].Delete();
	tapeDeck->Delete();
	if (fontRW.sprite)
		fontRW.sprite->Delete();
	fontRW.ok = false;
	spritesState = 0;
}

// The radio strip's last icon, the user's music: the Xbox's stack of tapes (r3_menu_616). False
// without it (the caller draws the PC's MP3 logo).
bool
MenuSkinXboxTapeDeck(const CRect &r, const CRGBA &col)
{
	if (tapeDeck == nil || tapeDeck->m_pTexture == nil)
		return false;
	tapeDeck->Draw(r, col);
	return true;
}

// The confirmation dialogs reached from the Load page: drawn as the Xbox's layered blue dialog
// over the dimmed Load page.
bool
MenuSkinXboxDialog(int page)
{
	return page == MENUPAGE_NEW_GAME_RELOAD || page == MENUPAGE_LOAD_SLOT_CONFIRM || page == MENUPAGE_DELETE_SLOT_CONFIRM ||
		page == MENUPAGE_EXIT;	// (revc: the quit dialog, from "quit game" on the Load page)
}

// Behind a dialog the console dims the title, logo, icon bar and help line too (to about 45 %).
static uint8
DialogDim(uint8 alpha)
{
	return MenuSkinXboxDialog(FrontEndMenuManager.m_nCurrScreen) ? alpha * 45 / 100 : alpha;
}

// The "advanced" sub-tab (MENUPAGE_CONSOLE_OPTIONS) belongs to the page it was opened from.
static int optionsOrigin = MENUPAGE_CONSOLE_DISPLAY;
// Focus on the sub-tab bar of Display / Audio / Controls (down from their last row; up leaves).
static bool subBarFocus;

static int
CurrentTab(int page)
{
	if (MenuSkinXboxDialog(page))
		return 0;
	if (page == MENUPAGE_CONSOLE_OPTIONS)
		page = optionsOrigin;
	for (int i = 0; i < (int)ARRAY_SIZE(tabs); i++)
		if (tabs[i].page == page || tabs[i].alt == page)
			return i;
	return -1;
}

// The Xbox page (icon) a menu page belongs to, -1 if none.
int
MenuSkinXboxTab(int page)
{
	return CurrentTab(page);
}

// Whether `page` is one of the seven icon-bar pages, or the Load tab's slot list (the controller
// picture, the Controls tab's other page, is a sub-page: it opens in one frame).
bool
MenuSkinXboxIsTabPage(int page)
{
	for (int i = 0; i < (int)ARRAY_SIZE(tabs); i++)
		if (tabs[i].page == page || (tabs[i].alt == page && page == MENUPAGE_CHOOSE_LOAD_SLOT))
			return true;
	return false;
}

// Whether the skin draws on this page: Xbox UI style, a game running, a page the Xbox pause menu
// has, and the Xbox menu sprites imported.
bool
MenuSkinXboxActive(int page, bool gameNotLoaded)
{
	static bool test = getenv("REVC_TEST_MENU_SKIN") != nil;
	return gUiStyle == 2 && (!gameNotLoaded || test) && CurrentTab(page) >= 0 && LoadSprites();
}

// The page title, top left in the menu's heading font with its dark shadow (instead of the PC's
// top right).
void
MenuSkinXboxTitle(wchar *title, uint8 alpha)
{
	// the Xbox's own names for these pages
	int page = FrontEndMenuManager.m_nCurrScreen;
	if (page == MENUPAGE_CONSOLE_OPTIONS)	// "advanced": the title of the page it belongs to
		page = optionsOrigin, title = TheText.Get(aScreens[optionsOrigin].m_ScreenName);
	if (page == MENUPAGE_CHOOSE_LOAD_SLOT || page == MENUPAGE_NEW_GAME || MenuSkinXboxDialog(page))
		title = TheText.Get("RVC_XTL");
	else if (page == MENUPAGE_CONTROLLER_PC)
		title = TheText.Get("RVC_XTC");
	else if (page == MENUPAGE_PAD_PICTURE)
		title = TheText.Get("RVC_XTD");
	alpha = DialogDim(alpha);
	CFont::SetRightJustifyOff();
	CFont::SetCentreOff();
	// the black copy under it: +4,+4 (Load +3.6,+4.0; Display +3.8,+3.8; research on the Xbox build)
	const bool load = page == MENUPAGE_CHOOSE_LOAD_SLOT || page == MENUPAGE_NEW_GAME || MenuSkinXboxDialog(page);
	const float sx = load ? 3.6f : page == MENUPAGE_CONSOLE_DISPLAY ? 3.8f : 4.0f, sy = page == MENUPAGE_CONSOLE_DISPLAY ? 3.8f : 4.0f;
	CFont::SetColor(CRGBA(0, 0, 0, alpha));
	CFont::PrintString(X(52.0f) + W(sx), Y(48.0f) + H(sy), title);
	CFont::SetColor(CRGBA(XBOX_PINK.r, XBOX_PINK.g, XBOX_PINK.b, alpha));
	CFont::PrintString(X(52.0f), Y(48.0f), title);
}

// The Vice City logo, top right (instead of the PC's top left), with the Xbox's glint: a white
// flash over "city" (r3_menu_230) then over "Vice" (232), 200 ms each from 3.4 s after the page
// started, repeating every 3.8 s (R3_Menu.anm anim 4; positions from the menu video).
// Test: draw the glint stage `stage` (1 or 2) whatever the time (REVC_TEST_GLINT).
static int TestGlint(void) { static int g = getenv("REVC_TEST_GLINT") ? atoi(getenv("REVC_TEST_GLINT")) : 0; return g; }
static int glintPage = -1;
static uint32 glintStart;

void
MenuSkinXboxLogo(uint8 alpha)
{
	alpha = DialogDim(alpha);
	const float x0 = X(458.0f), y0 = Y(36.0f), w = W(134.0f), h = H(67.0f);
	logo.Draw(CRect(x0, y0, x0 + w, y0 + h), CRGBA(255, 255, 255, alpha));
	uint32 now = CTimer::GetTimeInMillisecondsPauseMode();
	if (FrontEndMenuManager.m_nCurrScreen != glintPage) {
		glintPage = FrontEndMenuManager.m_nCurrScreen;
		glintStart = now;
	}
	uint32 t = (now - glintStart) % 3800;
	if (TestGlint())
		t = TestGlint() == 1 ? 3500 : 3700;
	if (t >= 3400 && t < 3600 && glint[0].m_pTexture)
		glint[0].Draw(CRect(x0 + 0.46f * w, y0 + 0.12f * h, x0 + 0.88f * w, y0 + 1.0f * h), CRGBA(255, 255, 255, alpha));
	else if (t >= 3600 && glint[1].m_pTexture)
		glint[1].Draw(CRect(x0 + 0.08f * w, y0 + 0.22f * h, x0 + 0.50f * w, y0 + 0.92f * h), CRGBA(255, 255, 255, alpha));
}


static void
DrawIconBar(CMenuManager *m, uint8 alpha)
{
	int cur = CurrentTab(m->m_nCurrScreen);
	for (int i = 0; i < 7; i++) {
		if (i == cur)
			continue;
		float s = ICON_SIZE;
		// the other pages' icons at half colour (research on the Xbox build)
		icons[i].Draw(CRect(X(iconX[i]) - W(s / 2), Y(ICON_Y) - H(s / 2), X(iconX[i]) + W(s / 2), Y(ICON_Y) + H(s / 2)), CRGBA(128, 128, 128, alpha));
	}
	// a page change (Xbox autopad set 2, Brief -> Stats): the mint box moves to the new icon at
	// once while the old icon stays enlarged and white and the new one stays small for one 30 Hz
	// game frame; then they swap sizes in one step
	static int prevIcon = -1, lastIcon = -1;
	static uint32 iconAt;
	uint32 now = CTimer::GetTimeInMillisecondsPauseMode();
	if (cur != lastIcon) {
		prevIcon = lastIcon;
		lastIcon = cur;
		iconAt = now;
	}
	bool swapped = now - iconAt >= 33;
	if (cur >= 0) {
		// the current page's icon, enlarged on the mint box (measured 482..580 x 368..425 for the TV)
		float cx = iconX[cur], s = swapped || prevIcon < 0 ? ICON_SELECTED : ICON_SIZE;
		if (barFocus)	// the green card only while the icon bar has focus
			CSprite2d::Draw2DPolygon(X(cx - 48.0f), Y(370.0f), X(cx + 50.0f), Y(367.0f), X(cx - 46.0f), Y(426.0f), X(cx + 48.0f), Y(424.0f),
				CRGBA(XBOX_MINT.r, XBOX_MINT.g, XBOX_MINT.b, alpha));
		icons[cur].Draw(CRect(X(cx) - W(s / 2), Y(ICON_Y + 2.0f) - H(s / 2), X(cx) + W(s / 2), Y(ICON_Y + 2.0f) + H(s / 2)), CRGBA(255, 255, 255, alpha));
	}
	if (prevIcon >= 0 && prevIcon != cur && !swapped) {	// (over the box's edge)
		float px = iconX[prevIcon], ps = ICON_SELECTED;
		icons[prevIcon].Draw(CRect(X(px) - W(ps / 2), Y(ICON_Y + 2.0f) - H(ps / 2), X(px) + W(ps / 2), Y(ICON_Y + 2.0f) + H(ps / 2)), CRGBA(255, 255, 255, alpha));
	}
}

// "START:resume game   <-/->:select   A:select   B:back" in pink under the icons, all in the
// Xbox's RWSFArborcrestHeavy, 17 px (word widths ~1.15x the 15 px cell, capitals at y 431-438;
// autopad set 2): the buttons as words (START, A, B, Y,
// thumbsticks), the d-pad as solid arrows about 10 x 7 px centred at y 434.5, 2 px in (autopad sets 1-3;
// the font has only a thin right arrow, so these are drawn).
// `x` in screen pixels (its left edge), the rest in 640x480 menu units.
static void
HelpArrow(float x, float cy, int dir, const CRGBA &col)
{
	// dir 0 left, 1 right, 2 up, 3 down; 10 long, the head 4 long and 7 wide, the shaft 4 wide
	const float L = 10.0f, hl = 4.0f, hw = 3.5f, sw = 2.0f;
	auto P = [&](float along, float across, float &px, float &py) {	// along: from the tip
		float u, v;
		switch (dir) {
		case 0: u = along; v = across; break;
		case 1: u = L - along; v = across; break;
		case 2: u = 3.0f + across; v = -5.0f + along; break;
		default: u = 3.0f + across; v = 5.0f - along; break;
		}
		px = x + W(u);
		py = Y(cy + v);
	};
	float ax, ay, bx, by, cx, cyy, dx, dy;
	P(0.0f, 0.0f, ax, ay); P(hl, -hw, bx, by); P(hl, hw, cx, cyy);
	CSprite2d::Draw2DPolygon(ax, ay, bx, by, ax, ay, cx, cyy, col);
	P(hl - 0.5f, -sw, ax, ay); P(L, -sw, bx, by); P(hl - 0.5f, sw, cx, cyy); P(L, sw, dx, dy);
	CSprite2d::Draw2DPolygon(ax, ay, bx, by, cx, cyy, dx, dy, col);
}

// A line from (x0, y0) to (x1, y1), `t` thick (x in screen pixels, y and t in menu units).
static void
HelpLine(float x0, float y0, float x1, float y1, float t, const CRGBA &col)
{
	float lx = (x1 - x0) / W(1.0f), ly = y1 - y0, l = Sqrt(lx * lx + ly * ly);	// (in menu units)
	if (l <= 0.0f)
		return;
	float nx = -ly / l * t / 2.0f, ny = lx / l * t / 2.0f;
	CSprite2d::Draw2DPolygon(x0 + W(nx), Y(y0 + ny), x1 + W(nx), Y(y1 + ny), x0 - W(nx), Y(y0 - ny), x1 - W(nx), Y(y1 - ny), col);
}

// A PlayStation face symbol for a pad of that family, monochrome in the line's pink at cap height
// (8 px), as the research's design rules put it (no circle around it): `x` its left edge in
// screen pixels; returns the advance.
static float
HelpShape(float x, float cy, const char *name, const CRGBA &col)
{
	const float h = 8.0f, t = 1.6f;
	float x0 = x, x1 = x + W(h), y0 = cy - h / 2.0f, y1 = cy + h / 2.0f;
	if (strcmp(name, "cross") == 0) {
		HelpLine(x0, y0, x1, y1, t, col);
		HelpLine(x0, y1, x1, y0, t, col);
	} else if (strcmp(name, "square") == 0) {
		HelpLine(x0, y0, x1, y0, t, col);
		HelpLine(x1, y0, x1, y1, t, col);
		HelpLine(x1, y1, x0, y1, t, col);
		HelpLine(x0, y1, x0, y0, t, col);
	} else if (strcmp(name, "triangle") == 0) {
		float xm = (x0 + x1) / 2.0f;
		HelpLine(xm, y0, x1, y1, t, col);
		HelpLine(x1, y1, x0, y1, t, col);
		HelpLine(x0, y1, xm, y0, t, col);
	} else {	// circle
		float xm = (x0 + x1) / 2.0f, r = h / 2.0f;
		for (int k = 0; k < 12; k++) {
			float a0 = k * 6.2831853f / 12.0f, a1 = (k + 1) * 6.2831853f / 12.0f;
			HelpLine(xm + W(r * Cos(a0)), cy + r * Sin(a0), xm + W(r * Cos(a1)), cy + r * Sin(a1), t, col);
		}
	}
	return W(h + 3.0f);
}

// One help-line item: the buttons (up to two, joined by "/"), ":" and the word; returns the end x.
// The buttons are named as on the pad in use (revc_pad_button_name: "A" on an Xbox pad, the
// shapes on a PlayStation one, "+" for a Switch's start); without a pad, the Xbox's own words.
static float
HelpItem(float x, float y, const int *glyphs, int ng, const char *pre, const char *word, uint8 alpha)
{
	const float th = H(17.0f), sp = W(0.7f);	// (letters ~0.7 px further apart than the font's boxes)
	CRGBA pink(XBOX_PINK.r, XBOX_PINK.g, XBOX_PINK.b, alpha);
	wchar buf[64];
	auto text = [&](const char *t) {
		AsciiToUnicode(t, buf);
		if (fontRW.ok) {
			XboxFontPrint(fontRW, x + W(1.0f), y + H(1.0f), th, buf, CRGBA(0, 0, 0, alpha), sp);	// (its black shadow, +1,+1)
			XboxFontPrint(fontRW, x, y, th, buf, pink, sp);
			x += XboxFontWidth(fontRW, buf, th, sp);
		}
	};
	for (int k = 0; k < ng; k++) {
		if (k > 0)
			text("/");
		int dir = -1, pos = -1;
		const char *name = nil;
		switch (glyphs[k]) {
		case BUTTON_DPAD_LEFT: dir = 0; break;
		case BUTTON_DPAD_RIGHT: dir = 1; break;
		case BUTTON_DPAD_UP: dir = 2; break;
		case BUTTON_DPAD_DOWN: dir = 3; break;
		case BUTTON_START: name = "START"; pos = 4; break;
		case BUTTON_CROSS: name = "A"; pos = 0; break;
		case BUTTON_CIRCLE: name = "B"; pos = 1; break;
		case BUTTON_SQUARE: name = "X"; pos = 2; break;
		case BUTTON_TRIANGLE: name = "Y"; pos = 3; break;
		default: name = "thumbsticks"; k = ng; break;	// (the map's two sticks: one word)
		}
		static char padName[16];
		if (pos >= 0 && revc_pad_button_name(pos, padName, sizeof(padName)) > 0)
			name = padName;
		if (dir >= 0) {
			HelpArrow(x + W(2.0f), 434.5f, dir, pink);
			x += W(dir < 2 ? 12.0f : 9.0f);
		} else if (strcmp(name, "cross") == 0 || strcmp(name, "circle") == 0 || strcmp(name, "square") == 0 || strcmp(name, "triangle") == 0)
			x += HelpShape(x + W(1.0f), 434.5f, name, pink);
		else
			text(name);
	}
	int n = 0;
	if (pre) {
		AsciiToUnicode(pre, buf);
		n = UnicodeStrlen(buf);
	}
	buf[n++] = ':';
	UnicodeStrcpy(buf + n, TheText.Get(word));
	if (fontRW.ok)
		XboxFontPrint(fontRW, x, y, th, buf, pink, sp);
	return x + (fontRW.ok ? XboxFontWidth(fontRW, buf, th, sp) : 0.0f);
}

// The help line under the icons, per state as on the Xbox (measured slots at x 52, 200, 340,
// 450): top level "START resume game, <-/-> select, A select"; in a list "START resume game, up/down
// select, A select, B back"; display controls "START resume game, <-/-> change, B back"; the map
// "A select, B back, thumbsticks move/zoom". The pad's glyphs, the text in RWSFArborcrestHeavy.
static void
DrawHelpLine(CMenuManager *m, uint8 alpha)
{
	const float y = Y(424.2f);
	// the Xbox's slots (x 52, 202, 338.95, 459.95, 497.95; the Map's legend 151.95, 255.95, 339.95;
	// research on the Xbox build), each pushed right past a longer item before it (another pad's
	// names: "OPTIONS:resume game")
	float last = 0.0f;
	auto slot = [&](float x) { return Max(X(x), last + W(8.0f)); };
	static const int start[1] = { BUTTON_START }, lr[2] = { BUTTON_DPAD_LEFT, BUTTON_DPAD_RIGHT }, ud[2] = { BUTTON_DPAD_UP, BUTTON_DPAD_DOWN };
	static const int a[1] = { BUTTON_CROSS }, b[1] = { BUTTON_CIRCLE }, sticks[2] = { BUTTON_UP, BUTTON_RSTICK_UP };
	if (m->m_nCurrScreen == MENUPAGE_MAP) {
		last = HelpItem(slot(151.95f), y, a, 1, nil, "RVC_XHS", alpha);
		last = HelpItem(slot(255.95f), y, b, 1, nil, "RVC_XHB", alpha);
		last = HelpItem(slot(339.95f), y, sticks, 2, nil, "RVC_XHM", alpha);
		return;
	}
	last = HelpItem(slot(52.0f), y, start, 1, nil, "RVC_XHR", alpha);
	if (HasSubTabs(m->m_nCurrScreen) && !(barFocus && IsTabPage(m->m_nCurrScreen)) && (!SettingsSubTabs(m->m_nCurrScreen) || subBarFocus)) {	// "START:resume game  <-/->:select  B:back" (the sub-tabs)
		last = HelpItem(slot(202.0f), y, lr, 2, nil, "RVC_XHS", alpha);
		last = HelpItem(slot(338.9f), y, b, 1, nil, "RVC_XHB", alpha);
		return;
	}
	if (m->m_nCurrScreen == MENUPAGE_CONSOLE_OPTIONS) {	// "START:resume game  up/down:select  <-/->:change  B:back"
		last = HelpItem(slot(202.0f), y, ud, 2, nil, "RVC_XHS", alpha);
		last = HelpItem(slot(338.95f), y, lr, 2, nil, "RVC_XHC", alpha);
		last = HelpItem(slot(459.95f), y, b, 1, nil, "RVC_XHB", alpha);
		return;
	}
	if (m->m_nCurrScreen == MENUPAGE_PAD_PICTURE) {
		last = HelpItem(slot(202.95f), y, lr, 2, nil, "RVC_XHC", alpha);
		last = HelpItem(slot(459.95f), y, b, 1, nil, "RVC_XHB", alpha);
		return;
	}
	bool bar = barFocus && !MenuSkinXboxDialog(m->m_nCurrScreen);	// a dialog is a list
	// (rows changed with left/right, on Display, Audio and Controls: "<-/->:change" where a list
	// says "A:select")
	int action = aScreens[m->m_nCurrScreen].m_aEntries[m->m_nCurrOption].m_Action;
	bool lrRow = !bar && (m->m_nCurrScreen == MENUPAGE_CONSOLE_DISPLAY || m->m_nCurrScreen == MENUPAGE_SOUND_SETTINGS || m->m_nCurrScreen == MENUPAGE_CONSOLE_CONTROLS) &&
		action != MENUACTION_CHANGEMENU && action != MENUACTION_GOBACK && action != MENUACTION_CFO_DYNAMIC;
	last = HelpItem(slot(202.0f), y, bar ? lr : ud, 2, nil, "RVC_XHS", alpha);
	if (lrRow)
		last = HelpItem(slot(338.95f), y, lr, 2, nil, "RVC_XHC", alpha);
	else
		last = HelpItem(slot(338.95f), y, a, 1, nil, "RVC_XHS", alpha);
	if (!bar)
		last = HelpItem(slot(459.95f), y, b, 1, nil, "RVC_XHB", alpha);
	if (SettingsSubTabs(m->m_nCurrScreen)) {	// "Y:advanced" (the face-button item grammar of "X:delete")
		static const int yb[1] = { BUTTON_TRIANGLE };
		last = HelpItem(slot(497.95f), y, yb, 1, nil, "RVC_XHA", alpha);
	}
}

// The selection bar's five hand-drawn outlines (R3_Menu.anm shapes 0-4, read from the player's
// imported disc): plain quads in pixels around the bar's origin. The console steps to the next one
// each time the selection moves. Path chunks: id 0x1A1, version 0x1003FFFF, the size counting the
// 12-byte header; body: closed, node count (5), 3 more words, then per node x, y, nx, ny, dist
// (floats) and flags; node 5 repeats node 1.
static float barShape[5][4][2];
static int barShapes = -1;	// -1 not read yet, 0 none
static int barShapeIndex = 1;	// (shape 1 after the game loads; research on the Xbox build)

static void
ReadBarShapes(void)
{
	barShapes = 0;
	static uint8 *buf;
	const size_t cap = 2 * 1024 * 1024;
	if (buf == nil)
		buf = new uint8[cap];
	ssize_t len = revc_import_read(0, "ui/r3_menu.anm", buf, cap);
	if (len <= 0)
		return;
	auto u32 = [&](ssize_t o) { uint32 v; memcpy(&v, buf + o, 4); return v; };
	auto f32 = [&](ssize_t o) { float v; memcpy(&v, buf + o, 4); return v; };
	for (ssize_t o = 0; o + 12 <= len && barShapes < 5; o += 4) {
		uint32 size = u32(o + 4);
		if (u32(o) != 0x1A1 || u32(o + 8) != 0x1003FFFF || size == 0 || size >= 0x10000 || o + (ssize_t)size > len)
			continue;
		ssize_t b = o + 12;
		if (u32(b + 4) != 5 || b + 20 + 5 * 24 > len)
			continue;
		for (int k = 0; k < 4; k++) {
			barShape[barShapes][k][0] = f32(b + 20 + k * 24);
			barShape[barShapes][k][1] = f32(b + 20 + k * 24 + 4);
		}
		barShapes++;
		o += size - 4;
	}
	if (barShapes < 5)
		barShapes = 0;	// all five or none
}

// The selection (page, row, sub-tab) as last drawn; a change steps the bar's outline and restarts
// the pulse (selectedAt), as the Xbox's states that move the selection do. The bar jumps to the new
// row at once and swaps to the next outline 50 ms later (research, autopad
// xbox_row_controls_slow: jump at 4015, next outline at 4018).
static int selScreen = -1, selOption = -1, selSub = -1;
static uint32 selectedAt;
static bool outlinePending, outlineReset;
static void
NoteSelection(void)
{
	int screen = FrontEndMenuManager.m_nCurrScreen, option = FrontEndMenuManager.m_nCurrOption;
	int sub = HasSubTabs(screen) ? MenuSkinXboxSubTab(screen) : -1;
	uint32 now = CTimer::GetTimeInMillisecondsPauseMode();
	if (outlinePending && now - selectedAt >= 50) {	// (3 frames at 60 Hz: autopad 4015 -> 4018)
		// entering Map, Brief or Display starts again at shape 1, any other move takes the next
		// (the menu animation's states; research on the Xbox build, autopad set 2)
		barShapeIndex = outlineReset ? 1 : (barShapeIndex + 1) % 5;
		outlinePending = false;
	}
	if (screen == selScreen && option == selOption && sub == selSub)
		return;
	if (selScreen >= 0) {
		outlinePending = true;
		outlineReset = screen != selScreen && (screen == MENUPAGE_MAP || screen == MENUPAGE_BRIEFS || screen == MENUPAGE_CONSOLE_DISPLAY);
	}
	selScreen = screen;
	selOption = option;
	selSub = sub;
	selectedAt = now;
}

// Draws the current outline over x0..x1 x y0..y1 (the quad's extent mapped onto the bar's box).
// native: x0..x1 x y0..y1 is the box of outline 0 at the bar's scale (the Xbox's data gives its bars
// so); the current outline is drawn at its own shape and that scale about the same centre, as the
// Xbox draws it (its outlines differ in size: 36-43 px tall at scale 1).
static bool
DrawBarShape(float x0, float y0, float x1, float y1, const CRGBA &col, bool advance = true, int shape = -1, bool native = false)
{
	if (barShapes < 0)
		ReadBarShapes();
	if (barShapes == 0)
		return false;
	// the selection moved: the next outline (0 -> 4, then again). Only a move steps it, never a
	// left/right value change, which leaves the bar as it is (Xbox autopad set 1, radio left/right:
	// the corners fixed within 1 px; R3_Menu.anm anim 3 just shows the next shape).
	if (advance)
		NoteSelection();
	const float (*q)[2] = barShape[shape >= 0 ? shape : barShapeIndex];
	float minx = q[0][0], maxx = q[0][0], miny = q[0][1], maxy = q[0][1];
	for (int k = 1; k < 4; k++) {
		minx = Min(minx, q[k][0]); maxx = Max(maxx, q[k][0]);
		miny = Min(miny, q[k][1]); maxy = Max(maxy, q[k][1]);
	}
	float px[4], py[4];
	if (native) {
		// outline 0 spans x -61.8..69.35, y -17.9..18.9 (y up) at scale 1
		const float sx = (x1 - x0) / 131.15f, sy = (y1 - y0) / 36.8f, cx = x0 + 61.8f * sx, cy = y0 + 18.9f * sy;
		for (int k = 0; k < 4; k++) {
			px[k] = cx + q[k][0] * sx;
			py[k] = cy - q[k][1] * sy;
		}
	} else
	for (int k = 0; k < 4; k++) {
		px[k] = x0 + (q[k][0] - minx) / (maxx - minx) * (x1 - x0);
		py[k] = y0 + (maxy - q[k][1]) / (maxy - miny) * (y1 - y0);	// (the shapes' y points up; research, captures)
	}
	// the corners by quadrant (the five shapes list their nodes in different orders)
	int tl = 0, tr = 0, bl = 0, br = 0;
	for (int k = 0; k < 4; k++) {
		bool left = q[k][0] < 0.0f, top = q[k][1] > 0.0f;
		(left ? (top ? tl : bl) : (top ? tr : br)) = k;
	}
	CSprite2d::Draw2DPolygon(px[tl], py[tl], px[tr], py[tr], px[bl], py[bl], px[br], py[br], col);
	return true;
}

// The mint selection bar behind the current item, with the Xbox's pulse: a white glow over it
// growing from 0.9x to 1.157 x 1.081 while fading from 0.5 to 0.059 alpha, every 0.8 s
// (R3_Menu.anm, decoded; docs/CONSOLE_STYLES.md 13h).
static bool selectionNative;	// (MenuSkinXboxFixedBar)

void
MenuSkinXboxSelection(float x0, float y0, float x1, float y1, uint8 alpha, bool tail, float minWidth)
{
	// Tilted with the panel (top edge leaning right), from the label's left edge to at least a
	// fixed width, the last part a darker mint tail (measured on the Xbox's list pages).
	x1 = Max(x1, x0 + W(minWidth));	// (dialog buttons: tight, minWidth 0)
	if (!tail) {	// whole-row bars (the radio station): the outline's own height, ~39 px, centred
		float cy = (y0 + y1) / 2.0f, hh = H(39.0f * 448.0f / 480.0f) / 2.0f;
		y0 = cy - hh;
		y1 = cy + hh;
	}
	float tilt = W(5.0f), tailW = W(16.0f);
	// the pulse (R3_Menu.anm anims 1, 2, 16...): a white copy of the current outline scaled about
	// the bar's centre while it fades, 5 keyframes 0.2 s apart, looping every 0.8 s, in 30 Hz steps,
	// from the last selection move (value changes don't restart it; autopad set 1).
	// Behind the bar, so it shows mostly past the bar's ends (x up to 1.157, y only 1.081).
	static const float key[5][3] = { { 0.900f, 0.900f, 0.500f }, { 0.940f, 0.928f, 0.430f }, { 0.996f, 0.968f, 0.336f },
		{ 1.069f, 1.019f, 0.211f }, { 1.157f, 1.081f, 0.059f } };
	NoteSelection();
	float t = (((CTimer::GetTimeInMillisecondsPauseMode() - selectedAt) % 800) * 3 / 100) / 30.0f;	// 0..0.8 s
	int k = Min((int)(t / 0.2f), 3);
	float f = Min((t - k * 0.2f) / 0.2f, 1.0f);
	float sx = key[k][0] + (key[k + 1][0] - key[k][0]) * f, sy = key[k][1] + (key[k + 1][1] - key[k][1]) * f;
	float a = key[k][2] + (key[k + 1][2] - key[k][2]) * f;
	{
		// centred on the whole bar (the anim's target position is the bar's own)
		float cx = (x0 + x1) / 2.0f, cy = (y0 + y1) / 2.0f, hw = (x1 - x0) / 2.0f * sx, hh = (y1 - y0) / 2.0f * sy;
		CRGBA glow(255, 255, 255, (uint8)(a * alpha));
		if (!DrawBarShape(cx - hw, cy - hh, cx + hw, cy + hh, glow, false, 0))	// (the glow: always shape 0)
			CSprite2d::DrawRect(CRect(cx - hw, cy - hh, cx + hw, cy + hh), glow);
	}
	// The bar: the console's own outline (colour from the shape's brush: 0, 207, 133), or a tilted
	// quad without the import. No end block: what looked like a lighter tail on the console is the
	// pulse's halo past the bar's end (round 25); the sub-tab bars draw their own block.
	(void)tailW;
	if (!DrawBarShape(x0, y0, x1, y1, CRGBA(0, 207, 133, alpha), true, -1, selectionNative))
		CSprite2d::Draw2DPolygon(x0 + tilt, y0, x1 + tilt, y0, x0, y1, x1, y1, CRGBA(XBOX_MINT.r, XBOX_MINT.g, XBOX_MINT.b, alpha));
}

// The Xbox's save screen (a screen of its own, not one of the pause menu's pages; research on the
// Xbox build): the game behind dimmed to black, the light-blue card (the Load card's shape, moved),
// "cancel" at its top left (the selection when it opens) and the eight slots in black. No title,
// logo, icon bar or help line. Overwriting a save asks in the Xbox's OK / Cancel dialog over it;
// while saving, the black only (the message is the Xbox's dialog, CMenuManager::DrawOverlays).
// A save's date as the Xbox writes it: "Nov 10 2026, 10:07:06PM" from the menu's own
// "10 Nov 2026 22:07:06" (research on the Xbox build, the console's Load page); unchanged if it
// doesn't read as that.
const wchar *
MenuSkinXboxDate(const wchar *pc)
{
	static wchar out[48];
	char a[48], month[16], buf[48];
	int n = 0;
	for (; pc[n] && n < 47; n++)
		a[n] = pc[n] < 128 ? (char)pc[n] : '?';
	a[n] = '\0';
	int day, year, h, m, sec;
	if (sscanf(a, "%d %15s %d %d:%d:%d", &day, month, &year, &h, &m, &sec) != 6)
		return pc;
	snprintf(buf, sizeof(buf), "%s %02d %04d, %d:%02d:%02d%s", month, day, year, h % 12 == 0 ? 12 : h % 12, m, sec, h < 12 ? "AM" : "PM");
	AsciiToUnicode(buf, out);
	return out;
}

void MenuSkinXboxDialogBack(uint8 alpha);
void MenuSkinXboxFixedBar(float x0, float y0, float x1, float y1, uint8 alpha);

bool
MenuSkinXboxSave(int page, bool gameNotLoaded)
{
	static bool test = getenv("REVC_TEST_MENU_SKIN") != nil;
	return gUiStyle == 2 && (!gameNotLoaded || test) && LoadSprites() &&
		(page == MENUPAGE_CHOOSE_SAVE_SLOT || page == MENUPAGE_SAVE_OVERWRITE_CONFIRM || page == MENUPAGE_SAVING_IN_PROGRESS);
}

void
MenuSkinXboxSaveScreen(CMenuManager *m, uint8 alpha)
{
	CSprite2d::DrawRect(CRect(0.0f, 0.0f, SCREEN_WIDTH, SCREEN_HEIGHT), CRGBA(0, 0, 0, 255));
	const int page = m->m_nCurrScreen;
	if (page == MENUPAGE_SAVING_IN_PROGRESS)
		return;
	const bool confirm = page == MENUPAGE_SAVE_OVERWRITE_CONFIRM;
	// (corners TL, TR, BL, BR; 0.6 alpha in the animation data, but it shows at 0.705 over the black
	// on the console's capture, measured against the opaque dialog card)
	CSprite2d::Draw2DPolygon(X(142.45f), Y(89.15f), X(479.4f), Y(102.15f), X(156.45f), Y(389.15f), X(498.4f), Y(400.15f),
		CRGBA(97, 194, 247, alpha * 180 / 255));
	const auto &s = aScreens[MENUPAGE_CHOOSE_SAVE_SLOT];
	for (int i = 0; i < NUM_MENUROWS; i++) {
		const int slot = s.m_aEntries[i].m_SaveSlot;
		const bool sel = confirm ? slot == SAVESLOT_1 + m->m_nCurrSaveSlot : i == m->m_nCurrOption;
		if (slot >= SAVESLOT_1 && slot <= SAVESLOT_8) {
			// rows 35.15 apart, the name in black with its date under it, the mint bar (2.5 x the
			// outline) at the Xbox's own centre for each row
			const int k = slot - SAVESLOT_1;
			static const float barCY[8] = { 122.95f, 156.95f, 192.95f, 226.95f, 260.95f, 297.25f, 332.55f, 367.7f };
			if (sel) {
				const float dx = k == 6 ? 1.0f : 0.0f;
				MenuSkinXboxFixedBar(X(157.55f + dx), Y(barCY[k] - 18.9f), X(485.43f + dx), Y(barCY[k] + 17.9f), alpha);
			}
			static wchar name[96], date[40];
			const wchar *l = nil, *r = nil;
			if (Slots[k] == SLOT_OK) {
				// (the Xbox lists a save as "1. In The Beginning...", the console's Load page)
				static wchar numbered[104];
				char num[8];
				sprintf(num, "%d. ", k + 1);
				AsciiToUnicode(num, numbered);
				const wchar *nm = GetNameOfSavedGame(k);
				int o = UnicodeStrlen(numbered);
				for (int c = 0; nm && nm[c] && o < 102; c++)
					numbered[o++] = nm[c];
				numbered[o] = 0;
				l = nm && nm[0] ? numbered : nil;
				r = MenuSkinXboxDate(GetSavedGameDateAndTime(k));
			}
			if (!l || l[0] == '\0') {
				UnicodeStrcpy(name, TheText.Get("RVC_XSG"));
				char num[8];
				sprintf(num, " %d ", k + 1);
				AsciiToUnicode(num, name + UnicodeStrlen(name));
				UnicodeStrcpy(name + UnicodeStrlen(name), TheText.Get("RVC_XNP"));
				l = name;
				AsciiToUnicode("--:---:----, --:--:--", date);
				r = date;
			}
			const float y = Y(113.0f + 35.15f * k);	// (measured against the console capture)
			MenuSkinXboxText(X(165.6f), y, H(19.0f), l, CRGBA(0, 0, 0, alpha), 2);
			if (r)
				MenuSkinXboxText(X(254.85f), y + H(15.55f), H(13.0f), r, CRGBA(0, 0, 0, alpha), 2);
		} else if (s.m_aEntries[i].m_Action == MENUACTION_RESUME_FROM_SAVEZONE) {
			// "cancel" above the card (x 150.2) on its bar (1.25 x the outline)
			if (sel)
				MenuSkinXboxFixedBar(X(124.5f), Y(60.9f), X(288.4f), Y(106.9f), alpha);
			CFont::SetFontStyle(FONT_LOCALE(FONT_HEADING));
			CFont::SetScale(W(BIGTEXT_X_SCALE * 1.2f), H(BIGTEXT_Y_SCALE * 1.2f * 480.0f / 448.0f));
			CFont::SetPropOn();
			CFont::SetCentreOff();
			CFont::SetRightJustifyOff();
			CFont::SetWrapx(SCREEN_WIDTH);
			CFont::SetDropShadowPosition(2);
			CFont::SetDropColor(CRGBA(0, 0, 0, alpha));
			CFont::SetColor(CRGBA(XBOX_PINK.r, XBOX_PINK.g, XBOX_PINK.b, alpha));
			CFont::PrintString(X(150.2f), Y(75.1f), TheText.Get("RVC_XCA"));
			CFont::DrawFonts();
			CFont::SetDropShadowPosition(0);
		}
	}
	if (!confirm)
		return;
	// overwriting: the Xbox's OK / Cancel dialog (as the Load page's, Frontend.cpp)
	MenuSkinXboxDialogBack(alpha);
	const wchar *q = TheText.Get(Slots[m->m_nCurrSaveSlot] == SLOT_OK ? "FESZ_QO" : "FESZ_QZ");
	CFont::SetFontStyle(FONT_LOCALE(FONT_STANDARD));
	CFont::SetScale(W(0.90f), H(0.85f * 480.0f / 448.0f));
	CFont::SetPropOn();
	CFont::SetRightJustifyOff();
	CFont::SetCentreOn();
	CFont::SetCentreSize(W(380.0f));
	CFont::SetDropShadowPosition(2);
	CFont::SetDropColor(CRGBA(0, 0, 0, alpha));
	CFont::SetColor(CRGBA(XBOX_PINK.r, XBOX_PINK.g, XBOX_PINK.b, alpha));
	int lines = CFont::GetNumberLines(X(290.0f), Y(181.0f), (wchar *)q);
	CFont::PrintString(X(290.0f), Y(220.0f - lines * 7.8f), (wchar *)q);
	CFont::DrawFonts();
	CFont::SetCentreOff();
	CFont::SetFontStyle(FONT_LOCALE(FONT_HEADING));
	CFont::SetScale(W(BIGTEXT_X_SCALE), H(BIGTEXT_Y_SCALE * 480.0f / 448.0f));
	for (int i = 0; i < NUM_MENUROWS; i++) {
		const int act = aScreens[page].m_aEntries[i].m_Action;
		if (act != MENUACTION_YES && act != MENUACTION_NO)
			continue;
		const bool yes = act == MENUACTION_YES;	// "OK" left, "Cancel" right
		if (i == m->m_nCurrOption)
			MenuSkinXboxFixedBar(X(yes ? 146.05f : 325.65f), Y(279.2f), X(yes ? 228.5f : 408.1f), Y(321.65f), alpha);
		CFont::PrintString(X(yes ? 151.9f : 331.65f), Y(290.0f), TheText.Get(yes ? "RVC_XOK" : "RVC_XCA"));
	}
	CFont::DrawFonts();
	CFont::SetDropShadowPosition(0);
}

// A bar the Xbox's data places (x0..x1 x y0..y1: outline 0's box at the bar's scale): the current
// outline at its own shape about the same centre, with the pulse.
void
MenuSkinXboxFixedBar(float x0, float y0, float x1, float y1, uint8 alpha)
{
	selectionNative = true;
	MenuSkinXboxSelection(x0, y0, x1, y1, alpha, true, 0.0f);
	selectionNative = false;
}

// A dialog's background, drawn before the page's own text (CMenuManager::DrawStandardMenus), as the
// Xbox's dialog animation data has it (research on the Xbox build): the menu dimmed (black, 0.699),
// then the card and its right-hand edge strip twice in (97,194,247): offset (20, 20) at alpha 0.5,
// and on top, opaque.
void
MenuSkinXboxDialogBack(uint8 alpha)
{
	CSprite2d::DrawRect(CRect(0.0f, 0.0f, SCREEN_WIDTH, SCREEN_HEIGHT), CRGBA(0, 0, 0, alpha * 699 / 1000));
	for (int layer = 0; layer < 2; layer++) {
		const float d = layer == 0 ? 20.0f : 0.0f;
		const CRGBA c(97, 194, 247, layer == 0 ? alpha / 2 : alpha);
		// (corners TL, TR, BL, BR)
		CSprite2d::Draw2DPolygon(X(84.5f + d), Y(137.3f + d), X(512.05f + d), Y(118.95f + d), X(99.7f + d), Y(327.9f + d), X(474.6f + d), Y(335.95f + d), c);
		CSprite2d::Draw2DPolygon(X(522.35f + d), Y(117.95f + d), X(542.6f + d), Y(117.1f + d), X(484.9f + d), Y(334.95f + d), X(516.2f + d), Y(335.65f + d), c);
	}
}

// The Xbox's slider (research on the Xbox build, its menu animation data): a wedge (rgb 51,98,128)
// from x 329.75 over 155 px, 11.25 px tall at the left to 37.46 at the right, its bottom 21 px under
// the row's top (measured); the level (0-16, in whole steps) shows that many bars (rgb 97,194,247),
// bar i 5 px wide at x 324.75 + 10 i, its bottom 2.5 px above the wedge's, 13.75 px tall rising to
// 39.95 for the last. The whole slider at half alpha unless its row is selected. Returns the right
// end of the lit part (the PC slider's "last active bar", used for mouse hover).
// The bottoms are the Xbox's own per slider (gXboxSliderBottom: music 169.8, SFX 215, brightness
// 168.1 in 640x480); other sliders 21 px under the row's top.
bool gXboxSliderSelected = true;
float gXboxSliderBottom = -1.0f;
float
MenuSkinXboxWedge(float y, float progress, uint8 alpha)
{
	const float gx = 329.75f, bottom = gXboxSliderBottom >= 0.0f ? Y(gXboxSliderBottom) : y + H(21.0f);
	const uint8 a = gXboxSliderSelected ? alpha : alpha / 2;
	CSprite2d::Draw2DPolygon(X(gx), bottom - H(11.25f), X(gx + 155.0f), bottom - H(37.46f), X(gx), bottom, X(gx + 155.0f), bottom,
		CRGBA(51, 98, 128, a));
	int level = Clamp((int)(progress * 16.0f + 0.5f), 0, 16);
	float filled = X(gx);
	for (int i = 0; i < level; i++) {
		float x0 = gx - 5.0f + 10.0f * i, h = 13.75f + (39.95f - 13.75f) * i / 15.0f;
		CSprite2d::DrawRect(CRect(X(x0), bottom - H(2.5f + h), X(x0 + 5.0f), bottom - H(2.5f)), CRGBA(97, 194, 247, a));
		filled = X(x0 + 5.0f);
	}
	return filled;
}

// Sub-tabs on the Stats ("stats | unlocked items") and Brief ("briefs | tutorial briefings")
// pages, at the bottom left: the active one on a mint bar with a dark block at its outer end
// (measured at 640x480: labels at x 68 and 248 / 308, y 350; bars 60-240 with the block at
// 45-60, and 300-590 with the block at 590-605). <-/-> switches.
static int subTab = 0, subTabPage = -1;

static bool
SettingsSubTabs(int page)
{
	return page == MENUPAGE_CONSOLE_DISPLAY || page == MENUPAGE_SOUND_SETTINGS || page == MENUPAGE_CONSOLE_CONTROLS || page == MENUPAGE_CONSOLE_OPTIONS;
}

static bool
HasSubTabs(int page)
{
	// Display, Audio and Controls stay as on the console (Y opens their "advanced" list); the
	// list shows "<page> | advanced" where Stats has its sub-tabs
	return page == MENUPAGE_STATS || page == MENUPAGE_BRIEFS || page == MENUPAGE_CONSOLE_OPTIONS;
}

int
MenuSkinXboxSubTab(int page)
{
	if (SettingsSubTabs(page))
		return page == MENUPAGE_CONSOLE_OPTIONS ? 1 : 0;
	return page == subTabPage ? subTab : 0;
}

// Test: the "advanced" list of a section (0 display, 1 audio, 2 controls) as if opened from its page.
extern "C" void
MenuSkinXboxOptionsFor(int section)
{
	extern int gConsoleOptionsSection;
	optionsOrigin = section == 1 ? MENUPAGE_SOUND_SETTINGS : section == 2 ? MENUPAGE_CONSOLE_CONTROLS : MENUPAGE_CONSOLE_DISPLAY;
	gConsoleOptionsSection = section;
	ConsoleAudioRows(true);
}

extern "C" void	// (C linkage: called from skel/host/testhooks.cpp)
MenuSkinXboxSetSubTab(int page, int tab)
{
	subTabPage = page;
	subTab = tab;
}

void MenuSkinXboxFixedBar(float x0, float y0, float x1, float y1, uint8 alpha);
static void
DrawSubTabs(int page, uint8 alpha)
{
	const bool stats = page == MENUPAGE_STATS;
	const char *names[2] = { stats ? "RVC_XS1" : "RVC_XB1", stats ? "RVC_XS2" : "RVC_XB2" };
	float lx[2] = { 68.2f, stats ? 248.2f : 307.65f };	// (research on the Xbox build)
	if (SettingsSubTabs(page)) {	// "display | advanced" and so on (OPTIONS_PLACEMENT)
		int origin = page == MENUPAGE_CONSOLE_OPTIONS ? optionsOrigin : page;
		names[0] = origin == MENUPAGE_SOUND_SETTINGS ? "RVC_XAU" : origin == MENUPAGE_CONSOLE_CONTROLS ? "RVC_XCO" : "RVC_XDI";
		names[1] = "RVC_XAD";
		lx[1] = 248.0f;
	}
	const float ty = 350.0f * 448.0f / 480.0f;	// (measured: the glyphs' top, a little under the font cell's)
	float y0, y1;
	int cur = MenuSkinXboxSubTab(page);
	CFont::SetFontStyle(FONT_LOCALE(FONT_HEADING));
	CFont::SetScale(W(BIGTEXT_X_SCALE * 1.2f), H(BIGTEXT_Y_SCALE * 1.2f));
	CFont::SetPropOn();
	CFont::SetCentreOff();
	CFont::SetRightJustifyOff();
	CFont::SetWrapx(SCREEN_WIDTH);
	// the bars at the Xbox's fixed extents per sub-tab, y 318.7-355.4, no end block; on Brief/Stats
	// with the icon bar focused the bar alone at 0.602 alpha, the labels opaque (research on the
	// Xbox build, checked on autopad set 2)
	static const float statsBar[2][2] = { { 59.9f, 243.5f }, { 239.5f, 593.6f } }, briefBar[2][2] = { { 62.4f, 298.4f }, { 302.8f, 591.4f } };
	const float (*bar)[2] = page == MENUPAGE_BRIEFS ? briefBar : statsBar;
	float tilt = W(5.0f), bx0 = X(bar[cur][0]), bx1 = X(bar[cur][1]) - tilt;
	y0 = Y(318.7f);
	y1 = Y(355.4f);
	(void)page;
	const uint8 barAlpha = barFocus && !SettingsSubTabs(page) ? alpha * 602 / 1000 : alpha;
	// Brief/Stats: the Xbox's own outline at the sub-tab's scale, pulsing while the sub-tabs have
	// focus (the console's Stats page: the pulse shows past the bar's ends)
	if (!SettingsSubTabs(page) && !barFocus)
		MenuSkinXboxFixedBar(X(bar[cur][0]), y0, X(bar[cur][1]), y1, alpha);
	else if (SettingsSubTabs(page) || !DrawBarShape(X(bar[cur][0]), y0, X(bar[cur][1]), y1, CRGBA(0, 207, 133, barAlpha), true, -1, true))
	CSprite2d::Draw2DPolygon(bx0 + tilt, y0, bx1 + tilt, y0, bx0, y1, bx1, y1, CRGBA(XBOX_MINT.r, XBOX_MINT.g, XBOX_MINT.b, barAlpha));
	CFont::SetDropShadowPosition(2);
	CFont::SetDropColor(CRGBA(0, 0, 0, alpha));
	CFont::SetColor(CRGBA(XBOX_PINK.r, XBOX_PINK.g, XBOX_PINK.b, alpha));
	for (int i = 0; i < 2; i++)
		CFont::PrintString(X(lx[i]), Y(ty), TheText.Get(names[i]));
	CFont::SetDropShadowPosition(0);
}

// Controls, "select controls": A opens an in-place drop-down over the row (Xbox: "standard" /
// "classic", light-blue box, the current one mint; revc adds its own "original PC").
#define NUM_DROP 3
static bool dropOpen;
static int dropSel;
// A on an entry asks first (dialog: "Do you want to overwrite your current controls?", OK / CANCEL
// with CANCEL preselected; left/right switch, A confirms, nothing else acts); then the drop-down
// is open again on the entry picked.
static bool dropConfirm;
static int dropConfirmSel;
extern "C" int PadLayoutCurrent(void);
extern "C" void PadLayoutApply(int layout);
void SaveINIControllerSettings(void);

static void
DrawDropDown(uint8 alpha)
{
	// the Xbox's drop-down (research on the Xbox build): a light-blue card TL(252.45, 93.5)
	// TR(413.45, 98) BL(256.95, 181.45) BR(416.95, 177.45) at alpha 154 for its two entries (one
	// row lower here for revc's third), the entries at x 273.55, 34 apart, in its item font and the
	// legend's pink
	static const char *names[NUM_DROP] = { "standard", "classic", "original pc" };
	const float pitch = 34.0f, extra = pitch * (NUM_DROP - 2);
	CSprite2d::Draw2DPolygon(X(252.45f), Y(93.5f), X(413.45f), Y(98.0f), X(256.95f), Y(181.45f + extra), X(416.95f), Y(177.45f + extra),
		CRGBA(97, 194, 247, alpha * 154 / 255));
	float top = 143.55f - 28.9f + pitch * dropSel;	// (the glyphs' top under the font cell's baseline)
	MenuSkinXboxSelection(X(262.0f), Y(top - 4.0f), X(408.0f), Y(top + 30.0f), alpha, false, 0.0f);
	CFont::SetFontStyle(FONT_LOCALE(FONT_HEADING));
	CFont::SetScale(W(BIGTEXT_X_SCALE * 1.2f), H(BIGTEXT_Y_SCALE * 1.2f));
	CFont::SetPropOn();
	CFont::SetCentreOff();
	CFont::SetRightJustifyOff();
	CFont::SetWrapx(SCREEN_WIDTH);
	CFont::SetDropShadowPosition(2);
	CFont::SetDropColor(CRGBA(0, 0, 0, alpha));
	CFont::SetColor(CRGBA(252, 138, 242, alpha));
	for (int i = 0; i < NUM_DROP; i++) {
		wchar buf[32];
		AsciiToUnicode(names[i], buf);
		CFont::PrintString(X(273.55f), Y(143.55f - 28.9f + pitch * i), buf);
	}
	CFont::SetDropShadowPosition(0);
}

// The page START opens (Frontend.cpp), with the icon bar focused.
int
MenuSkinXboxOpenPage(void)
{
	barFocus = true;
	return lastPage;
}

// The icon bar has focus (Frontend.cpp: no row highlighted).
bool
MenuSkinXboxBarFocus(CMenuManager *m)
{
	return MenuSkinXboxActive(m->m_nCurrScreen, m->m_bGameNotLoaded) && barFocus && !MenuSkinXboxDialog(m->m_nCurrScreen);
}

// Test shots: focus inside the page (rows highlighted) or on the icon bar.
extern "C" void
MenuSkinXboxSetBarFocus(int bar)
{
	barFocus = bar != 0;
	dropOpen = false;
	dropConfirm = false;
}

// Test sweep: the icon bar's pages, in order (-1 past the end).
int
MenuSkinXboxTabPage(int i)
{
	return i >= 0 && i < (int)ARRAY_SIZE(tabs) ? tabs[i].page : -1;
}

// A page the icon bar switches to (not a sub-page, dialog or "advanced" list).
static bool
IsTabPage(int page)
{
	for (int i = 0; i < (int)ARRAY_SIZE(tabs); i++)
		if (tabs[i].page == page)
			return true;
	return false;
}

// Focus into the page: Brief/Stats focus their sub-tab bar, the others row 0.
static void
EnterPage(CMenuManager *m)
{
	barFocus = false;
	if (!HasSubTabs(m->m_nCurrScreen)) {
		m->m_nCurrOption = aScreens[m->m_nCurrScreen].m_aEntries[0].m_Action == MENUACTION_LABEL ? 1 : 0;
		m->m_nFirstVisibleRowOnList = 0;
	}
}

// Navigation as on the Xbox pause menu (measured):
// - the icon bar: left/right switch pages at once and wrap (load map brief stats controls audio
//   display); A goes into the page; B closes the menu; nothing else acts.
// - inside a page: the PC menu's own list input; LT/RT switch to the previous/next page and stay
//   inside it; B goes back to the icon bar (on sub-pages, dialogs and the "advanced" list B goes
//   back as on the PC). Brief and Stats: inside means the sub-tab bar, left/right switch sub-tabs.
// - revc: Y on Display/Audio/Controls opens their "advanced" list (B comes back).
// START closes the menu from anywhere (Frontend.cpp). True when the input was used here.
bool
MenuSkinXboxInput(CMenuManager *m)
{
	// The Xbox's menu sounds for what the skin handles itself (research XBOX_MENU_SOUNDS.md: the
	// same ids and recordings as the PC's): item moves and page changes the highlight sound (195),
	// A select (196), B back (197); no error sound anywhere.
	auto sound = [](int s) { DMAudio.PlayFrontEndSound(s, 0); };
	if (!MenuSkinXboxActive(m->m_nCurrScreen, m->m_bGameNotLoaded))
		return false;
	CPad *pad = CPad::GetPad(0);
	const bool left = pad->GetDPadLeftJustDown() || pad->GetLeftJustDown(), right = pad->GetDPadRightJustDown() || pad->GetRightJustDown();
	const bool a = pad->GetCrossJustDown() || pad->GetEnterJustDown(), b = pad->GetCircleJustDown() || pad->GetEscapeJustDown();
	subBarFocus = false;
	if (IsTabPage(m->m_nCurrScreen))
		lastPage = m->m_nCurrScreen;
	int n = ARRAY_SIZE(tabs), cur = CurrentTab(m->m_nCurrScreen);
	if (barFocus && !MenuSkinXboxDialog(m->m_nCurrScreen)) {
		if (b) {
			sound(SOUND_FRONTEND_BACK);
			m->m_bShutDownFrontEndRequested = true;
			return true;
		}
		if (a) {
			if (!IsTabPage(m->m_nCurrScreen))
				return false;	// (a sub-page: as on the PC)
			// (entering Controls or Display: the Xbox's "info" bip, SOUND_HUD, 179)
			sound(m->m_nCurrScreen == MENUPAGE_CONSOLE_CONTROLS || m->m_nCurrScreen == MENUPAGE_CONSOLE_DISPLAY ? SOUND_HUD : SOUND_FRONTEND_ENTER_OR_ADJUST);
			EnterPage(m);
			return true;
		}
		if ((left || right) && cur >= 0) {
			sound(SOUND_FRONTEND_HIGHLIGHT_OPTION);
			m->SwitchToNewScreen(SWITCH_TO_PAGE(tabs[(cur + (right ? 1 : -1) + n) % n].page));
			barFocus = true;
		}
		return true;
	}
	// inside a page
	if (dropOpen && dropConfirm) {
		if (left || right) {
			sound(SOUND_FRONTEND_HIGHLIGHT_OPTION);
			dropConfirmSel = 1 - dropConfirmSel;
		}
		if (a) {
			sound(SOUND_FRONTEND_ENTER_OR_ADJUST);
			if (dropConfirmSel == 0) {
				extern int8 gPs2Setup;
				PadLayoutApply(dropSel);
				gPs2Setup = -1;
				SaveINIControllerSettings();
			}
			dropConfirm = false;
		}
		return true;
	}
	if (dropOpen) {	// the Controls drop-down: up/down choose, A asks to apply, B closes it
		const bool up = pad->GetDPadUpJustDown() || pad->GetUpJustDown(), down = pad->GetDPadDownJustDown() || pad->GetDownJustDown();
		if (up || down) {
			sound(SOUND_FRONTEND_HIGHLIGHT_OPTION);
			dropSel = (dropSel + (down ? 1 : NUM_DROP - 1)) % NUM_DROP;
		}
		if (a) {
			sound(SOUND_FRONTEND_ENTER_OR_ADJUST);
			dropConfirm = true;
			dropConfirmSel = 1;
		}
		if (b)
			sound(SOUND_FRONTEND_BACK);
		if (b || m->m_nCurrScreen != MENUPAGE_CONSOLE_CONTROLS)
			dropOpen = false;
		return true;
	}
	if (a && m->m_nCurrScreen == MENUPAGE_CONSOLE_CONTROLS && m->m_nCurrOption == 0) {
		sound(SOUND_FRONTEND_ENTER_OR_ADJUST);
		dropOpen = true;
		dropConfirm = false;
		dropSel = Max(PadLayoutCurrent(), 0);
		return true;
	}
	int step = pad->GetRightShoulder2JustDown() ? 1 : pad->GetLeftShoulder2JustDown() ? -1 : 0;
	if (step != 0 && cur >= 0) {
		sound(SOUND_FRONTEND_HIGHLIGHT_OPTION);
		m->SwitchToNewScreen(SWITCH_TO_PAGE(tabs[(cur + step + n) % n].page));
		EnterPage(m);
		return true;
	}
	if (b && IsTabPage(m->m_nCurrScreen)) {
		sound(SOUND_FRONTEND_BACK);
		barFocus = true;
		return true;
	}
	if (b && pad->GetCircleJustDown()) {	// B on a sub-page or dialog: back, the row it came from selected (the PC menu goes back on triangle)
		sound(SOUND_FRONTEND_BACK);
		for (int i = 0; i < n; i++)
			if (tabs[i].alt == m->m_nCurrScreen && tabs[i].alt != tabs[0].alt) {	// a tab's second page (the controller picture): to the tab's page
				int from = m->m_nCurrScreen;
				m->SwitchToNewScreen(SWITCH_TO_PAGE(tabs[i].page));
				for (int r = 0; r < NUM_MENUROWS; r++)
					if (aScreens[m->m_nCurrScreen].m_aEntries[r].m_TargetMenu == from)
						m->m_nCurrOption = r;
				return true;
			}
		m->SwitchToNewScreen(-2);
		return true;
	}
	// Y on Display, Audio and Controls opens their "advanced" list; B there goes back to the page
	if (SettingsSubTabs(m->m_nCurrScreen) && m->m_nCurrScreen != MENUPAGE_CONSOLE_OPTIONS && pad->GetTriangleJustDown()) {
		extern int gConsoleOptionsSection;
		optionsOrigin = m->m_nCurrScreen;
		gConsoleOptionsSection = optionsOrigin == MENUPAGE_SOUND_SETTINGS ? 1 : optionsOrigin == MENUPAGE_CONSOLE_CONTROLS ? 2 : 0;
		ConsoleAudioRows(true);
		sound(SOUND_FRONTEND_ENTER_OR_ADJUST);
		aScreens[MENUPAGE_CONSOLE_OPTIONS].m_PreviousPage = optionsOrigin;
		m->SwitchToNewScreen(MENUPAGE_CONSOLE_OPTIONS);
		return true;
	}
	if (HasSubTabs(m->m_nCurrScreen) && !SettingsSubTabs(m->m_nCurrScreen)) {
		if (left || right) {
			sound(SOUND_FRONTEND_HIGHLIGHT_OPTION);
			// a hard cut: the content and the sub-tab highlight switch in one frame (research,
			// autopad set 2 at 3888: no fade, slide or glide)
			MenuSkinXboxSetSubTab(m->m_nCurrScreen, 1 - MenuSkinXboxSubTab(m->m_nCurrScreen));
		}
		return left || right || a;
	}
	return false;
}

// Called by CMenuManager::DrawBackground after the page.
void
MenuSkinXboxDraw(CMenuManager *m, uint8 alpha)
{
	if (!MenuSkinXboxActive(m->m_nCurrScreen, m->m_bGameNotLoaded))
		return;
	if (HasSubTabs(m->m_nCurrScreen))
		DrawSubTabs(m->m_nCurrScreen, alpha);
	DrawIconBar(m, DialogDim(alpha));
	DrawHelpLine(m, DialogDim(alpha));
	if (dropOpen && m->m_nCurrScreen == MENUPAGE_CONSOLE_CONTROLS) {
		DrawDropDown(alpha);
		if (dropConfirm) {
			MenuSkinXboxDialogBack(alpha);
			wchar buf[96];
			AsciiToUnicode("do you want to overwrite your current controls?", buf);
			CFont::SetFontStyle(FONT_LOCALE(FONT_HEADING));
			CFont::SetScale(W(0.6f), H(0.9f));
			CFont::SetCentreOn();
			CFont::SetCentreSize(W(360.0f));
			CFont::SetPropOn();
			CFont::SetDropShadowPosition(0);
			CFont::SetColor(CRGBA(XBOX_PINK.r, XBOX_PINK.g, XBOX_PINK.b, alpha));
			CFont::PrintString(X(312.0f), Y(185.0f), buf);
			const char *b2[2] = { "ok", "cancel" };
			const float bx[2] = { 200.0f, 420.0f };
			for (int i = 0; i < 2; i++) {
				AsciiToUnicode(b2[i], buf);
				float w = CFont::GetStringWidth(buf, true);
				if (i == dropConfirmSel)
					MenuSkinXboxSelection(X(bx[i]) - w / 2 - W(8.0f), Y(272.0f), X(bx[i]) + w / 2 + W(8.0f), Y(272.0f) + H(22.0f), alpha, false, 0.0f);
				CFont::SetColor(i == dropConfirmSel ? CRGBA(0, 0, 0, alpha) : CRGBA(XBOX_PINK.r, XBOX_PINK.g, XBOX_PINK.b, alpha));
				CFont::PrintString(X(bx[i]), Y(275.0f), buf);
			}
			CFont::SetCentreOff();
			CFont::DrawFonts();
		}
	}
	CFont::DrawFonts();
}
