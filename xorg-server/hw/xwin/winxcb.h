/* Copyright (c) 2026. SPDX-License-Identifier: MIT */
#ifndef VCX_WINXCB_H
#define VCX_WINXCB_H
#include <xcb/xcb.h>
/* Worker threads only: waits for X11 setup. NULL on failure.
 * Caller owns successful connections and must xcb_disconnect them. */
xcb_connection_t* winLocalXcbConnect(unsigned int screen, xcb_auth_info_t* auth);
#endif
