/* Hyper-V discovery only publishes state; listener replacement belongs to the
 * X server main-thread timer. Bound polling is a host-only Toolhelp scan. */
#ifdef HAVE_DIX_CONFIG_H
#include <dix-config.h>
#endif
#include "win.h"
#include "winglobals.h"
#include "winmanaged.h"
#include "winwsldiscovery.h"
#include "../../os/winlocal.h"
#include "os.h"
#include "dix/dix_priv.h"
#ifdef HYPERV
#include <tlhelp32.h>
#include <pthread.h>
#define XSERV_t
#define TRANS_SERVER
#include <X11/Xtrans/Xtrans.h>
extern int HyperVRebindListener(void);
#define WIN_VSOCK_POLL_MS 500
#define WIN_VSOCK_TIMER_MS 500
#define WIN_VSOCK_GUID_LEN 40
static CRITICAL_SECTION g_vsockLock;
static BOOL g_vsockLockInited = FALSE;
static winWslDiscoveryState g_vsockState;
static BOOL g_vsockAuto = FALSE;
static BOOL g_vsockWatcherStarted = FALSE;

static BOOL
winWslVmPresent(void)
{
    HANDLE snap;
    PROCESSENTRY32W pe;
    BOOL found = FALSE;
    snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    if (snap == INVALID_HANDLE_VALUE)
        return FALSE;
    ZeroMemory(&pe, sizeof(pe));
    pe.dwSize = sizeof(pe);
    if (Process32FirstW(snap, &pe)) {
        do {
            if (_wcsicmp(pe.szExeFile, L"vmmemWSL") == 0 ||
                _wcsicmp(pe.szExeFile, L"vmmem") == 0) {
                found = TRUE;
                break;
            }
        } while (Process32NextW(snap, &pe));
    }
    CloseHandle(snap);
    return found;
}

static winWslResult
winVsockDiscover(char guid[37])
{
    const winWslRunner *runner = winWslDefaultRunner();
    wchar_t distro[WIN_WSL_NAME_CAP];
    DWORD start = GetTickCount(), elapsed;
    winWslResult result;
    if (g_pwszWslDistro)
        return winWslQueryVmId(runner, g_pwszWslDistro, guid, WIN_WSL_COMMAND_MS);
    result = winWslQueryRunningDistro(runner, NULL, distro, WIN_WSL_COMMAND_MS);
    if (result != WIN_WSL_SUCCESS) return result;
    elapsed = GetTickCount() - start;
    if (elapsed >= WIN_WSL_COMMAND_MS) return WIN_WSL_TIMEOUT;
    return winWslQueryVmId(runner, distro, guid, WIN_WSL_COMMAND_MS - elapsed);
}

static void *
winVsockWorkerProc(void *arg)
{
    (void)arg;
    for (;;) {
        BOOL query, present;
        unsigned serial;
        char guid[37];
        Sleep(WIN_VSOCK_POLL_MS);
        present = winWslVmPresent();
        EnterCriticalSection(&g_vsockLock);
        query = winWslObserveVm(&g_vsockState, present, WIN_VSOCK_POLL_MS);
        serial = g_vsockState.serial;
        LeaveCriticalSection(&g_vsockLock);
        if (query && winVsockDiscover(guid) == WIN_WSL_SUCCESS) {
            /* A stop during the query also cancels its result. This does not
             * make list/exec atomic: WSL can stop after the membership check. */
            present = winWslVmPresent();
            EnterCriticalSection(&g_vsockLock);
            if (!present) winWslObserveVm(&g_vsockState, FALSE, 0);
            else winWslPublishGuid(&g_vsockState, guid, serial);
            LeaveCriticalSection(&g_vsockLock);
        }
    }
    return NULL;
}

