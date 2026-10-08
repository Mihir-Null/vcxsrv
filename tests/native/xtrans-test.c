#include <direct.h>
#define S_ISDIR(m) (((m) & _S_IFMT) == _S_IFDIR)
#include <stdarg.h>
#include <stdio.h>
__declspec(dllexport) void ErrorF(const char* fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    vfprintf(stderr, fmt, ap);
    va_end(ap);
}
void VErrorF(const char* fmt, va_list ap) { vfprintf(stderr, fmt, ap); }
#include "../../xorg-server/os/winlocal.h"
#include <stdlib.h>
static int fail_listener_array;
static void *fixture_malloc(size_t n) {
    if (fail_listener_array && n == sizeof(void *)) { fail_listener_array = 0; return NULL; }
    return malloc(n);
}
#define malloc fixture_malloc
#include "../../xorg-server/os/xstrans.c"
void LogVMessageVerb(MessageType type, int verb, const char* fmt, va_list args)
{
    vfprintf(stderr, fmt, args);
}
void FatalError(const char* fmt, ...) { exit(2); }
#define CHECK(x)                                                                                   \
    do {                                                                                           \
        if (!(x)) {                                                                                \
            fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #x);                                   \
            return 1;                                                                              \
        }                                                                                          \
    } while (0)
int main(void)
{
    XtransConnInfo l, a;
    int c, status, len, family;
    Xtransaddr* addr;
    char data[3];
    WSADATA w;
    CHECK(WSAStartup(MAKEWORD(2, 2), &w) == 0);
    CHECK(winlocal_prepare(NULL));
    l = _XSERVTransOpenCOTSServer("winlocal/:0");
    CHECK(l);
    CHECK(_XSERVTransCreateListener(l, "0", 0) == 0);
    c = winlocal_connect();
    CHECK(c >= 0);
    a = _XSERVTransAccept(l, &status);
    CHECK(a && status == 0);
    CHECK(_XSERVTransIsLocal(a));
    CHECK(!(a->flags & TRANS_NOXAUTH));
    CHECK(_XSERVTransGetPeerAddr(a, &family, &len, &addr) == 0);
    CHECK(family == AF_UNIX && len >= 2);
    free(addr);
    CHECK(send(c, "a\0b", 3, 0) == 3);
    CHECK(_XSERVTransRead(a, data, 3) == 3);
    CHECK(memcmp(data, "a\0b", 3) == 0);
    _XSERVTransClose(a);
    closesocket(c);
    CHECK(winlocal_listener_ready());
    CHECK(_XSERVTransResetListener(l) == TRANS_RESET_NOOP);
    _XSERVTransClose(l);
    CHECK(!winlocal_listener_ready());
    /* Failure allocating the final listener array must unwind the live socket. */
    {
        XtransConnInfo *listeners = NULL;
        int count = 0, partial = 0;
        _XSERVTransNoListen("tcp");
        _XSERVTransNoListen("inet");
        _XSERVTransNoListen("inet6");
        _XSERVTransNoListen("hyperv");
        _XSERVTransListen("winlocal");
        fail_listener_array = 1;
        CHECK(_XSERVTransMakeAllCOTSServerListeners("0", &partial, &count, &listeners) == -1);
        CHECK(!winlocal_listener_ready());
        CHECK(count == 0 && listeners == NULL);
    }
    winlocal_cleanup();
    WSACleanup();
    puts("PASS actual xtrans listener, auth-capable accept, address metadata, I/O, reset and owner "
         "cleanup");
    return 0;
}
