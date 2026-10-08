/***********************************************************

Copyright 1987, 1989, 1998  The Open Group

Permission to use, copy, modify, distribute, and sell this software and its
documentation for any purpose is hereby granted without fee, provided that
the above copyright notice appear in all copies and that both that
copyright notice and this permission notice appear in supporting
documentation.

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.  IN NO EVENT SHALL THE
OPEN GROUP BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN
AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

Except as contained in this notice, the name of The Open Group shall not be
used in advertising or otherwise to promote the sale, use or other dealings
in this Software without prior written authorization from The Open Group.

Copyright 1987, 1989 by Digital Equipment Corporation, Maynard, Massachusetts.

                        All Rights Reserved

Permission to use, copy, modify, and distribute this software and its
documentation for any purpose and without fee is hereby granted,
provided that the above copyright notice appear in all copies and that
both that copyright notice and this permission notice appear in
supporting documentation, and that the name of Digital not be
used in advertising or publicity pertaining to distribution of the
software without specific, written prior permission.

DIGITAL DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE, INCLUDING
ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS, IN NO EVENT SHALL
DIGITAL BE LIABLE FOR ANY SPECIAL, INDIRECT OR CONSEQUENTIAL DAMAGES OR
ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS
SOFTWARE.

******************************************************************/
/* Client setup authorization. Kept separate from listener lifecycle so native
 * tests can exercise the production gate without the whole server dispatcher. */
#include <dix-config.h>
#ifdef WIN32
#include <X11/Xwinsock.h>
#endif
#include <X11/X.h>
#include <X11/Xproto.h>
#define XSERV_t
#define TRANS_SERVER
#include <X11/Xtrans/Xtrans.h>
#include <X11/Xtrans/Xtransint.h>
#ifndef WIN32
#include <sys/socket.h>
#ifdef TCPCONN
#include <netinet/in.h>
#include <arpa/inet.h>
#endif
#endif
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _MSC_VER
typedef int pid_t;
#endif
#ifdef HAVE_GETPEERUCRED
#include <ucred.h>
#include <zone.h>
#else
#define zoneid_t int
#endif
#include "dixstruct_priv.h"
#include "os/audit.h"
#include "os/auth.h"
#include "os/osdep.h"
#include "xace.h"
#include "probes.h"
#include "xdmcp.h"
#if defined(_WIN32) && !defined(__CYGWIN__)
#include "winlocal.h"
#endif

static void
AuthAudit(ClientPtr client, Bool letin,
          struct sockaddr *saddr, int len,
          unsigned int proto_n, char *auth_proto, int auth_id)
{
    char addr[128];
    char client_uid_string[64];
    LocalClientCredRec *lcc;

#ifdef XSERVER_DTRACE
    pid_t client_pid = -1;
    zoneid_t client_zid = -1;
#endif

    if (!len)
        strlcpy(addr, "local host", sizeof(addr));
    else
        switch (saddr->sa_family) {
        case AF_UNSPEC:
#if defined(UNIXCONN) || defined(LOCALCONN) || (defined(_WIN32) && !defined(__CYGWIN__))
        case AF_UNIX:
#endif
            strlcpy(addr, "local host", sizeof(addr));
            break;
#if defined(TCPCONN)
        case AF_INET:{
#if defined(HAVE_INET_NTOP)
            char ipaddr[INET_ADDRSTRLEN];

            inet_ntop(AF_INET, &((struct sockaddr_in *) saddr)->sin_addr,
                      ipaddr, sizeof(ipaddr));
#else
            const char *ipaddr =
                inet_ntoa(((struct sockaddr_in *) saddr)->sin_addr);
#endif
            snprintf(addr, sizeof(addr), "IP %s", ipaddr);
        }
            break;
#if defined(IPv6)
        case AF_INET6:{
            char ipaddr[INET6_ADDRSTRLEN];

            inet_ntop(AF_INET6, &((struct sockaddr_in6 *) saddr)->sin6_addr,
                      ipaddr, sizeof(ipaddr));
            snprintf(addr, sizeof(addr), "IP %s", ipaddr);
        }
            break;
#endif
#endif
        default:
            strlcpy(addr, "unknown address", sizeof(addr));
        }

    if (GetLocalClientCreds(client, &lcc) != -1) {
        int slen;               /* length written to client_uid_string */

        strcpy(client_uid_string, " ( ");
        slen = 3;

        if (lcc->fieldsSet & LCC_UID_SET) {
            snprintf(client_uid_string + slen,
                     sizeof(client_uid_string) - slen,
                     "uid=%ld ", (long) lcc->euid);
            slen = strlen(client_uid_string);
        }

        if (lcc->fieldsSet & LCC_GID_SET) {
            snprintf(client_uid_string + slen,
                     sizeof(client_uid_string) - slen,
                     "gid=%ld ", (long) lcc->egid);
            slen = strlen(client_uid_string);
        }

        if (lcc->fieldsSet & LCC_PID_SET) {
#ifdef XSERVER_DTRACE
            client_pid = lcc->pid;
#endif
            snprintf(client_uid_string + slen,
                     sizeof(client_uid_string) - slen,
                     "pid=%ld ", (long) lcc->pid);
            slen = strlen(client_uid_string);
        }

        if (lcc->fieldsSet & LCC_ZID_SET) {
#ifdef XSERVER_DTRACE
            client_zid = lcc->zoneid;
#endif
            snprintf(client_uid_string + slen,
                     sizeof(client_uid_string) - slen,
                     "zoneid=%ld ", (long) lcc->zoneid);
            slen = strlen(client_uid_string);
        }

        snprintf(client_uid_string + slen, sizeof(client_uid_string) - slen,
                 ")");
        FreeLocalClientCreds(lcc);
    }
    else {
        client_uid_string[0] = '\0';
    }

#ifdef XSERVER_DTRACE
    XSERVER_CLIENT_AUTH(client->index, addr, client_pid, client_zid);
#endif
    if (auditTrailLevel > 1) {
        if (proto_n)
            AuditF("client %d %s from %s%s\n  Auth name: %.*s ID: %d\n",
                   client->index, letin ? "connected" : "rejected", addr,
                   client_uid_string, (int) proto_n, auth_proto, auth_id);
        else
            AuditF("client %d %s from %s%s\n",
                   client->index, letin ? "connected" : "rejected", addr,
                   client_uid_string);

    }
}

