// revc (roadmap R4): the Xbox's cutscene hands. In the Xbox UI style, cutscene characters (the
// special characters, MI_SPECIAL01..21) get the Xbox's separate hand models in place of their own
// hands, per side, with the colour, size, position offset and finger pose the Xbox's
// data/cutscenehands.xml gives per cutscene and character (read by rust/revc/src/cshands.rs).
// Everything comes from the player's Xbox import; with any piece missing nothing changes.
// Draw or do nothing: a character's own hand bones are collapsed only in a frame where its
// replacement is placed and will be drawn. Not copied from the Xbox: it also shrinks bones 54-63
// (the toes in many of its models) for every shown hand, which is left out until a capture shows
// what the console really does (docs/CONSOLE_STYLES.md); a model without the hand bone gets no
// replacement on that side.
#include "common.h"
#include "RwHelper.h"
#include "NodeName.h"
#include "General.h"
#include "ModelInfo.h"
#include "ModelIndices.h"
#include "CutsceneMgr.h"
#include "CutsceneObject.h"
#include "Script.h"
#include "Timer.h"
#include "CutsceneHands.h"

extern int8 gUiStyle;
extern "C" ssize_t revc_import_read(int kind, const char *rel, uint8 *buf, size_t cap);

struct RevcHandSettings {
	int32 on, glove, female, black;
	uint8 rgb[4];
	float scale[3];
	float position[3];
	int32 animStatic;
	float time;
};
extern "C" int32 revc_cshands_set_config(const uint8 *text, size_t len);
extern "C" int32 revc_cshands_eval(const char *cutscene, const char *const *missions, int32 numMissions,
	const char *character, RevcHandSettings *out);

