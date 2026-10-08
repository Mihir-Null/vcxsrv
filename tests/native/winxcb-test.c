/* Test the XCB ownership boundary, including allocation failure which cannot
 * reliably be forced in a live XCB DLL. Network calls are real AF_UNIX. */
#define LIBXCB_DLL
#include "../../xorg-server/hw/xwin/winxcb.h"
#include "../../xorg-server/os/winlocal.h"
#include <stdio.h>
#include <string.h>
static int mode, fd = -1, error;
static xcb_setup_t setup;
static int fake_conn;
xcb_connection_t* xcb_connect_to_fd(int sock, xcb_auth_info_t* auth)
{
    fd = sock;
    error = 0;
    if (mode == 1) {
        closesocket(sock);
        error = XCB_CONN_CLOSED_MEM_INSUFFICIENT;
    }
    if (mode == 2) {
        closesocket(sock);
        WSACleanup();
        error = XCB_CONN_ERROR;
    }
    return (xcb_connection_t*)&fake_conn;
}
int xcb_connection_has_error(xcb_connection_t* c) { return error; }
void xcb_disconnect(xcb_connection_t* c)
{
    if (!error) {
        closesocket(fd);
        WSACleanup();
    }
}
const xcb_setup_t* xcb_get_setup(xcb_connection_t* c) { return &setup; }
int xcb_setup_roots_length(const xcb_setup_t* s) { return s->roots_len; }
#define CHECK(x)                                                                                   \
    do {                                                                                           \
        if (!(x)) {                                                                                \
            fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #x);                                   \
            return 1;                                                                              \
        }                                                                                          \
    } while (0)
int main(void)
{
    WSADATA w;
    int l, a, i;
    char cookie[16] = { 0 };
    xcb_auth_info_t auth = { 18, "MIT-MAGIC-COOKIE-1", 16, cookie };
    xcb_connection_t* c;
    SOCKET probe;
    CHECK(WSAStartup(MAKEWORD(2, 2), &w) == 0);
    CHECK(winlocal_prepare(NULL));
    l = winlocal_listen();
    CHECK(l >= 0);
    setup.roots_len = 2;
    CHECK(winLocalXcbConnect(0, NULL) == NULL);
    for (i = 0; i < 30; i++) {
        mode = i % 3;
        c = winLocalXcbConnect(1, &auth);
        if (mode == 0) {
            CHECK(c != NULL);
            xcb_disconnect(c);
        } else
            CHECK(c == NULL);
        a = (int)accept(l, NULL, NULL);
        CHECK(a >= 0);
        closesocket(a);
        probe = socket(AF_UNIX, SOCK_STREAM, 0);
        CHECK(probe != INVALID_SOCKET);
        closesocket(probe);
    }
    mode = 0;
    CHECK(winLocalXcbConnect(2, &auth) == NULL);
    a = (int)accept(l, NULL, NULL);
    CHECK(a >= 0);
    closesocket(a);
    winlocal_close_listener(l);
    CHECK(winLocalXcbConnect(0, &auth) == NULL);
    winlocal_cleanup();
    WSACleanup();
    CHECK(socket(AF_UNIX, SOCK_STREAM, 0) == INVALID_SOCKET);
    CHECK(WSAGetLastError() == WSANOTINITIALISED);
    puts("PASS real local connect, auth precondition, screen bounds, XCB ownership "
         "success/reject/allocation/connect failures");
    return 0;
}
