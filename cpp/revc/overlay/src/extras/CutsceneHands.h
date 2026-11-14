#pragma once
// revc (roadmap R4): the Xbox's cutscene hands, in the Xbox UI style (CutsceneHands.cpp).
class CCutsceneObject;

namespace CutsceneHands {
void PreRender(CCutsceneObject *obj);	// after the character's bones are updated
void Render(CCutsceneObject *obj);	// before the character is drawn
void EndCutscene(void);	// the cutscene's objects are gone
void Shutdown(void);
}
