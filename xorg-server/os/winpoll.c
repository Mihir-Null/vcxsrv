/* Copyright (c) 2026. SPDX-License-Identifier: MIT */
#if defined(_WIN32) && !defined(__CYGWIN__)
#if !defined(_WIN32_WINNT) || _WIN32_WINNT < 0x0600
#undef _WIN32_WINNT
#define _WIN32_WINNT 0x0600
#endif
#include <winsock2.h>
#include <windows.h>
#include "winpoll.h"
#include <stdlib.h>
/* A shared event is only a wake hint. Reset BEFORE scanning every socket;
 * events arriving during/after the scan then remain signalled. Enumerating
 * with NULL clears each socket's record without resetting the shared event.
 * Level readiness is probed using one socket per select, so distinct Winsock
 * providers never enter the same select. Only two wait handles are needed,
 * independent of descriptor count. No helper threads or dispatch callbacks.
 * Each invocation owns its registrations, including all failure unwinding. */
int winpoll_wait(struct winpollfd* fds, unsigned long count, int timeout, void* cancel)
{
    HANDLE wake = CreateEventW(NULL, TRUE, FALSE, NULL), handles[2];
    unsigned char* registered = NULL;
    unsigned long i;
    int result = -1, error = 0;
    ULONGLONG start = GetTickCount64();
    if (!wake) {
        WSASetLastError(WSA_NOT_ENOUGH_MEMORY);
        return -1;
    }
    if (count) {
        registered = (unsigned char*)calloc(count, 1);
        if (!registered) {
            error = WSA_NOT_ENOUGH_MEMORY;
            goto done;
        }
    }
    handles[0] = wake;
    handles[1] = (HANDLE)cancel;
    for (i = 0; i < count; i++) {
        long mask = FD_CLOSE;
        fds[i].revents = 0;
        if (fds[i].fd < 0)
            continue;
        if (fds[i].events & WINPOLL_IN)
            mask |= FD_READ | FD_ACCEPT;
        if (fds[i].events & WINPOLL_OUT)
            mask |= FD_WRITE;
        if (fds[i].events & WINPOLL_PRI)
            mask |= FD_OOB;
        if (WSAEventSelect(fds[i].fd, wake, mask) == SOCKET_ERROR) {
            error = WSAGetLastError();
            if (error == WSAENOTSOCK) {
                fds[i].revents = WINPOLL_NVAL;
                continue;
            }
            goto done;
        }
        registered[i] = 1;
    }
    for (;;) {
        DWORD wait_ms, status;
        ULONGLONG elapsed;
        result = 0;
        ResetEvent(wake);
        if (cancel && WaitForSingleObject(cancel, 0) == WAIT_OBJECT_0) {
            error = WSAEINTR;
            result = -1;
            break;
        }
        for (i = 0; i < count; i++) {
            fd_set rd, wr, ex;
            struct timeval zero = { 0, 0 };
            WSANETWORKEVENTS ne;
            int k, rc;
            if (registered[i]) {
                if (WSAEnumNetworkEvents(fds[i].fd, NULL, &ne) == SOCKET_ERROR) {
                    error = WSAGetLastError();
                    result = -1;
                    goto done;
                }
                if (ne.lNetworkEvents & FD_CLOSE)
                    fds[i].revents |= WINPOLL_HUP;
                for (k = 0; k < FD_MAX_EVENTS; k++)
                    if ((ne.lNetworkEvents & (1L << k)) && ne.iErrorCode[k])
                        fds[i].revents |= WINPOLL_ERR;
                FD_ZERO(&rd);
                FD_ZERO(&wr);
                FD_ZERO(&ex);
                if (fds[i].events & WINPOLL_IN)
                    FD_SET((SOCKET)fds[i].fd, &rd);
                if (fds[i].events & WINPOLL_OUT)
                    FD_SET((SOCKET)fds[i].fd, &wr);
                if (fds[i].events & WINPOLL_PRI)
                    FD_SET((SOCKET)fds[i].fd, &ex);
                if (rd.fd_count || wr.fd_count || ex.fd_count) {
                    rc = select(0, &rd, &wr, &ex, &zero);
                    if (rc == SOCKET_ERROR)
                        fds[i].revents
                            |= WSAGetLastError() == WSAENOTSOCK ? WINPOLL_NVAL : WINPOLL_ERR;
                    else {
                        if (FD_ISSET((SOCKET)fds[i].fd, &rd))
                            fds[i].revents |= WINPOLL_IN;
                        if (FD_ISSET((SOCKET)fds[i].fd, &wr))
                            fds[i].revents |= WINPOLL_OUT;
                        if (FD_ISSET((SOCKET)fds[i].fd, &ex))
                            fds[i].revents |= WINPOLL_PRI;
                    }
                }
            }
            if (fds[i].revents)
                result++;
        }
        if (result || timeout == 0)
            break;
        elapsed = GetTickCount64() - start;
        if (timeout >= 0 && elapsed >= (ULONGLONG)timeout)
            break;
        wait_ms = timeout < 0 ? INFINITE : (DWORD)((ULONGLONG)timeout - elapsed);
        status = WaitForMultipleObjects(cancel ? 2 : 1, handles, FALSE, wait_ms);
        if (status == WAIT_TIMEOUT)
            break;
        if (status == WAIT_FAILED) {
            error = WSASYSCALLFAILURE;
            result = -1;
            break;
        }
    }
done:
    for (i = 0; registered && i < count; i++)
        if (registered[i])
            WSAEventSelect(fds[i].fd, NULL, 0);
    free(registered);
    CloseHandle(wake);
    if (result < 0)
        WSASetLastError(error);
    return result;
}
#endif