static CARD32
winVsockTimerProc(OsTimerPtr timer, CARD32 now, void *arg)
{
    static BOOL bindFailReported = FALSE;
    (void)now;
    (void)arg;
    /* Hold this short socket-only transition against worker invalidation.
     * No process execution or guest query happens on a timer tick. */
    EnterCriticalSection(&g_vsockLock);
    if (g_vsockState.dirty) {
        char braced[WIN_VSOCK_GUID_LEN], guid[37];
        unsigned serial = g_vsockState.serial;
        BOOL had = g_vsockState.bound[0] != 0;
        strcpy(guid, g_vsockState.pending);
        snprintf(braced, sizeof(braced), "{%s}", guid);
        _XSERVTransSetHyperVVmId(braced);
        if (HyperVRebindListener() == 0) {
            winWslAcceptGuid(&g_vsockState, guid, serial);
            ErrorF("winVsock: listener %s WSL2 VM %s%s\n",
                   had ? "rebound to" : "bound to", guid,
                   bindFailReported ? " (recovered after bind failure)" : "");
            bindFailReported = FALSE;
        }
        else if (!bindFailReported) {
            ErrorF("winVsock: listener bind to WSL2 VM %s failed; retrying\n", guid);
            bindFailReported = TRUE;
        }
    }
    LeaveCriticalSection(&g_vsockLock);
    TimerSet(timer, 0, WIN_VSOCK_TIMER_MS, winVsockTimerProc, NULL);
    return 0;
}

void
winVsockStartWatcher(void)
{
    pthread_t tid;
    if (serverGeneration != 1 || !g_vsockAuto || g_vsockWatcherStarted)
        return;
    if (!g_vsockLockInited) {
        InitializeCriticalSection(&g_vsockLock);
        g_vsockLockInited = TRUE;
    }
    if (pthread_create(&tid, NULL, winVsockWorkerProc, NULL)) {
        ErrorF("winVsock: watcher creation failed; VM restart discovery unavailable\n");
        return;
    }
    pthread_detach(tid);
    TimerSet(NULL, 0, WIN_VSOCK_TIMER_MS, winVsockTimerProc, NULL);
    g_vsockWatcherStarted = TRUE;
}

void
winVsockPreInit(int argc, char *argv[])
{
    char guid[37], braced[WIN_VSOCK_GUID_LEN];
    int manual = winVsockLegacyOverrides(winLocalManaged, argc, argv);
    winWslResult result = WIN_WSL_STOPPED;
    const char *status[] = { "success", "stopped/no WSL2 VM",
        "selected distro missing", "unsupported WSL version or output", "discovery timeout" };
    if (!g_fWslVsock) {
        if (!(manual & (WIN_VSOCK_MANUAL_LISTEN | WIN_VSOCK_MANUAL_VM)))
            _XSERVTransNoListen("hyperv");
        return;
    }
    if (manual & WIN_VSOCK_MANUAL_VM) {
        ErrorF("winVsock: explicit -vmid overrides WSL2 discovery\n");
        return;
    }
    if (manual & WIN_VSOCK_MANUAL_NOLISTEN) return;
    g_vsockAuto = TRUE;
    if (winWslVmPresent()) result = winVsockDiscover(guid);
    if (result == WIN_WSL_SUCCESS && winWslVmPresent()) {
        snprintf(braced, sizeof(braced), "{%s}", guid);
        _XSERVTransSetHyperVVmId(braced);
        if (!g_vsockLockInited) {
            InitializeCriticalSection(&g_vsockLock);
            g_vsockLockInited = TRUE;
        }
        EnterCriticalSection(&g_vsockLock);
        g_vsockState.wasUp = TRUE;
        strcpy(g_vsockState.bound, guid);
        LeaveCriticalSection(&g_vsockLock);
        ErrorF("winVsock: WSL2 VM %s detected; listener enabled\n", guid);
    }
    else {
        _XSERVTransNoListen("hyperv");
        ErrorF("winVsock: selected running WSL2 distro/VM unavailable (%s); "
               "listener disabled for now%s\n", status[result],
               winLocalManaged ? " (managed TCP remains disabled)" : "");
    }
}
#else
void winVsockPreInit(int argc, char *argv[]) { (void)argc; (void)argv; }
void winVsockStartWatcher(void) { }
#endif
