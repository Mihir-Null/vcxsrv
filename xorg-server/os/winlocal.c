/* Copyright (c) 2026. SPDX-License-Identifier: MIT */
#if defined(_WIN32) && !defined(__CYGWIN__)
#if !defined(_WIN32_WINNT) || _WIN32_WINNT < 0x0600
#undef _WIN32_WINNT
#define _WIN32_WINNT 0x0600
#endif
#include <winsock2.h>
#include <windows.h>
#include "winlocal.h"
#include <aclapi.h>
#include <afunix.h>
#include <bcrypt.h>
#include <limits.h>
#include <stdio.h>
#if !defined(_WIN64)
#error winlocal requires Windows x64
#endif
static wchar_t directory[MAX_PATH], socket_path[MAX_PATH];
static char endpoint[sizeof(((SOCKADDR_UN*)0)->sun_path)];
static HANDLE directory_handle = INVALID_HANDLE_VALUE;
static int listener = -1;
static int cleanup_registered;
static SRWLOCK endpoint_lock = SRWLOCK_INIT;
int winLocalManaged = 0;
int winlocal_socket_to_fd(uintptr_t s)
{
    if (s > (uintptr_t)INT_MAX) {
        WSASetLastError(WSAEMFILE);
        return -1;
    }
    return (int)s;
}
const char* winlocal_endpoint(void) { return endpoint[0] ? endpoint : NULL; }
const wchar_t* winlocal_directory(void) { return directory[0] ? directory : NULL; }
int winlocal_listener_ready(void) { return listener >= 0; }
int winlocal_prepare(const wchar_t* base)
{
    wchar_t temp[MAX_PATH], candidate[MAX_PATH], path[MAX_PATH], name[36];
    unsigned char random[16];
    HANDLE token = NULL;
    TOKEN_USER* user = NULL;
    DWORD bytes = 0;
    PACL acl = NULL;
    EXPLICIT_ACCESSW access = { 0 };
    SECURITY_DESCRIPTOR sd;
    SECURITY_ATTRIBUTES sa = { sizeof(sa), &sd, FALSE };
    char encoded[sizeof(endpoint)];
    int i, ok = 0;
    if (endpoint[0])
        return 1;
    if (!cleanup_registered) {
        if (atexit(winlocal_cleanup) != 0)
            return 0;
        cleanup_registered = 1;
    }
    if (!base) {
        DWORD n = GetTempPathW(MAX_PATH, temp);
        if (!n || n >= MAX_PATH)
            return 0;
        base = temp;
    }
    /* Only absolute local drive paths; UNC endpoints are not local sockets. */
    if (wcslen(base) < 3 || base[1] != L':' || (base[2] != L'\\' && base[2] != L'/'))
        return 0;
    /* Reject before the secure CRT formatter, whose invalid-parameter handler
     * terminates the process for insufficient destination capacity. */
    if (wcslen(base) > MAX_PATH - 40)
        return 0;
    if (BCryptGenRandom(NULL, random, sizeof(random), BCRYPT_USE_SYSTEM_PREFERRED_RNG) != 0)
        return 0;
    for (i = 0; i < 16; i++)
        swprintf_s(name + i * 2, 36 - i * 2, L"%02x", random[i]);
    if (swprintf_s(candidate, MAX_PATH, L"%s\\vcx-%s", base, name) < 0
        || swprintf_s(path, MAX_PATH, L"%s\\X", candidate) < 0)
        return 0;
    if (!WideCharToMultiByte(
            CP_UTF8, WC_ERR_INVALID_CHARS, path, -1, encoded, sizeof(encoded), NULL, NULL))
        return 0;
    if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token))
        goto done;
    GetTokenInformation(token, TokenUser, NULL, 0, &bytes);
    user = (TOKEN_USER*)HeapAlloc(GetProcessHeap(), 0, bytes);
    if (!user)
        goto done;
    if (!GetTokenInformation(token, TokenUser, user, bytes, &bytes))
        goto done;
    access.grfAccessPermissions = FILE_ALL_ACCESS;
    access.grfAccessMode = SET_ACCESS;
    access.grfInheritance = SUB_CONTAINERS_AND_OBJECTS_INHERIT;
    access.Trustee.TrusteeForm = TRUSTEE_IS_SID;
    access.Trustee.TrusteeType = TRUSTEE_IS_USER;
    access.Trustee.ptstrName = (LPWSTR)user->User.Sid;
    if (SetEntriesInAclW(1, &access, NULL, &acl) != ERROR_SUCCESS)
        goto done;
    if (!InitializeSecurityDescriptor(&sd, SECURITY_DESCRIPTOR_REVISION)
        || !SetSecurityDescriptorDacl(&sd, TRUE, acl, FALSE)
        || !SetSecurityDescriptorControl(&sd, SE_DACL_PROTECTED, SE_DACL_PROTECTED))
        goto done;
    if (!CreateDirectoryW(candidate, &sa))
        goto done; /* Never adopt or delete an existing directory. */
    directory_handle
        = CreateFileW(candidate, FILE_READ_ATTRIBUTES, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL,
            OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT, NULL);
    if (directory_handle == INVALID_HANDLE_VALUE) {
        RemoveDirectoryW(candidate);
        goto done;
    }
    wcscpy_s(directory, MAX_PATH, candidate);
    wcscpy_s(socket_path, MAX_PATH, path);
    strcpy_s(endpoint, sizeof(endpoint), encoded);
    ok = 1;
