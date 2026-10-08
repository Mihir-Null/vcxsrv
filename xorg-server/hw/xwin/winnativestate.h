/* Shared production owner-thread state callback. X property publication stays
 * in UpdateState; its caller uses NativeState.previous to suppress duplicates. */
#ifndef WINNATIVESTATE_H
#define WINNATIVESTATE_H
#include <stdint.h>
#include <xcb/xcb_icccm.h>
#include "winadmission.h"
extern const char WIN_STATE_PROP[];
#define XCB_ICCCM_WM_STATE_ZOOM 2
typedef struct { int state, previous; } NativeState;
static void ApplyState(HWND hwnd, void *data)
{
    NativeState *state = data;
    int command;
    state->previous = (int)(intptr_t)GetProp(hwnd, WIN_STATE_PROP);
    /* A new admission resets its show command, even if WIN_STATE_PROP survived.
     * Capture that command again without making unchanged X state look new. */
    if (state->previous == state->state && !winAdmissionPending(hwnd)) return;
    if (state->previous != state->state)
        SetProp(hwnd, WIN_STATE_PROP, (HANDLE)(intptr_t)state->state);
    switch (state->state) {
    case XCB_ICCCM_WM_STATE_ICONIC: command = SW_SHOWMINNOACTIVE; break;
    case XCB_ICCCM_WM_STATE_ZOOM: command = SW_SHOWMAXIMIZED; break;
    case XCB_ICCCM_WM_STATE_NORMAL: command = SW_SHOWNOACTIVATE; break;
    case XCB_ICCCM_WM_STATE_WITHDRAWN: command = SW_HIDE; break;
    default: return;
    }
    winAdmissionSetShowCommand(hwnd, command);
}

#endif
