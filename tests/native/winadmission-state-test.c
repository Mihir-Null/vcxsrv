/* Exercise the exact owner callback shared with winmultiwindowwm.c. */
#define FD_SETSIZE 1024
#include <winsock2.h>
#include "../../xorg-server/hw/xwin/winnativestate.h"
#include <stdio.h>
#include <stdlib.h>
#define CHECK(condition) do { if (!(condition)) { fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #condition); exit(1); } } while (0)

const char WIN_STATE_PROP[] = "vcxsrv_state_prop_rl";

static int failures;
static void check_state(HWND hwnd, int state, const char *phase)
{
    BOOL valid = state == XCB_ICCCM_WM_STATE_ICONIC ? IsIconic(hwnd) : IsZoomed(hwnd);
    if (!valid) {
        fprintf(stderr, "FAIL %s state=%d: iconic=%d zoomed=%d visible=%d\n",
                phase, state, IsIconic(hwnd), IsZoomed(hwnd), IsWindowVisible(hwnd));
        failures++;
    }
}
int main(void)
{
    WNDCLASSW wc = {0};
    int states[] = {XCB_ICCCM_WM_STATE_ICONIC, XCB_ICCCM_WM_STATE_ZOOM};
    int i;
    wc.lpfnWndProc = DefWindowProcW;
    wc.hInstance = GetModuleHandle(NULL);
    wc.lpszClassName = L"admission-state-test";
    CHECK(RegisterClassW(&wc));
    for (i = 0; i < sizeof(states)/sizeof(states[0]); i++) {
        /* TOOLWINDOW keeps the desktop tiler from changing this state fixture. */
        HWND hwnd = CreateWindowExW(WS_EX_TOOLWINDOW, wc.lpszClassName, L"ApplyState regression",
                                    WS_OVERLAPPEDWINDOW, 40, 40, 200, 120,
                                    NULL, NULL, wc.hInstance, NULL);
        NativeState native = {states[i], -1};
        ULONG_PTR token;
        CHECK(hwnd);
        token = winAdmissionBegin(hwnd, 42);
        winPrepareNativeWindow(hwnd);
        ApplyState(hwnd, &native);
        CHECK(!IsWindowVisible(hwnd));
        CHECK(native.previous == XCB_ICCCM_WM_STATE_WITHDRAWN);
        CHECK(winAdmissionComplete(hwnd, 42, token, TRUE));
        check_state(hwnd, states[i], "initial");

        /* Cancellation leaves WIN_STATE_PROP on a surviving native window. */
        ShowWindow(hwnd, SW_HIDE);
        winAdmissionCancel(hwnd);
        token = winAdmissionBegin(hwnd, 42);
        winPrepareNativeWindow(hwnd);
        native.previous = -1;
        ApplyState(hwnd, &native);
        CHECK(!IsWindowVisible(hwnd));
        /* This unchanged result must keep UpdateState's X-publication no-op. */
        CHECK(native.previous == native.state);
        CHECK(winAdmissionComplete(hwnd, 42, token, TRUE));
        check_state(hwnd, states[i], "same-state remap");

        /* Outside pending admission, repeated state must remain a no-op. */
        ShowWindow(hwnd, SW_HIDE);
        ApplyState(hwnd, &native);
        CHECK(!IsWindowVisible(hwnd));
        CHECK(native.previous == native.state);
        winAdmissionCancel(hwnd);
        ApplyState(hwnd, &native);
        CHECK(!IsWindowVisible(hwnd)); /* generic window also stays hidden */
        RemoveProp(hwnd, WIN_STATE_PROP);
        DestroyWindow(hwnd);
    }
    if (failures) return 1;
    puts("PASS production ApplyState: iconic/zoom surviving-HWND remap and generic same-state no-op");
    return 0;
}
