#include "../../xorg-server/hw/xwin/winxcb.h"
#include "../../xorg-server/os/winlocal.h"
#include <stdio.h>
#include <string.h>
#define CHECK(x)                                                                                   \
    do {                                                                                           \
        if (!(x)) {                                                                                \
            fprintf(stderr, "FAIL line %d: %s wsa=%d\n", __LINE__, #x, WSAGetLastError());         \
            return 1;                                                                              \
        }                                                                                          \
    } while (0)
static int listener, reject_setup, fixture_ok;
static const char cookie[16] = { 1, 0, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16 };
static int receive_all(int fd, char* p, int n)
{
    int r;
    while (n) {
        r = recv(fd, p, n, 0);
        if (r <= 0)
            return 0;
        p += r;
        n -= r;
    }
    return 1;
}
static DWORD WINAPI setup_server(void* arg)
{
    int a = (int)accept(listener, NULL, NULL);
    unsigned char request[48];
    struct {
        xcb_setup_t setup;
        xcb_screen_t screen;
    } reply = { 0 };
    char b;
    if (a < 0)
        return 1;
    if (!receive_all(a, (char*)request, sizeof(request)))
        goto done;
    if (request[0] != 'l' || request[6] != 18 || request[8] != 16
        || memcmp(request + 12, "MIT-MAGIC-COOKIE-1", 18) || memcmp(request + 32, cookie, 16))
        goto done;
    fixture_ok = 1;
    reply.setup.status = reject_setup ? 0 : 1;
    reply.setup.protocol_major_version = 11;
    reply.setup.length = (sizeof(reply) - 8) / 4;
    reply.setup.resource_id_base = 0x200000;
    reply.setup.resource_id_mask = 0x1fffff;
    reply.setup.maximum_request_length = 65535;
    reply.setup.roots_len = 1;
    reply.setup.min_keycode = 8;
    reply.setup.max_keycode = 255;
    reply.screen.root = 1;
    reply.screen.width_in_pixels = 800;
    reply.screen.height_in_pixels = 600;
    reply.screen.root_depth = 24;
    if (reject_setup) {
        reply.setup.length = 0;
        send(a, (char*)&reply, 8, 0);
    } else
        send(a, (char*)&reply, sizeof(reply), 0);
    while (recv(a, &b, 1, 0) > 0) { }
done:
    closesocket(a);
    return 0;
}
int main(void)
{
    WSADATA w;
    HANDLE thread;
    xcb_connection_t* c;
    int i;
    xcb_auth_info_t auth = { 18, "MIT-MAGIC-COOKIE-1", 16, (char*)cookie };
    SOCKET sentinel;
    CHECK(WSAStartup(MAKEWORD(2, 2), &w) == 0);
    CHECK(winlocal_prepare(NULL));
    listener = winlocal_listen();
    CHECK(listener >= 0);
    sentinel = socket(AF_UNIX, SOCK_STREAM, 0);
    CHECK(sentinel != INVALID_SOCKET);
    for (i = 0; i < 12; i++) {
        reject_setup = i % 2;
        fixture_ok = 0;
        thread = CreateThread(NULL, 0, setup_server, NULL, 0, NULL);
        CHECK(thread);
        c = winLocalXcbConnect(0, &auth);
        if (reject_setup)
            CHECK(c == NULL);
        else {
            CHECK(c != NULL);
            CHECK(!xcb_connection_has_error(c));
            xcb_disconnect(c);
        }
        CHECK(WaitForSingleObject(thread, 2000) == WAIT_OBJECT_0);
        CloseHandle(thread);
        CHECK(fixture_ok);
        {
            int type, n = sizeof(type);
            CHECK(getsockopt(sentinel, SOL_SOCKET, SO_TYPE, (char*)&type, &n) == 0);
        }
    }
    closesocket(sentinel);
    winlocal_cleanup();
    WSACleanup();
    CHECK(socket(AF_UNIX, SOCK_STREAM, 0) == INVALID_SOCKET);
    CHECK(WSAGetLastError() == WSANOTINITIALISED);
    puts("PASS bundled XCB: authenticated binary setup over AF_UNIX, success/rejection, balanced "
         "Winsock ownership");
    return 0;
}
