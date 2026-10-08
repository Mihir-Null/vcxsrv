#if defined(_WIN32) && !defined(__CYGWIN__)
/* Copyright (c) 2026. SPDX-License-Identifier: MIT */
#include "winxcb.h"
#include "../../os/winlocal.h"

xcb_connection_t* winLocalXcbConnect(unsigned int screen, xcb_auth_info_t* auth)
{
    WSADATA data;
    int fd, error;
    xcb_connection_t* conn;
    if (!auth || !auth->name || auth->namelen <= 0 || !auth->data || auth->datalen <= 0)
        return NULL;
    /* xcb_connect_to_fd does not acquire Winsock, but xcb_disconnect releases
     * it. Transfer exactly one reference together with the connected socket. */
    if (WSAStartup(MAKEWORD(2, 2), &data) != 0)
        return NULL;
    fd = winlocal_connect();
    if (fd < 0) {
        WSACleanup();
        return NULL;
    }
    conn = xcb_connect_to_fd(fd, auth);
    error = xcb_connection_has_error(conn);
    if (error) {
        /* Bundled XCB closes fd in all failure paths. Only its allocation
         * failure happens before xcb_disconnect can release Winsock. */
        if (error == XCB_CONN_CLOSED_MEM_INSUFFICIENT)
            WSACleanup();
        xcb_disconnect(conn);
        return NULL;
    }
    if (screen >= (unsigned int)xcb_setup_roots_length(xcb_get_setup(conn))) {
        xcb_disconnect(conn);
        return NULL;
    }
    return conn;
}

#endif