namespace CutsceneHands {

enum { LEFT, RIGHT, NUM_SIDES };
enum { MAX_OBJECTS = 8, MAX_MISSIONS = 128, KEYFRAME_BYTES = 36 };

// HAnim IDs of the character's bones per side: hand, forearm, and the hand bones collapsed
static const int32 handBone[NUM_SIDES] = { 34, 24 };
static const int32 forearmBone[NUM_SIDES] = { 33, 23 };
static const int32 ownHand[NUM_SIDES][3] = { { 34, 35, 36 }, { 24, 25, 26 } };

struct Side {
	RevcHandSettings set;
	RpAtomic *atomic;
	RwFrame *palm;
	RpHAnimHierarchy *hier;
	rw::Animation *anim;
	float time;	// pose time (loop: advanced each frame)
	bool draw;	// placed this frame
};

struct Instance {
	CCutsceneObject *obj;
	RpClump *clone;
	Side side[NUM_SIDES];
};

static RwTexDictionary *txd;
static RpClump *master;
static rw::Animation *anims[NUM_SIDES][2];	// [side][female]
static bool tried, ready;
static Instance instances[MAX_OBJECTS];

// A RenderWare hierarchy animation (.anm, scheme 1): the header and the keyframes (time,
// rotation, translation, the byte offset of the node's previous keyframe; the first keyframe of
// each node holds no valid offset).
static rw::Animation*
ReadAnm(const uint8 *p, size_t n)
{
	if(n < 32)
		return nil;
	uint32 type, scheme, numFrames, flags;
	float duration;
	memcpy(&type, p, 4);
	memcpy(&scheme, p + 16, 4);
	memcpy(&numFrames, p + 20, 4);
	memcpy(&flags, p + 24, 4);
	memcpy(&duration, p + 28, 4);
	if(type != 0x1B || scheme != 1 || numFrames == 0 || n < 32 + (size_t)numFrames * KEYFRAME_BYTES)
		return nil;
	rw::AnimInterpolatorInfo *info = rw::AnimInterpolatorInfo::find(1);
	if(info == nil)
		return nil;
	rw::Animation *a = rw::Animation::create(info, numFrames, flags, duration);
	if(a == nil)
		return nil;
	rw::HAnimKeyFrame *kf = (rw::HAnimKeyFrame*)a->keyframes;
	for(uint32 i = 0; i < numFrames; i++){
		const uint8 *s = p + 32 + i * KEYFRAME_BYTES;
		float f[8];
		int32 prev;
		memcpy(f, s, 32);
		memcpy(&prev, s + 32, 4);
		kf[i].time = f[0];
		kf[i].q.x = f[1]; kf[i].q.y = f[2]; kf[i].q.z = f[3]; kf[i].q.w = f[4];
		kf[i].t.x = f[5]; kf[i].t.y = f[6]; kf[i].t.z = f[7];
		uint32 pi = (uint32)prev / KEYFRAME_BYTES;
		kf[i].prev = prev >= 0 && prev % KEYFRAME_BYTES == 0 && pi < i ? &kf[pi] : nil;
	}
	return a;
}

static uint8*
ReadImport(const char *rel, size_t cap, ssize_t *len)
{
	uint8 *buf = new uint8[cap];
	*len = revc_import_read(0, rel, buf, cap);
	if(*len <= 0){
		delete[] buf;
		return nil;
	}
	return buf;
}

static void
FreeInstance(Instance &in)
{
	if(in.clone)
		RpClumpDestroy(in.clone);
	memset(&in, 0, sizeof(in));
}

static void
Free(void)
{
	for(Instance &in : instances)
		FreeInstance(in);
	for(auto &s : anims)
		for(rw::Animation *&a : s)
			if(a){ a->destroy(); a = nil; }
	if(master){ RpClumpDestroy(master); master = nil; }
	if(txd){ RwTexDictionaryDestroy(txd); txd = nil; }
	ready = false;
}

static void
Load(void)
{
	tried = true;
	static const char *anmFiles[NUM_SIDES][2] = {
		{ "anim/slhandwm.anm", "anim/slhandwf.anm" }, { "anim/srhandwm.anm", "anim/srhandwf.anm" } };
	const char *missing = nil;
	ssize_t len;
	uint8 *buf = ReadImport("data/cutscenehands.xml", 1 << 20, &len);
	if(buf == nil || revc_cshands_set_config(buf, len) <= 0)
		missing = "the hands configuration";
	delete[] buf;
	if(missing == nil){
		buf = ReadImport("models/cshands.txd", 8 << 20, &len);
		if(buf){
			RwMemory mem = { buf, (RwUInt32)len };
			RwStream *s = RwStreamOpen(rwSTREAMMEMORY, rwSTREAMREAD, &mem);
			if(s){
				if(RwStreamFindChunk(s, rwID_TEXDICTIONARY, nil, nil))
					txd = RwTexDictionaryGtaStreamRead(s);
				RwStreamClose(s, &mem);
			}
			delete[] buf;
		}
		if(txd == nil)
			missing = "the hands' textures";
	}
	if(missing == nil){
		buf = ReadImport("models-world/cshands.dff", 8 << 20, &len);
		if(buf){
			RwTexDictionary *prev = RwTexDictionaryGetCurrent();
			RwTexDictionarySetCurrent(txd);
			RwMemory mem = { buf, (RwUInt32)len };
			RwStream *s = RwStreamOpen(rwSTREAMMEMORY, rwSTREAMREAD, &mem);
			if(s){
				if(RwStreamFindChunk(s, rwID_CLUMP, nil, nil))
					master = RpClumpStreamRead(s);
				RwStreamClose(s, &mem);
			}
			RwTexDictionarySetCurrent(prev);
			delete[] buf;
		}
		if(master == nil)
			missing = "the hands model";
	}
	for(int32 side = 0; side < NUM_SIDES && missing == nil; side++)
		for(int32 f = 0; f < 2 && missing == nil; f++){
			buf = ReadImport(anmFiles[side][f], 1 << 20, &len);
			if(buf)
				anims[side][f] = ReadAnm(buf, len);
			delete[] buf;
			if(anims[side][f] == nil)
				missing = "a finger pose";
		}
	if(missing){
		printf("CutsceneHands: %s missing; no hands\n", missing);
		Free();
		return;
	}
	ready = true;
}

// The palm frame of a variant: S{L|R}handPalm{C|W}{M|F}, or S{L|R}handPalmGM for the glove.
struct FindArgs { const char *name; RpAtomic *atomic; RwFrame *palm; };

static RpAtomic*
FindPalm(RpAtomic *atomic, void *data)
{
	FindArgs *a = (FindArgs*)data;
	RwFrame *f = RpAtomicGetFrame(atomic);
	for(int up = 0; f && up < 2; up++, f = RwFrameGetParent(f))
		if(CGeneral::faststricmp(GetFrameNodeName(f), a->name) == 0){
			a->atomic = atomic;
			a->palm = f;
			return nil;
		}
	return atomic;
}

static void
SetPose(Side &s)
{
	rw::HAnimHierarchy *h = (rw::HAnimHierarchy*)s.hier;
	float d = s.anim->duration;
	float t = d > 0.0f ? fmodf(s.time, d) : 0.0f;
	if(t < 0.0f)
		t += d;
	h->interpolator->setCurrentAnim(s.anim);
	h->interpolator->addTime(t);
}

// A new character in the running cutscene: its settings, and its own copy of the hands model
// with each shown side's palm linked to its skeleton and pose.
static Instance*
Create(CCutsceneObject *obj)
{
	Instance *in = nil;
	for(Instance &i : instances)
		if(i.obj == nil){ in = &i; break; }
	if(in == nil)
		return nil;
	static char names[MAX_MISSIONS][9];
	static const char *ptrs[MAX_MISSIONS];
	int32 n = 0;
	for(CRunningScript *s = CTheScripts::pActiveScripts; s && n < MAX_MISSIONS; s = s->GetNext()){
		memcpy(names[n], s->m_abScriptName, 8);
		names[n][8] = '\0';
		ptrs[n] = names[n];
		n++;
	}
	RevcHandSettings set[NUM_SIDES];
	revc_cshands_eval(CCutsceneMgr::GetCutsceneName(), ptrs, n,
		CModelInfo::GetModelInfo(obj->GetModelIndex())->GetModelName(), set);
	in->obj = obj;
	if(!set[LEFT].on && !set[RIGHT].on)
		return in;	// known, no hands
	in->clone = RpClumpClone(master);
	if(in->clone == nil)
		return in;
	for(int32 side = 0; side < NUM_SIDES; side++){
		Side &s = in->side[side];
		s.set = set[side];
		if(!s.set.on)
			continue;
		char name[16];
		if(s.set.glove)
			sprintf(name, "S%chandPalmGM", side == RIGHT ? 'R' : 'L');
		else
			sprintf(name, "S%chandPalm%c%c", side == RIGHT ? 'R' : 'L', s.set.black ? 'C' : 'W', s.set.female ? 'F' : 'M');
		FindArgs a = { name, nil, nil };
		RpClumpForAllAtomics(in->clone, FindPalm, &a);
		RpHAnimHierarchy *hier = a.palm ? RpHAnimFrameGetHierarchy(a.palm) : nil;
		if(hier == nil){
			s.set.on = false;
			continue;
		}
		// as clump model infos do for their skinned atomics; the matrices come out relative to
		// the palm, the frame the atomic hangs on, so they are local space
		RpSkinAtomicSetHAnimHierarchy(a.atomic, hier);
		((rw::HAnimHierarchy*)hier)->attach();
		((rw::HAnimHierarchy*)hier)->flags |= rw::HAnimHierarchy::LOCALSPACEMATRICES;
		s.atomic = a.atomic;
		s.palm = a.palm;
		s.hier = hier;
		s.anim = anims[side][s.set.female ? 1 : 0];
		s.time = s.set.time;
		if(((rw::HAnimHierarchy*)hier)->interpolator == nil){
			s.set.on = false;
			continue;
		}
		SetPose(s);
		// the configured colour tints the texture (set again before each draw: clones share the
		// geometry and its materials)
		RpGeometry *g = RpAtomicGetGeometry(a.atomic);
		RpGeometrySetFlags(g, RpGeometryGetFlags(g) | rpGEOMETRYMODULATEMATERIALCOLOR);
	}
	return in;
}

static Instance*
Find(CCutsceneObject *obj)
{
	for(Instance &i : instances)
		if(i.obj == obj)
			return &i;
	return nil;
}

void
PreRender(CCutsceneObject *obj)
{
	Instance *in = Find(obj);
	if(in)
		in->side[LEFT].draw = in->side[RIGHT].draw = false;
	if(gUiStyle != 2 || obj->GetClump() == nil)
		return;
	int32 mi = obj->GetModelIndex();
	if(mi < MI_SPECIAL01 || mi > MI_SPECIAL21)
		return;
	if(!tried)
		Load();
	if(!ready)
		return;
	if(in == nil && (in = Create(obj)) == nil)
		return;
	if(in->clone == nil)
		return;
	RpHAnimHierarchy *hier = GetAnimHierarchyFromSkinClump(obj->GetClump());
	RwMatrix *mats = hier ? RpHAnimHierarchyGetMatrixArray(hier) : nil;
	if(mats == nil)
		return;
	RwV3d zero = { 0.0f, 0.0f, 0.0f };
	for(int32 side = 0; side < NUM_SIDES; side++){
		Side &s = in->side[side];
		if(!s.set.on)
			continue;
		int32 h = RpHAnimIDGetIndex(hier, handBone[side]);
		if(h < 0)
			continue;	// no hand bone: no replacement (the Xbox's would drift)
		int32 fa = RpHAnimIDGetIndex(hier, forearmBone[side]);
		const RwMatrix &axes = fa >= 0 ? mats[fa] : mats[h];
		// the hand bone's matrix, moved along the forearm's axes and scaled in its own
		RwMatrix target = mats[h];
		for(int32 k = 0; k < 3; k++){
			const RwV3d &ax = k == 0 ? axes.right : k == 1 ? axes.up : axes.at;
			target.pos.x += ax.x * s.set.position[k];
			target.pos.y += ax.y * s.set.position[k];
			target.pos.z += ax.z * s.set.position[k];
		}
		RwV3dScale(&target.right, &target.right, s.set.scale[0]);
		RwV3dScale(&target.up, &target.up, s.set.scale[1]);
		RwV3dScale(&target.at, &target.at, s.set.scale[2]);
		// the palm's matrix is relative to its parents (the hands model's root turns it):
		// local = target * inverse(parent LTM)
		RwFrame *parent = RwFrameGetParent(s.palm);
		if(parent){
			RwMatrix inv;
			RwMatrixInvert(&inv, RwFrameGetLTM(parent));
			rw::Matrix::mult(RwFrameGetMatrix(s.palm), &target, &inv);
		}else
			*RwFrameGetMatrix(s.palm) = target;
		RwMatrixUpdate(RwFrameGetMatrix(s.palm));
		RwFrameUpdateObjects(s.palm);
		if(!s.set.animStatic){
			s.time += CTimer::GetTimeStepInSeconds();
			SetPose(s);
		}
		RpHAnimHierarchyUpdateMatrices(s.hier);
		// the character's own hand collapsed under it (IDs the model lacks are skipped)
		for(int32 id : ownHand[side]){
			int32 i = RpHAnimIDGetIndex(hier, id);
			if(i >= 0){
				RwMatrixScale(&mats[i], &zero, rwCOMBINEPRECONCAT);
				RwMatrixUpdate(&mats[i]);
			}
		}
		s.draw = true;
	}
}

void
Render(CCutsceneObject *obj)
{
	Instance *in = Find(obj);
	if(in == nil)
		return;
	for(Side &s : in->side){
		if(!s.draw)
			continue;
		RpGeometry *g = RpAtomicGetGeometry(s.atomic);
		if(g->matList.numMaterials > 0){
			RwRGBA c = { s.set.rgb[0], s.set.rgb[1], s.set.rgb[2], 255 };
			RpMaterialSetColor(g->matList.materials[0], &c);
		}
		RpAtomicRender(s.atomic);
	}
}

void
EndCutscene(void)
{
	for(Instance &in : instances)
		FreeInstance(in);
}

void
Shutdown(void)
{
	Free();
	tried = false;
}

}