done:
    if (acl)
        LocalFree(acl);
    if (user)
        HeapFree(GetProcessHeap(), 0, user);
    if (token)
        CloseHandle(token);
    return ok;
}
static int open_socket(void)
{
    SOCKET s = socket(AF_UNIX, SOCK_STREAM, 0);
    int fd;
    if (s == INVALID_SOCKET)
        return -1;
    fd = winlocal_socket_to_fd((uintptr_t)s);
    if (fd < 0)
        closesocket(s);
    return fd;
}
int winlocal_listen(void)
{
    SOCKADDR_UN addr = { 0 };
    int fd;
    WSADATA data;
    if (!endpoint[0] || listener >= 0) {
        WSASetLastError(WSAEINVAL);
        return -1;
    }
    if (WSAStartup(MAKEWORD(2, 2), &data))
        return -1;
    fd = open_socket();
    if (fd < 0) {
        WSACleanup();
        return -1;
    }
    addr.sun_family = AF_UNIX;
    strcpy_s(addr.sun_path, sizeof(addr.sun_path), endpoint);
    if (bind(fd, (struct sockaddr*)&addr, sizeof(addr)) == SOCKET_ERROR) {
        int e = WSAGetLastError();
        closesocket(fd);
        WSACleanup();
        WSASetLastError(e);
        return -1;
    }
    if (listen(fd, SOMAXCONN) == SOCKET_ERROR) {
        int e = WSAGetLastError();
        closesocket(fd);
        DeleteFileW(socket_path);
        WSACleanup();
        WSASetLastError(e);
        return -1;
    }
    listener = fd;
    return fd;
}
int winlocal_connect(void)
{
    SOCKADDR_UN addr = { 0 };
    int fd;
    /* Snapshot while protected; never hold cleanup behind a socket operation.
     * A racing final cleanup may cause connect/setup to fail, which is safe. */
    AcquireSRWLockShared(&endpoint_lock);
    if (!endpoint[0]) {
        ReleaseSRWLockShared(&endpoint_lock);
        WSASetLastError(WSAEINVAL);
        return -1;
    }
    addr.sun_family = AF_UNIX;
    strcpy_s(addr.sun_path, sizeof(addr.sun_path), endpoint);
    ReleaseSRWLockShared(&endpoint_lock);
    fd = open_socket();
    if (fd < 0)
        return -1;
    if (connect(fd, (struct sockaddr*)&addr, sizeof(addr)) == SOCKET_ERROR) {
        int e = WSAGetLastError();
        closesocket(fd);
        WSASetLastError(e);
        return -1;
    }
    return fd;
}
void winlocal_close_listener(int fd)
{
    if (fd != listener || fd < 0)
        return;
    closesocket(fd);
    listener = -1;
    DeleteFileW(socket_path);
    WSACleanup();
}
void winlocal_cleanup(void)
{
    AcquireSRWLockExclusive(&endpoint_lock);
    if (listener >= 0)
        winlocal_close_listener(listener);
    if (directory_handle != INVALID_HANDLE_VALUE) {
        CloseHandle(directory_handle);
        directory_handle = INVALID_HANDLE_VALUE;
    }
    if (directory[0])
        RemoveDirectoryW(directory);
    directory[0] = socket_path[0] = endpoint[0] = 0;
    ReleaseSRWLockExclusive(&endpoint_lock);
}
#endif
