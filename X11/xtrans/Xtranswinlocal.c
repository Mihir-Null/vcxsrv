/* Native server-only transport. Internal clients connect to the retained
 * endpoint directly; this intentionally does not extend libxcb DISPLAY. */
#include "../../xorg-server/os/winlocal.h"
#include <afunix.h>
static XtransConnInfo TRANS(WinLocalOpen)(
    Xtransport* t, const char* p, const char* h, const char* port)
{
    XtransConnInfo c;
    (void)t;
    (void)p;
    (void)h;
    (void)port;
    if (!winlocal_endpoint())
        return NULL;
    c = calloc(1, sizeof(*c));
    if (c) {
        c->fd = -1;
        c->family = AF_UNIX;
    }
    return c;
}
static int TRANS(WinLocalAddresses)(XtransConnInfo c, int peer)
{
    SOCKADDR_UN addr = { 0 };
    int len = sizeof(addr);
    if (getsockname(c->fd, (struct sockaddr*)&addr, &len) == SOCKET_ERROR)
        return -1;
    c->addr = malloc(len);
    if (!c->addr)
        return -1;
    memcpy(c->addr, &addr, len);
    c->addrlen = len;
    c->family = AF_UNIX;
    if (peer) {
        memset(&addr, 0, sizeof(addr));
        len = sizeof(addr);
        if (getpeername(c->fd, (struct sockaddr*)&addr, &len) == SOCKET_ERROR)
            return -1;
        /* Windows unnamed peers contain only the address family. */
        if (len < (int)sizeof(addr.sun_family))
            return -1;
        c->peeraddr = malloc(len);
        if (!c->peeraddr)
            return -1;
        memcpy(c->peeraddr, &addr, len);
        c->peeraddrlen = len;
    }
    return 0;
}
static int TRANS(WinLocalCreateListener)(XtransConnInfo c, const char* port, unsigned int flags)
{
    (void)port;
    (void)flags;
    c->fd = winlocal_listen();
    if (c->fd < 0)
        return TRANS_CREATE_LISTENER_FAILED;
    if (TRANS(WinLocalAddresses)(c, 0) < 0) {
        winlocal_close_listener(c->fd);
        c->fd = -1;
        return TRANS_CREATE_LISTENER_FAILED;
    }
    c->flags |= TRANS_LOCAL;
    return 0;
}
static int TRANS(WinLocalReset)(XtransConnInfo c)
{
    return c->fd >= 0 && winlocal_listener_ready() ? TRANS_RESET_NOOP : TRANS_RESET_FAILURE;
}
static XtransConnInfo TRANS(WinLocalAccept)(XtransConnInfo c, int* status)
{
    SOCKET socket_value;
    XtransConnInfo a = calloc(1, sizeof(*a));
    *status = TRANS_ACCEPT_BAD_MALLOC;
    if (!a)
        return NULL;
    *status = TRANS_ACCEPT_FAILED;
    socket_value = accept(c->fd, NULL, NULL);
    if (socket_value == INVALID_SOCKET) {
        errno = WSAGetLastError();
        free(a);
        return NULL;
    }
    a->fd = winlocal_socket_to_fd((uintptr_t)socket_value);
    if (a->fd < 0) {
        closesocket(socket_value);
        free(a);
        return NULL;
    }
    if (TRANS(WinLocalAddresses)(a, 1) < 0) {
        closesocket(socket_value);
        TRANS(FreeConnInfo)(a);
        return NULL;
    }
    *status = 0;
    return a; /* Normal authorization; never TRANS_NOXAUTH. */
}
static int TRANS(WinLocalClose)(XtransConnInfo c)
{
    if (c->flags & TRANS_LOCAL)
        winlocal_close_listener(c->fd);
    else if (c->fd >= 0)
        closesocket(c->fd);
    return 0;
}
static Xtransport TRANS(WinLocalFuncs) = { "winlocal", TRANS_NOLISTEN, NULL, TRANS(WinLocalOpen),
#ifdef TRANS_REOPEN
    NULL,
#endif
    TRANS(SocketSetOption), TRANS(WinLocalCreateListener), TRANS(WinLocalReset),
    TRANS(WinLocalAccept), TRANS(SocketBytesReadable), TRANS(SocketRead), TRANS(SocketWrite),
    TRANS(SocketReadv), TRANS(SocketWritev), TRANS(SocketSendFdInvalid), TRANS(SocketRecvFdInvalid),
    TRANS(SocketDisconnect), TRANS(WinLocalClose), TRANS(WinLocalClose) };
