// Controller button glyphs in prompts (plan task 9), C++ half. CFont draws ~X~-style tokens with
// CFont::ButtonSprite (BUTTON_ICONS); originally these came from one fixed TXD. Here the sprites
// follow the pad in use: the Rust host (rust/revc/src/host/glyphs.rs) picks and decodes the right
// images from assets/glyphs (Kenney Input Prompts); without those assets the game's own
// x360btns / ps3btns / nswbtns TXDs are used per pad family.
#if defined LIBRW_HOST

#include "common.h"
#include "MusicManager.h"
#include "rwcore.h"
#include "skeleton.h"
#include "Font.h"
#include "Sprite2d.h"
#include "TxdStore.h"
#include "Particle.h"
#include "Hud.h"

extern "C" {
void revc_glyph_set_override(int style);
void revc_ui_style_set(int style);
int revc_glyph_set_id(void);
int revc_glyph_rgba(int index, int *width, int *height, unsigned char **rgba);
void revc_texmod_free(unsigned char *rgba, int width, int height);
int hostActiveInputKind(void);
}

#ifdef BUTTON_ICONS
// Texture names CFont::LoadButtons uses, in BUTTON_* order.
static const char *ButtonTexNames[MAX_BUTTON_ICONS] = {
	"thumblyu", "thumblyd", "thumblxl", "thumblxr",
	"cross", "circle", "square", "triangle",
	"l1", "l2", "l3", "r1", "r2", "r3",
	"thumbryu", "thumbryd", "thumbrxl", "thumbrxr",
	"dpadu", "dpadr", "dpadd", "dpadl", "select", "start",
};

static rw::Texture *
MakeTexture(int index)
{
	int w, h;
	unsigned char *px;
	if (!revc_glyph_rgba(index, &w, &h, &px))
		return nil;
	rw::Image *img = rw::Image::create(w, h, 32);
	img->pixels = px;	// not owned by the image, freed by Rust below
	img->stride = w * 4;
	// Drawn much smaller than 128 px inside text, so mipmapped and trilinear.
	rw::Raster *ras = rw::Raster::create(w, h, 32,
		rw::Raster::C8888 | rw::Raster::TEXTURE | rw::Raster::MIPMAP | rw::Raster::AUTOMIPMAP);
	rw::Texture *tex = nil;
	if (ras && ras->setFromImage(img)) {
		tex = rw::Texture::create(ras);
		strncpy(tex->name, ButtonTexNames[index], sizeof(tex->name) - 1);
		tex->setFilter(rw::Texture::LINEARMIPLINEAR);
	} else if (ras)
		ras->destroy();
	img->pixels = nil;
	img->destroy();
	revc_texmod_free(px, w, h);
	return tex;
}

// Puts `dict` into the "buttons" TXD slot and points the button sprites at it (as LoadButtons).
static void
InstallButtons(RwTexDictionary *dict)
{
	if (CFont::ButtonsSlot == -1) {
		CFont::ButtonsSlot = CTxdStore::AddTxdSlot("buttons");
		CTxdStore::AddRef(CFont::ButtonsSlot);
	} else {
		for (int i = 0; i < MAX_BUTTON_ICONS; i++)
			CFont::ButtonSprite[i].Delete();
		CTxdStore::RemoveTxd(CFont::ButtonsSlot);
	}
	CTxdStore::GetSlot(CFont::ButtonsSlot)->texDict = dict;
	CTxdStore::PushCurrentTxd();
	CTxdStore::SetCurrentTxd(CFont::ButtonsSlot);
	for (int i = 0; i < MAX_BUTTON_ICONS; i++)
		CFont::ButtonSprite[i].SetTexture(ButtonTexNames[i]);
	CTxdStore::PopCurrentTxd();
}

static bool
LoadGlyphSet(void)
{
	RwTexDictionary *dict = RwTexDictionaryCreate();
	int count = 0;
	for (int i = 0; i < MAX_BUTTON_ICONS; i++)
		if (rw::Texture *tex = MakeTexture(i)) {
			dict->add(tex);
			count++;
		}
	if (count == 0) {
		RwTexDictionaryDestroy(dict);
		return false;
	}
	InstallButtons(dict);
	return true;
}

// PadKind values (rust/revc/src/host/pads.rs) -> the game's own button TXD of that family.
static const char *
FallbackTxd(int kind)
{
	switch (kind) {
	case 4: case 5: case 6: return "MODELS/PS3BTNS.TXD";	// PlayStation 3/4/5
	case 7: case 8: return "MODELS/NSWBTNS.TXD";		// Switch, GameCube
	default: return "MODELS/X360BTNS.TXD";
	}
}

// Controls menu "BUTTON PROMPTS" ([Controller] ButtonPrompts in reVC.ini): 0 auto, 1 Xbox,
// 2 PlayStation, 3 Nintendo, 4 Steam Deck, 5 Steam Controller.
int8 gButtonPromptsOverride = 0;
// Graphics menu "UI STYLE" ([Graphics] UiStyle): 0 PC, 1 PS2, 2 Xbox (plan task 13).
int8 gUiStyle = 0;
// The button sprites come from the glyph pack (white line art in the console UI styles' outline
// variant) rather than the game's coloured button TXDs; the PS2 menu skin tints them.
bool gGlyphPackLoaded = false;

static int LoadedGlyphSet = 0;
static int GlyphSetsLoaded = 0;

// Called every frame in the menus and in game; reloads the sprites when another kind of pad
// (or one with a different face-button layout) becomes the one in use.
void
HostUpdateButtonGlyphs(void)
{
	revc_glyph_set_override(gButtonPromptsOverride);
	revc_ui_style_set(gUiStyle);
	// Texture packs per UI style (mods/packs/pc-*, ps2-*, xbox-*): textures loaded so far used the
	// PC style's packs; when the style differs, load the HUD and fonts again (the menu's art loads
	// each time the menu opens). Also right after start with a console style saved.
	static int texStyle = 0;
	if (gUiStyle != texStyle && CTxdStore::FindTxdSlot("hud") != -1) {
		texStyle = gUiStyle;
		CFont::Shutdown();
		CFont::Initialise();
		CHud::ReloadTextures();
		CParticle::ApplyStyle();	// the Xbox style's fire (particle values and flame frames)
		MusicManager.RefreshTrackLengths();	// the PS2 style's radio and cutscene streams (R5b)
		LoadedGlyphSet = 0;	// CFont::Initialise put back the game's buttons
	}
	int set = revc_glyph_set_id();
	if (set == 0 || set == LoadedGlyphSet)
		return;	// no pad, or already loaded (keyboard use keeps the last pad's set)
	LoadedGlyphSet = set;
	GlyphSetsLoaded++;
	gGlyphPackLoaded = LoadGlyphSet();
	if (!gGlyphPackLoaded)
		CFont::LoadButtons(FallbackTxd(((set - 1) >> 16) & 0xFF));
}

extern "C" int
revc_test_glyph_sets_loaded(void)
{
	return GlyphSetsLoaded;
}
#else
int8 gButtonPromptsOverride = 0;
int8 gUiStyle = 0;
bool gGlyphPackLoaded = false;
void HostUpdateButtonGlyphs(void) {}
extern "C" int revc_test_glyph_sets_loaded(void) { return 0; }
#endif // BUTTON_ICONS

#endif
