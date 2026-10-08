#include <winsock2.h>
#include <windows.h>
#include "../../xorg-server/os/winlocal.h"
#include <aclapi.h>
#include <limits.h>
#include <stdio.h>
#define CHECK(x)                                                                                   \
    do {                                                                                           \
        if (!(x)) {                                                                                \
            fprintf(stderr, "FAIL line %d: %s (win=%lu wsa=%d)\n", __LINE__, #x, GetLastError(),   \
                WSAGetLastError());                                                                \
            return 1;                                                                              \
        }                                                                                          \
    } while (0)
struct connector_args { HANDLE started; HANDLE proceed; int failed; };
static DWORD WINAPI connect_during_cleanup(void *p) {
    struct connector_args *args = p;
    int fd = winlocal_connect();
    if (fd < 0) args->failed = 1;
    else closesocket(fd);
    SetEvent(args->started);
    WaitForSingleObject(args->proceed, 1000);
    fd = winlocal_connect();
    if (fd >= 0) { args->failed = 1; closesocket(fd); }
    return 0;
}
int main(void)
{
    WSADATA w;
    int l, c[80], a[80], i;
    char b[5];
    wchar_t dir[MAX_PATH];
    const char bytes[5] = { 0, 1, 127, -1, 0 };
    PSECURITY_DESCRIPTOR sd = NULL;
    PACL dacl;
    SECURITY_DESCRIPTOR_CONTROL control;
    DWORD rev;
    CHECK(WSAStartup(MAKEWORD(2, 2), &w) == 0);
    CHECK(winlocal_socket_to_fd((uintptr_t)INT_MAX) == INT_MAX);
    CHECK(winlocal_socket_to_fd((uintptr_t)INT_MAX + 1) == -1);
    CHECK(winlocal_socket_to_fd((uintptr_t)INVALID_SOCKET) == -1);
    CHECK(winlocal_prepare(NULL));
    wcscpy_s(dir, MAX_PATH, winlocal_directory());
    CHECK(GetNamedSecurityInfoW(
              dir, SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, NULL, NULL, &dacl, NULL, &sd)
        == ERROR_SUCCESS);
    CHECK(GetSecurityDescriptorControl(sd, &control, &rev));
    CHECK(control & SE_DACL_PROTECTED);
    CHECK(dacl && dacl->AceCount == 1);
    {
        ACCESS_ALLOWED_ACE* ace;
        HANDLE token;
        unsigned char info[1024];
        DWORD size;
        CHECK(GetAce(dacl, 0, (void**)&ace));
        CHECK(ace->Header.AceType == ACCESS_ALLOWED_ACE_TYPE);
        CHECK(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token));
        CHECK(GetTokenInformation(token, TokenUser, info, sizeof(info), &size));
        CHECK(EqualSid(&ace->SidStart, ((TOKEN_USER*)info)->User.Sid));
        CloseHandle(token);
    }
    LocalFree(sd);
    l = winlocal_listen();
    CHECK(l >= 0);
    CHECK(winlocal_listener_ready());
    CHECK(winlocal_listen() == -1);
    for (i = 0; i < 80; i++) {
        c[i] = winlocal_connect();
        CHECK(c[i] >= 0);
        a[i] = (int)accept(l, NULL, NULL);
        CHECK(a[i] >= 0);
        CHECK(send(c[i], bytes, 5, 0) == 5);
        CHECK(recv(a[i], b, 2, MSG_WAITALL) == 2);
        CHECK(recv(a[i], b + 2, 3, MSG_WAITALL) == 3);
        CHECK(memcmp(b, bytes, 5) == 0);
    }
    for (i = 0; i < 80; i++) {
        closesocket(c[i]);
        closesocket(a[i]);
    }
    winlocal_close_listener(l);
    CHECK(!winlocal_listener_ready());
    winlocal_cleanup();
    CHECK(GetFileAttributesW(dir) == INVALID_FILE_ATTRIBUTES);
    CHECK(winlocal_prepare(NULL));
    l = winlocal_listen();
    CHECK(l >= 0);
    winlocal_close_listener(l);
    winlocal_cleanup();
    {
        wchar_t too_long[200];
        for (i = 0; i < 199; i++)
            too_long[i] = 'a';
        too_long[199] = 0;
        CHECK(!winlocal_prepare(too_long));
        CHECK(!winlocal_endpoint());
    }
    /* A preexisting file at the socket path must never be adopted or deleted. */
    CHECK(winlocal_prepare(NULL));
    {
        HANDLE file;
        wchar_t path[MAX_PATH];
        DWORD written;
        CHECK(MultiByteToWideChar(CP_UTF8, 0, winlocal_endpoint(), -1, path, MAX_PATH));
        file = CreateFileW(path, GENERIC_WRITE, 0, NULL, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, NULL);
        CHECK(file != INVALID_HANDLE_VALUE);
        CHECK(WriteFile(file, "foreign", 7, &written, NULL) && written == 7);
        CloseHandle(file);
        CHECK(winlocal_listen() == -1);
        CHECK(!winlocal_listener_ready());
        CHECK(GetFileAttributesW(path) != INVALID_FILE_ATTRIBUTES);
        CHECK(DeleteFileW(path));
    }
    winlocal_cleanup();
    {
        wchar_t too_long[230];
        wcscpy_s(too_long, 230, L"C:\\");
        for (i = 3; i < 229; i++)
            too_long[i] = L'x';
        too_long[229] = 0;
        CHECK(!winlocal_prepare(too_long));
    }
    {
        struct connector_args args = {0}; HANDLE thread;
        CHECK(winlocal_prepare(NULL)); CHECK(winlocal_listen() >= 0);
        args.started = CreateEventW(NULL, TRUE, FALSE, NULL);
        args.proceed = CreateEventW(NULL, TRUE, FALSE, NULL);
        CHECK(args.started && args.proceed);
        thread = CreateThread(NULL, 0, connect_during_cleanup, &args, 0, NULL); CHECK(thread);
        CHECK(WaitForSingleObject(args.started, 1000) == WAIT_OBJECT_0);
        winlocal_cleanup(); SetEvent(args.proceed);
        CHECK(WaitForSingleObject(thread, 1000) == WAIT_OBJECT_0); CHECK(!args.failed);
        CloseHandle(thread); CloseHandle(args.started); CloseHandle(args.proceed);
    }
    WSACleanup();
    puts("PASS local lifecycle, ACL, narrowing, binary I/O, 80 clients, restart, path rejection");
    return 0;
}