const char *
ClientAuthorized(ClientPtr client,
                 unsigned int proto_n, char *auth_proto,
                 unsigned int string_n, char *auth_string)
{
    OsCommPtr priv;
    Xtransaddr *from = NULL;
    int family;
    int fromlen;
    XID auth_id;
    const char *reason = NULL;
    XtransConnInfo trans_conn;
    Bool requireAuth = FALSE;
#if defined(_WIN32) && !defined(__CYGWIN__)
    requireAuth = winLocalManaged;
#endif

    priv = (OsCommPtr) client->osPrivate;
    trans_conn = priv->trans_conn;

    /* Allow any client to connect without authorization on a launchd socket,
       because it is securely created -- this prevents a race condition on launch */
    if (!requireAuth && (trans_conn->flags & TRANS_NOXAUTH)) {
        auth_id = (XID) 0L;
    }
    else {
        auth_id =
            CheckAuthorization(proto_n, auth_proto, string_n, auth_string,
                               client, &reason);
    }

    if (auth_id == (XID) ~0L) {
        if (_XSERVTransGetPeerAddr(trans_conn, &family, &fromlen, &from) != -1) {
            /* Cookie authentication is mandatory on every managed transport,
             * including native local sockets and the exact Hyper-V VM. */
            if (requireAuth || InvalidHost((struct sockaddr *) from, fromlen, client))
                AuthAudit(client, FALSE, (struct sockaddr *) from,
                          fromlen, proto_n, auth_proto, auth_id);
            else {
                auth_id = (XID) 0;
#ifdef XSERVER_DTRACE
                if ((auditTrailLevel > 1) || XSERVER_CLIENT_AUTH_ENABLED())
#else
                if (auditTrailLevel > 1)
#endif
                    AuthAudit(client, TRUE,
                              (struct sockaddr *) from, fromlen,
                              proto_n, auth_proto, auth_id);
            }

            free(from);
        }

        if (auth_id == (XID) ~0L) {
            if (reason)
                return reason;
            else
                return "Client is not authorized to connect to Server";
        }
    }
#ifdef XSERVER_DTRACE
    else if ((auditTrailLevel > 1) || XSERVER_CLIENT_AUTH_ENABLED())
#else
    else if (auditTrailLevel > 1)
#endif
    {
        if (_XSERVTransGetPeerAddr(trans_conn, &family, &fromlen, &from) != -1) {
            AuthAudit(client, TRUE, (struct sockaddr *) from, fromlen,
                      proto_n, auth_proto, auth_id);

            free(from);
        }
    }
    priv->auth_id = auth_id;
    priv->conn_time = 0;

#ifdef XDMCP
    /* indicate to Xdmcp protocol that we've opened new client */
    XdmcpOpenDisplay(priv->fd);
#endif                          /* XDMCP */

    XaceHookAuthAvail(client, auth_id);

    /* At this point, if the client is authorized to change the access control
     * list, we should getpeername() information, and add the client to
     * the selfhosts list.  It's not really the host machine, but the
     * true purpose of the selfhosts list is to see who may change the
     * access control list.
     */
    return ((char *) NULL);
}
