#ifndef WIN_WSL_DISCOVERY_H
#define WIN_WSL_DISCOVERY_H
#include <windows.h>
#include <stddef.h>
#define WIN_WSL_NAME_CAP 257
#define WIN_WSL_OUTPUT_CAP 65536
#define WIN_WSL_COMMAND_MS 5000

typedef enum { WIN_WSL_SUCCESS, WIN_WSL_STOPPED, WIN_WSL_MISSING,
               WIN_WSL_UNSUPPORTED, WIN_WSL_TIMEOUT } winWslResult;
typedef winWslResult (*winWslRunFn)(void *, const wchar_t *const *,
    unsigned char *, size_t, size_t *, DWORD);
typedef unsigned (*winWslVersionFn)(void *, const wchar_t *);
typedef struct { winWslRunFn run; winWslVersionFn version; void *context; } winWslRunner;
typedef struct {
    char bound[37], pending[37];
    BOOL dirty, wasUp;
    unsigned serial;
    DWORD stateTick;
} winWslDiscoveryState;
winWslResult winWslDecode(const unsigned char *, size_t, wchar_t *, size_t);
BOOL winWslValidName(const wchar_t *);
BOOL winWslParseGuid(const unsigned char *, size_t, char [37]);
BOOL winWslQuoteArgs(const wchar_t *const *, wchar_t *, size_t);
BOOL winWslDistroArgument(int, wchar_t [WIN_WSL_NAME_CAP]);
BOOL winWslSystemPath(wchar_t *, size_t);
winWslResult winWslRunProcess(const wchar_t *, const wchar_t *const *,
    unsigned char *, size_t, size_t *, DWORD);
const winWslRunner *winWslDefaultRunner(void);
winWslResult winWslQueryRunningDistro(const winWslRunner *, const wchar_t *,
    wchar_t [WIN_WSL_NAME_CAP], DWORD);
winWslResult winWslQueryVmId(const winWslRunner *, const wchar_t *, char [37], DWORD);
BOOL winWslObserveVm(winWslDiscoveryState *, BOOL, DWORD);
BOOL winWslPublishGuid(winWslDiscoveryState *, const char *, unsigned);
BOOL winWslAcceptGuid(winWslDiscoveryState *, const char *, unsigned);
#endif
