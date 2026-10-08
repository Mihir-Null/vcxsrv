#include <winsock2.h>
#include <windows.h>
#include "../../xorg-server/os/winlocal.h"
#include "../../xorg-server/os/winpoll.h"
#include <hvsocket.h>
#include <stdio.h>
#define CHECK(x)                                                                                   \
    do {                                                                                           \
        if (!(x)) {                                                                                \
            fprintf(stderr, "FAIL line %d: %s wsa=%d\n", __LINE__, #x, WSAGetLastError());         \
            return 1;                                                                              \
        }                                                                                          \
    } while (0)
static DWORD WINAPI signal_later(void* p)
{
    Sleep(60);
    SetEvent((HANDLE)p);
    return 0;
}
static DWORD WINAPI send_later(void* p)
{
    Sleep(60);
    send((SOCKET)(uintptr_t)p, "z", 1, 0);
    return 0;
}
struct drain_args {
    SOCKET fd;
    int count;
};
static DWORD WINAPI drain_later(void* p)
{
    struct drain_args* arg = p;
    char buf[16384];
    int n;
    Sleep(60);
    while (arg->count > 0) {
        n = recv(arg->fd, buf, arg->count > (int)sizeof(buf) ? sizeof(buf) : arg->count, 0);
        if (n <= 0)
            return 1;
        arg->count -= n;
    }
    return 0;
}
int main(void)
{
    WSADATA w;
    int l, c[80], a[80], i;
    char b;
    struct winpollfd f[81];
    HANDLE cancel, thread;
    ULONGLONG start;
    SOCKET hv;
    SOCKADDR_HV addr = { 0 };
    WSAPROTOCOL_INFOW unixinfo, hvinfo;
    int n = sizeof(unixinfo);
    CHECK(WSAStartup(MAKEWORD(2, 2), &w) == 0);
    CHECK(winlocal_prepare(NULL));
    l = winlocal_listen();
    CHECK(l >= 0);
    c[0] = winlocal_connect();
    CHECK(c[0] >= 0);
    f[0] = (struct winpollfd) { l, WINPOLL_IN, 0 };
    CHECK(winpoll_wait(f, 1, 100, NULL) == 1);
    CHECK(f[0].revents & WINPOLL_IN);
    a[0] = (int)accept(l, NULL, NULL);
    CHECK(a[0] >= 0);
    for (i = 1; i < 80; i++) {
        c[i] = winlocal_connect();
        CHECK(c[i] >= 0);
        a[i] = (int)accept(l, NULL, NULL);
        CHECK(a[i] >= 0);
    }
    for (i = 0; i < 80; i++)
        f[i] = (struct winpollfd) { a[i], WINPOLL_IN, 0 };
    start = GetTickCount64();
    CHECK(winpoll_wait(f, 80, 80, NULL) == 0);
    CHECK(GetTickCount64() - start >= 60);
    thread = CreateThread(NULL, 0, send_later, (void*)(uintptr_t)c[79], 0, NULL);
    CHECK(thread);
    CHECK(winpoll_wait(f, 80, 1000, NULL) == 1);
    CHECK(f[79].revents & WINPOLL_IN);
    CHECK(recv(a[79], &b, 1, 0) == 1);
    WaitForSingleObject(thread, 1000);
    CloseHandle(thread);
    f[0].events = WINPOLL_OUT;
    CHECK(winpoll_wait(f, 1, 100, NULL) == 1);
    CHECK(f[0].revents & WINPOLL_OUT);
    f[0].events = WINPOLL_IN;
    for (i = 0; i < 80; i++)
        CHECK(send(c[i], "x", 1, 0) == 1);
    CHECK(winpoll_wait(f, 80, 1000, NULL) == 80);
    for (i = 0; i < 80; i++)
        CHECK(recv(a[i], &b, 1, 0) == 1);
    /* Partial reads must remain level-ready after a fresh registration. */
    CHECK(send(c[79], "xy", 2, 0) == 2);
    CHECK(winpoll_wait(f, 80, 100, NULL) == 1);
    CHECK(recv(a[79], &b, 1, 0) == 1);
    CHECK(winpoll_wait(f, 80, 100, NULL) == 1);
    CHECK(recv(a[79], &b, 1, 0) == 1);
    /* FD_WRITE must wake again after real send backpressure, not only connect. */
    {
        char payload[16384] = { 0 };
        int sent, total = 0;
        struct drain_args arg;
        do {
            sent = send(a[1], payload, sizeof(payload), 0);
            if (sent > 0)
                total += sent;
        } while (sent > 0 && total < 64 * 1024 * 1024);
        CHECK(sent == SOCKET_ERROR && WSAGetLastError() == WSAEWOULDBLOCK);
        f[1].events = WINPOLL_OUT;
        CHECK(winpoll_wait(&f[1], 1, 50, NULL) == 0);
        arg.fd = c[1];
        arg.count = total;
        thread = CreateThread(NULL, 0, drain_later, &arg, 0, NULL);
        CHECK(thread);
        CHECK(winpoll_wait(&f[1], 1, 1000, NULL) == 1);
        CHECK(f[1].revents & WINPOLL_OUT);
        CHECK(WaitForSingleObject(thread, 2000) == WAIT_OBJECT_0);
        CloseHandle(thread);
        CHECK(arg.count == 0);
        f[1].events = WINPOLL_IN;
    }
    cancel = CreateEventW(NULL, TRUE, FALSE, NULL);
    CHECK(cancel);
    thread = CreateThread(NULL, 0, signal_later, cancel, 0, NULL);
    CHECK(thread);
    CHECK(winpoll_wait(f, 80, -1, cancel) == -1);
    CHECK(WSAGetLastError() == WSAEINTR);
    WaitForSingleObject(thread, 1000);
    CloseHandle(thread);
    CloseHandle(cancel);
    closesocket(c[0]);
    c[0] = -1;
    CHECK(winpoll_wait(f, 80, 1000, NULL) >= 1);
    CHECK(f[0].revents & (WINPOLL_IN | WINPOLL_HUP));
    CHECK(recv(a[0], &b, 1, 0) == 0);
    CHECK(getsockopt(a[1], SOL_SOCKET, SO_PROTOCOL_INFOW, (char*)&unixinfo, &n) == 0);
    hv = socket(AF_HYPERV, SOCK_STREAM, HV_PROTOCOL_RAW);
    CHECK(hv != INVALID_SOCKET);
    n = sizeof(hvinfo);
    CHECK(getsockopt(hv, SOL_SOCKET, SO_PROTOCOL_INFOW, (char*)&hvinfo, &n) == 0);
    CHECK(memcmp(&unixinfo.ProviderId, &hvinfo.ProviderId, sizeof(GUID)) != 0);
    addr.Family = AF_HYPERV;
    addr.VmId = HV_GUID_LOOPBACK;
    addr.ServiceId.Data1 = 0x91e88faa;
    addr.ServiceId.Data2 = 0x5a49;
    addr.ServiceId.Data3 = 0x471e;
    addr.ServiceId.Data4[0] = 0xb1;
    addr.ServiceId.Data4[7] = 0xa1;
    if (bind(hv, (struct sockaddr*)&addr, sizeof(addr)) == 0 && listen(hv, 16) == 0) {
        SOCKET hc = socket(AF_HYPERV, SOCK_STREAM, HV_PROTOCOL_RAW);
        SOCKET ha;
        u_long nonblock = 1;
        CHECK(ioctlsocket(hc, FIONBIO, &nonblock) == 0);
        i = connect(hc, (struct sockaddr*)&addr, sizeof(addr));
        CHECK(i == 0 || WSAGetLastError() == WSAEWOULDBLOCK);
        f[0] = (struct winpollfd) { (int)hv, WINPOLL_IN, 0 };
        f[1] = (struct winpollfd) { a[1], WINPOLL_IN, 0 };
        CHECK(send(c[1], "u", 1, 0) == 1);
        CHECK(winpoll_wait(f, 2, 1000, NULL) >= 1);
        CHECK(f[1].revents & WINPOLL_IN);
        f[0].revents = 0;
        CHECK(winpoll_wait(f, 1, 1000, NULL) == 1);
        ha = accept(hv, NULL, NULL);
        CHECK(ha != INVALID_SOCKET);
        CHECK(send(hc, "h", 1, 0) == 1);
        f[0] = (struct winpollfd) { (int)ha, WINPOLL_IN, 0 };
        CHECK(winpoll_wait(f, 2, 1000, NULL) >= 1);
        CHECK(f[1].revents & WINPOLL_IN);
        CHECK(recv(a[1], &b, 1, 0) == 1 && b == 'u');
        CHECK(winpoll_wait(f, 2, 1000, NULL) >= 1);
        CHECK(f[0].revents & WINPOLL_IN);
        CHECK(recv(ha, &b, 1, 0) == 1 && b == 'h');
        closesocket(hc);
        closesocket(ha);
        puts("PASS actual mixed AF_UNIX / AF_HYPERV providers");
    } else {
        printf("LIMITATION Hyper-V loopback bind/listen unavailable: %d\n", WSAGetLastError());
    }
    closesocket(hv);
    for (i = 0; i < 80; i++) {
        if (c[i] >= 0)
            closesocket(c[i]);
        closesocket(a[i]);
    }
    winlocal_cleanup();
    WSACleanup();
    puts("PASS accept, read/write, EOF, timeout, cancellation, 80 descriptors");
    return 0;
}
