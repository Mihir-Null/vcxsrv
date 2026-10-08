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
/*****************************************************************
 *  Stuff to create connections --- OS dependent
 *
 *      EstablishNewConnections, CreateWellKnownSockets, ResetWellKnownSockets,
 *      CloseDownConnection,
 *	OnlyListToOneClient,
 *      ListenToAllClients,
 *
 *      (WaitForSomething is in its own file)
 *
 *      In this implementation, a client socket table is not kept.
 *      Instead, what would be the index into the table is just the
 *      file descriptor of the socket.  This won't work for if the
 *      socket ids aren't small nums (0 - 2^8)
 *
 *****************************************************************/

#include <dix-config.h>

#ifdef WIN32
#include <X11/Xwinsock.h>
#ifdef _DEBUG
#define DEBUG
#endif
#endif
#include <X11/X.h>
#include <X11/Xproto.h>
#define XSERV_t
#define TRANS_SERVER
#define TRANS_REOPEN
#include <X11/Xtrans/Xtrans.h>
#include <X11/Xtrans/Xtransint.h>
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>

#include <sys/stat.h>

#ifndef WIN32
#include <sys/socket.h>

#if defined(TCPCONN)
#include <netinet/in.h>
#include <arpa/inet.h>
#ifdef CSRG_BASED
#include <sys/param.h>
#endif
#include <netinet/tcp.h>
#include <arpa/inet.h>
#endif
#include <sys/uio.h>
#endif                          /* WIN32 */

#include "dix/dix_priv.h"
#include "os/audit.h"
#include "os/auth.h"
#include "os/client_priv.h"
#include "os/osdep.h"

#include "misc.h"               /* for typedef of pointer */
#include "dixstruct_priv.h"
#include "xace.h"
#include "xserver_poll.h"

#ifdef _MSC_VER
typedef int pid_t;
#endif

#ifdef HAVE_GETPEERUCRED
#include <ucred.h>
#include <zone.h>
#else
#define zoneid_t int
#endif

#ifdef HAVE_SYSTEMD_DAEMON
#include <systemd/sd-daemon.h>
#endif

#include "probes.h"
#include "xdmcp.h"

struct ospoll   *server_poll;

Bool NewOutputPending;          /* not yet attempted to write some new output */
Bool NoListenAll;               /* Don't establish any listening sockets */

#if !defined(_MSC_VER)
static Pid_t ParentProcess;
#endif
static char dynamic_display[7]; /* display name */
static int dynamic_display_id;
Bool PartialNetwork;            /* continue even if unable to bind all addrs */
#if !defined(WIN32)
static pid_t ParentProcess;
static Bool RunFromSmartParent; /* send SIGUSR1 to parent process */
#endif

int GrabInProgress = 0;

static void
EstablishNewConnections_local(int curconn, int ready, void *data);

static void
set_poll_client(ClientPtr client);

static void
set_poll_clients(void);

static XtransConnInfo *ListenTransConns = NULL;
static int *ListenTransFds = NULL;
static int ListenTransCount;

static void ErrorConnMax(XtransConnInfo /* trans_conn */ );

static XtransConnInfo
lookup_trans_conn(int fd)
{
    if (ListenTransFds) {
        int i;

        for (i = 0; i < ListenTransCount; i++)
            if (ListenTransFds[i] == fd)
                return ListenTransConns[i];
    }

    return NULL;
}

int
TransIsListening(char *protocol)
{
  /* look for this transport in the list of listeners */
  int i;
  for (i = 0; i < ListenTransCount; i++)
    {
      if (!strcmp(protocol, ListenTransConns[i]->transptr->TransName))
        {
          return 1;
        }
    }

  return 0;
}


/*
 * If SIGUSR1 was set to SIG_IGN when the server started, assume that either
 *
 *  a- The parent process is ignoring SIGUSR1
 *
 * or
 *
 *  b- The parent process is expecting a SIGUSR1
 *     when the server is ready to accept connections
 *
 * In the first case, the signal will be harmless, in the second case,
 * the signal will be quite useful.
 */
static void
InitParentProcess(void)
{
#if !defined(WIN32)
    OsSigHandlerPtr handler;

    handler = OsSignal(SIGUSR1, SIG_IGN);
    if (handler == SIG_IGN)
        RunFromSmartParent = TRUE;
    OsSignal(SIGUSR1, handler);
    ParentProcess = getppid();
#endif
}

void
NotifyParentProcess(void)
{
#if !defined(WIN32)
    if (displayfd >= 0) {
        if (write(displayfd, display, strlen(display)) != strlen(display))
            FatalError("Cannot write display number to fd %d\n", displayfd);
        if (write(displayfd, "\n", 1) != 1)
            FatalError("Cannot write display number to fd %d\n", displayfd);
        close(displayfd);
        displayfd = -1;
    }
    if (RunFromSmartParent) {
        if (ParentProcess > 1) {
            kill(ParentProcess, SIGUSR1);
        }
    }
#ifdef HAVE_SYSTEMD_DAEMON
    /* If we have been started as a systemd service, tell systemd that
       we are ready. Otherwise sd_notify() won't do anything. */
    sd_notify(0, "READY=1");
#endif
#else
/* On windows the displayfd points to shared memory, so write the id to it */
    int *pDisplayfd=(int*)MapViewOfFile((HANDLE)displayfd, FILE_MAP_READ|FILE_MAP_WRITE, 0, 0, 0);
    if (pDisplayfd)
      *pDisplayfd=dynamic_display_id;
#endif
}

#if defined(_WIN32) && !defined(__CYGWIN__)
#include "winlocal.h"
#endif

static Bool
TryCreateSocket(int num, int *partial)
{
    char port[20];

    snprintf(port, sizeof(port), "%d", num);

    return (_XSERVTransMakeAllCOTSServerListeners(port, partial,
                                                  &ListenTransCount,
                                                  &ListenTransConns) >= 0);
}

/*****************
 * CreateWellKnownSockets
 *    At initialization, create the sockets to listen on for new clients.
 *****************/

void
CreateWellKnownSockets(void)
{
    int i;
    int partial = 0;

    /* display is initialized to "0" by main(). It is then set to the display
     * number if specified on the command line. */

#if defined(_WIN32) && !defined(__CYGWIN__)
    if (winLocalManaged) {
        if (NoListenAll || !winlocal_prepare(NULL))
            FatalError("Managed local endpoint preparation failed");
        _XSERVTransListen("winlocal");
    }
#endif
    if (NoListenAll) {
        ListenTransCount = 0;
    }
    else if ((displayfd < 0) || explicit_display) {
        if (TryCreateSocket(atoi(display), &partial) &&
            ListenTransCount >= 1)
            if (!PartialNetwork && partial)
                FatalError ("Failed to establish all listening sockets");
    }
    else { /* -displayfd and no explicit display number */
        Bool found = 0;
        for (i = 0; i < 65536 - X_TCP_PORT; i++) {
            if (TryCreateSocket(i, &partial) && ListenTransCount >= 1 && (PartialNetwork || !partial)) {
                found = 1;
                break;
            }
            else
                CloseWellKnownConnections();
        }
        if (!found)
            FatalError("Failed to find a socket to listen on");
        dynamic_display_id=i;
        snprintf(dynamic_display, sizeof(dynamic_display), "%d", i);
        display = dynamic_display;
        LogSetDisplay();
    }

#if defined(_WIN32) && !defined(__CYGWIN__)
    if (winLocalManaged && !winlocal_listener_ready())
        FatalError("Required managed local listener unavailable");
    if (winLocalManaged)
        ErrorF("Managed local endpoint: %s\n", winlocal_endpoint());
#endif
    ListenTransFds = xallocarray(ListenTransCount, sizeof (int));
    if (ListenTransFds == NULL)
        FatalError ("Failed to create listening socket array");

    for (i = ListenTransCount; i > 0; i--) {
        int fd = _XSERVTransGetConnectionNumber (ListenTransConns[i-1]);

        ListenTransFds[i-1] = fd;
        SetNotifyFd(fd, EstablishNewConnections_local, X_NOTIFY_READ, NULL);

        if (!_XSERVTransIsLocal (ListenTransConns[i-1]))
            DefineSelf (fd);
    }

    if (ListenTransCount == 0 && !NoListenAll)
        FatalError
            ("Cannot establish any listening sockets - Make sure an X server isn't already running");

#if !defined(WIN32)
    OsSignal(SIGPIPE, SIG_IGN);
    OsSignal(SIGHUP, AutoResetServer);
#endif
    OsSignal(SIGINT, GiveUp);
    OsSignal(SIGTERM, GiveUp);
    ResetHosts(display);

    InitParentProcess();

#ifdef XDMCP
    XdmcpInit();
#endif
}

#if defined(WIN32) && defined(HYPERV)
/*
 * (Re)bind the "hyperv" vsock listener with the VM id most recently set
 * through _XSERVTransSetHyperVVmId().
 *
 * Used by hw/xwin (winvsock.c) to follow the WSL2 VM id: the id changes
 * every time the WSL2 VM reboots, so the listener has to be torn down and
 * recreated with the new id. Also creates the listener if it was not
 * created at startup (e.g. WSL2 was not running yet). Must be called on
 * the main thread. Returns 0 on success, -1 otherwise.
 */
int
HyperVRebindListener(void)
{
    XtransConnInfo conn;
    char name[32];
    int i, fd;

    if (NoListenAll)
        return -1;

    /* Tear down an existing hyperv listener, if any */
    for (i = 0; i < ListenTransCount; i++) {
        if (!ListenTransConns[i] || !ListenTransConns[i]->transptr ||
            !ListenTransConns[i]->transptr->TransName ||
            strcmp(ListenTransConns[i]->transptr->TransName, "hyperv") != 0)
            continue;

        fd = _XSERVTransGetConnectionNumber(ListenTransConns[i]);
        RemoveNotifyFd(fd);
        _XSERVTransClose(ListenTransConns[i]);

        /* compact the arrays */
        ListenTransCount--;
        ListenTransConns[i] = ListenTransConns[ListenTransCount];
        ListenTransFds[i] = ListenTransFds[ListenTransCount];
        break;
    }

    /* winvsock may have marked the transport "nolisten" at startup when
     * no WSL2 VM was running; clear that again for the late bind */
    _XSERVTransListen("hyperv");

    snprintf(name, sizeof(name), "hyperv/:%s", display);
    conn = _XSERVTransOpenCOTSServer(name);
    if (conn == NULL) {
        /* silent: the caller (winvsock.c) retries every tick and logs
         * rate-limited */
        return -1;
    }
    if (_XSERVTransCreateListener(conn, display, 0) < 0) {
        _XSERVTransClose(conn);
        return -1;
    }

    fd = _XSERVTransGetConnectionNumber(conn);

    ListenTransConns = reallocarray(ListenTransConns, ListenTransCount + 1,
                                    sizeof(XtransConnInfo));
    ListenTransFds = reallocarray(ListenTransFds, ListenTransCount + 1,
                                  sizeof(int));
    if (!ListenTransConns || !ListenTransFds)
        FatalError("HyperVRebindListener: allocation failure\n");

    ListenTransConns[ListenTransCount] = conn;
    ListenTransFds[ListenTransCount] = fd;
    ListenTransCount++;

    SetNotifyFd(fd, EstablishNewConnections_local, X_NOTIFY_READ, NULL);

    return 0;
}
#endif                          /* WIN32 && HYPERV */

void
ResetWellKnownSockets(void)
{
    int i;

    ResetOsBuffers();

    for (i = 0; i < ListenTransCount; i++) {
        int status = _XSERVTransResetListener(ListenTransConns[i]);

        if (status != TRANS_RESET_NOOP) {
            if (status == TRANS_RESET_FAILURE) {
                /*
                 * ListenTransConns[i] freed by xtrans.
                 * Remove it from out list.
                 */

                RemoveNotifyFd(ListenTransFds[i]);
                ListenTransFds[i] = ListenTransFds[ListenTransCount - 1];
                ListenTransConns[i] = ListenTransConns[ListenTransCount - 1];
                ListenTransCount -= 1;
                i -= 1;
            }
            else if (status == TRANS_RESET_NEW_FD) {
                /*
                 * A new file descriptor was allocated (the old one was closed)
                 */

                int newfd = _XSERVTransGetConnectionNumber(ListenTransConns[i]);

                ListenTransFds[i] = newfd;
            }
        }
    }
    for (i = 0; i < ListenTransCount; i++)
        SetNotifyFd(ListenTransFds[i], EstablishNewConnections_local, X_NOTIFY_READ,
                    NULL);

    ResetAuthorization();
    ResetHosts(display);
    /*
     * restart XDMCP
     */
#ifdef XDMCP
    XdmcpReset();
#endif
}

void
CloseWellKnownConnections(void)
{
    int i;

    for (i = 0; i < ListenTransCount; i++) {
        if (ListenTransConns[i] != NULL) {
            _XSERVTransClose(ListenTransConns[i]);
            ListenTransConns[i] = NULL;
            if (ListenTransFds != NULL)
                RemoveNotifyFd(ListenTransFds[i]);
        }
    }
    ListenTransCount = 0;
}

XID
AuthorizationIDOfClient(ClientPtr client)
{
    if (client->osPrivate)
        return ((OsCommPtr) client->osPrivate)->auth_id;
    else
        return None;
}

/*****************************************************************
 * ClientAuthorized
 *
 *    Sent by the client at connection setup:
 *                typedef struct _xConnClientPrefix {
 *                   CARD8	byteOrder;
 *                   BYTE	pad;
 *                   CARD16	majorVersion, minorVersion;
 *                   CARD16	nbytesAuthProto;
 *                   CARD16	nbytesAuthString;
 *                 } xConnClientPrefix;
 *
 *     	It is hoped that eventually one protocol will be agreed upon.  In the
 *        mean time, a server that implements a different protocol than the
 *        client expects, or a server that only implements the host-based
 *        mechanism, will simply ignore this information.
 *
 *****************************************************************/

static void
ClientReady(int fd, int xevents, void *data)
{
    ClientPtr client = data;

    if (xevents & X_NOTIFY_ERROR) {
        CloseDownClient(client);
        return;
    }
    if (xevents & X_NOTIFY_READ)
        mark_client_ready(client);
    if (xevents & X_NOTIFY_WRITE) {
        ospoll_mute(server_poll, fd, X_NOTIFY_WRITE);
        NewOutputPending = TRUE;
    }
}

static ClientPtr
AllocNewConnection(XtransConnInfo trans_conn, int fd, CARD32 conn_time)
{
    OsCommPtr oc;
    ClientPtr client;

    oc = malloc(sizeof(OsCommRec));
    if (!oc)
        return NullClient;
    oc->trans_conn = trans_conn;
    oc->fd = fd;
    oc->input = (ConnectionInputPtr) NULL;
    oc->output = (ConnectionOutputPtr) NULL;
    oc->auth_id = None;
    oc->conn_time = conn_time;
    oc->flags = 0;
    if (!(client = NextAvailableClient((void *) oc))) {
        free(oc);
        return NullClient;
    }
    client->local = ComputeLocalClient(client);
    ospoll_add(server_poll, fd,
               ospoll_trigger_edge,
               ClientReady,
               client);
    set_poll_client(client);

#ifdef DEBUG
    ErrorF("AllocNewConnection: client index = %d, socket fd = %d, local = %d\n",
           client->index, fd, client->local);
#endif
#ifdef XSERVER_DTRACE
    XSERVER_CLIENT_CONNECT(client->index, fd);
#endif

    return client;
}

/*****************
 * EstablishNewConnections_local
 *    If anyone is waiting on listened sockets, accept them. Drop pending
 *    connections if they've stuck around for more than one minute.
 *****************/
#define TimeOutValue 60 * MILLI_PER_SECOND
static void
EstablishNewConnections_local(int curconn, int ready, void *data)
{
    int newconn;       /* fd of new client */
    CARD32 connect_time;
    int i;
    ClientPtr client;
    OsCommPtr oc;
    XtransConnInfo trans_conn, new_trans_conn;
    int status;

    connect_time = GetTimeInMillis();
    /* kill off stragglers */
    for (i = 1; i < currentMaxClients; i++) {
        if ((client = clients[i])) {
            oc = (OsCommPtr) (client->osPrivate);
            if ((oc && (oc->conn_time != 0) &&
                 (connect_time - oc->conn_time) >= TimeOutValue) ||
                (client->noClientException != Success && !client->clientGone))
                CloseDownClient(client);
        }
    }

    if ((trans_conn = lookup_trans_conn(curconn)) == NULL)
        return;

    if ((new_trans_conn = _XSERVTransAccept(trans_conn, &status)) == NULL)
        return;

    newconn = _XSERVTransGetConnectionNumber(new_trans_conn);

    _XSERVTransSetOption(new_trans_conn, TRANS_NONBLOCKING, 1);

    if (trans_conn->flags & TRANS_NOXAUTH)
        new_trans_conn->flags = new_trans_conn->flags | TRANS_NOXAUTH;

    if (!AllocNewConnection(new_trans_conn, newconn, connect_time)) {
        ErrorConnMax(new_trans_conn);
    }
    return;
}

#define NOROOM "Maximum number of clients reached"

/************
 *   ErrorConnMax
 *     Fail a connection due to lack of client or file descriptor space
 ************/

static void
ConnMaxNotify(int fd, int events, void *data)
{
    XtransConnInfo trans_conn = data;
    char order = 0;

    /* try to read the byte-order of the connection */
    (void) _XSERVTransRead(trans_conn, &order, 1);
    if (order == 'l' || order == 'B' || order == 'r' || order == 'R') {
        xConnSetupPrefix csp;
        char pad[3] = { 0, 0, 0 };
        int whichbyte = 1;
        struct iovec iov[3];

        csp.success = xFalse;
        csp.lengthReason = sizeof(NOROOM) - 1;
        csp.length = (sizeof(NOROOM) + 2) >> 2;
        csp.majorVersion = X_PROTOCOL;
        csp.minorVersion = X_PROTOCOL_REVISION;
        if (((*(char *) &whichbyte) && (order == 'B' || order == 'R')) ||
            (!(*(char *) &whichbyte) && (order == 'l' || order == 'r'))) {
            swaps(&csp.majorVersion);
            swaps(&csp.minorVersion);
            swaps(&csp.length);
        }
        iov[0].iov_len = sz_xConnSetupPrefix;
        iov[0].iov_base = (char *) &csp;
        iov[1].iov_len = csp.lengthReason;
        iov[1].iov_base = (void *) NOROOM;
        iov[2].iov_len = (4 - (csp.lengthReason & 3)) & 3;
        iov[2].iov_base = pad;
        (void) _XSERVTransWritev(trans_conn, iov, 3);
    }
    RemoveNotifyFd(trans_conn->fd);
    _XSERVTransClose(trans_conn);
}

static void
ErrorConnMax(XtransConnInfo trans_conn)
{
    if (!SetNotifyFd(trans_conn->fd, ConnMaxNotify, X_NOTIFY_READ, trans_conn))
        _XSERVTransClose(trans_conn);
}

/************
 *   CloseDownFileDescriptor:
 *     Remove this file descriptor
 ************/

void
CloseDownFileDescriptor(OsCommPtr oc)
{
    if (oc->trans_conn) {
        int connection = oc->fd;
#ifdef XDMCP
        XdmcpCloseDisplay(connection);
#endif
        ospoll_remove(server_poll, connection);
        _XSERVTransDisconnect(oc->trans_conn);
        _XSERVTransClose(oc->trans_conn);
        oc->trans_conn = NULL;
        oc->fd = -1;
    }
}

/*****************
 * CheckConnections
 *    Some connection has died, go find which one and shut it down
 *    The file descriptor has been closed, but is still in AllClients.
 *    If would truly be wonderful if select() would put the bogus
 *    file descriptors in the exception mask, but nooooo.  So we have
 *    to check each and every socket individually.
 *****************/

#ifdef WIN32
void
CheckConnections(struct pollfd *fds, int num)
{
    fd_set tmask;
    int i;
    struct timeval notime;
    int r;

    notime.tv_sec = 0;
    notime.tv_usec = 0;

    for (i=0; i<num; i++)
    {
      int curclient=fds[i].fd;
      fd_set tmask;
      FD_ZERO(&tmask);
      FD_SET(curclient, &tmask);
      do
      {
        r = select (curclient + 1, &tmask, NULL, NULL, &notime);
      } while (r == SOCKET_ERROR && (WSAGetLastError() == WSAEINTR || WSAGetLastError() == WSAEWOULDBLOCK));
      if (r < 0)
      {
        for (i = 0; i < currentMaxClients; i++) {
          ClientPtr client = clients[i];
          if (client && !client->clientGone)
          {
            OsCommPtr oc = (OsCommPtr) (client->osPrivate);
            if (oc->fd==curclient)
            {
              CloseDownClient(client);
              break;
            }
          }
        }
      }
    }
}
#endif

/*****************
 * CloseDownConnection
 *    Delete client from AllClients and free resources
 *****************/

void
CloseDownConnection(ClientPtr client)
{
    OsCommPtr oc = (OsCommPtr) client->osPrivate;

    if (FlushCallback)
        CallCallbacks(&FlushCallback, client);

#ifdef DEBUG
    ErrorF("CloseDownConnection: client index = %d, socket fd = %d\n",
           client->index, oc->fd);
#endif
    if (oc->output)
        FlushClient(client, oc, (char *) NULL, 0);
    CloseDownFileDescriptor(oc);
    FreeOsBuffers(oc);
    free(client->osPrivate);
    client->osPrivate = (void *) NULL;
    if (auditTrailLevel > 1)
        AuditF("client %d disconnected\n", client->index);
}

struct notify_fd {
    int mask;
    NotifyFdProcPtr notify;
    void *data;
};

/*****************
 * HandleNotifyFd
 *    A poll callback to be called when the registered
 *    file descriptor is ready.
 *****************/

static void
HandleNotifyFd(int fd, int xevents, void *data)
{
    struct notify_fd *n = data;
    n->notify(fd, xevents, n->data);
}

/*****************
 * SetNotifyFd
 *    Registers a callback to be invoked when the specified
 *    file descriptor becomes readable.
 *****************/

Bool
SetNotifyFd(int fd, NotifyFdProcPtr notify, int mask, void *data)
{
    struct notify_fd *n;

    n = ospoll_data(server_poll, fd);
    if (!n) {
        if (mask == 0)
            return TRUE;

        n = calloc(1, sizeof (struct notify_fd));
        if (!n)
            return FALSE;
        ospoll_add(server_poll, fd,
                   ospoll_trigger_level,
                   HandleNotifyFd,
                   n);
    }

    if (mask == 0) {
        ospoll_remove(server_poll, fd);
        free(n);
    } else {
        int listen = mask & ~n->mask;
        int mute = n->mask & ~mask;

        if (listen)
            ospoll_listen(server_poll, fd, listen);
        if (mute)
            ospoll_mute(server_poll, fd, mute);
        n->mask = mask;
        n->data = data;
        n->notify = notify;
    }

    return TRUE;
}

/*****************
 * OnlyListenToOneClient:
 *    Only accept requests from  one client.  Continue to handle new
 *    connections, but don't take any protocol requests from the new
 *    ones.  Note that if GrabInProgress is set, EstablishNewConnections
 *    needs to put new clients into SavedAllSockets and SavedAllClients.
 *    Note also that there is no timeout for this in the protocol.
 *    This routine is "undone" by ListenToAllClients()
 *****************/

int
OnlyListenToOneClient(ClientPtr client)
{
    int rc;

    rc = XaceHookServerAccess(client, DixGrabAccess);
    if (rc != Success)
        return rc;

    if (!GrabInProgress) {
        GrabInProgress = client->index;
        set_poll_clients();
    }

    return rc;
}

/****************
 * ListenToAllClients:
 *    Undoes OnlyListentToOneClient()
 ****************/

void
ListenToAllClients(void)
{
    if (GrabInProgress) {
        GrabInProgress = 0;
        set_poll_clients();
    }
}

/****************
 * IgnoreClient
 *    Removes one client from input masks.
 *    Must have corresponding call to AttendClient.
 ****************/

void
IgnoreClient(ClientPtr client)
{
    OsCommPtr oc = (OsCommPtr) client->osPrivate;

    client->ignoreCount++;
    if (client->ignoreCount > 1)
        return;

    isItTimeToYield = TRUE;
    mark_client_not_ready(client);

    oc->flags |= OS_COMM_IGNORED;
    set_poll_client(client);
}

/****************
 * AttendClient
 *    Adds one client back into the input masks.
 ****************/

void
AttendClient(ClientPtr client)
{
    OsCommPtr oc = (OsCommPtr) client->osPrivate;

    if (client->clientGone) {
        /*
         * client is gone, so any pending requests will be dropped and its
         * ignore count doesn't matter.
         */
        return;
    }

    client->ignoreCount--;
    if (client->ignoreCount)
        return;

    oc->flags &= ~OS_COMM_IGNORED;
    set_poll_client(client);
    if (listen_to_client(client))
        mark_client_ready(client);
    else {
        /* grab active, mark ready when grab goes away */
        mark_client_saved_ready(client);
    }
}

/* make client impervious to grabs; assume only executing client calls this */

void
MakeClientGrabImpervious(ClientPtr client)
{
    OsCommPtr oc = (OsCommPtr) client->osPrivate;

    oc->flags |= OS_COMM_GRAB_IMPERVIOUS;
    set_poll_client(client);

    if (ServerGrabCallback) {
        ServerGrabInfoRec grabinfo;

        grabinfo.client = client;
        grabinfo.grabstate = CLIENT_IMPERVIOUS;
        CallCallbacks(&ServerGrabCallback, &grabinfo);
    }
}

/* make client pervious to grabs; assume only executing client calls this */

void
MakeClientGrabPervious(ClientPtr client)
{
    OsCommPtr oc = (OsCommPtr) client->osPrivate;

    oc->flags &= ~OS_COMM_GRAB_IMPERVIOUS;
    set_poll_client(client);
    isItTimeToYield = TRUE;

    if (ServerGrabCallback) {
        ServerGrabInfoRec grabinfo;

        grabinfo.client = client;
        grabinfo.grabstate = CLIENT_PERVIOUS;
        CallCallbacks(&ServerGrabCallback, &grabinfo);
    }
}

/* Add a fd (from launchd or similar) to our listeners */
void
ListenOnOpenFD(int fd, int noxauth)
{
    char port[PATH_MAX];
    XtransConnInfo ciptr;
    const char *display_env = getenv("DISPLAY");

    /* First check if display_env matches a <absolute path to unix socket>[.<screen number>] scheme (eg: launchd) */
    if (display_env && display_env[0] == '/') {
        struct stat sbuf;

        strlcpy(port, display_env, sizeof(port));

        /* If the path exists, we don't have do do anything else.
         * If it doesn't, we need to check for a .<screen number> to strip off and recheck.
         */
        if (0 != stat(port, &sbuf)) {
            char *dot = strrchr(port, '.');
            if (dot) {
                *dot = '\0';

                if (0 != stat(port, &sbuf)) {
                    display_env = NULL;
                }
            } else {
                display_env = NULL;
            }
        }
    }

    if (!display_env || display_env[0] != '/') {
        /* Just some default so things don't break and die. */
        snprintf(port, sizeof(port), ":%d", atoi(display));
    }

    /* Make our XtransConnInfo
     * TRANS_SOCKET_LOCAL_INDEX = 5 from Xtrans.c
     */
    ciptr = _XSERVTransReopenCOTSServer(5, fd, port);
    if (ciptr == NULL) {
        ErrorF("Got NULL while trying to Reopen listen port.\n");
        return;
    }

    if (noxauth)
        ciptr->flags = ciptr->flags | TRANS_NOXAUTH;

    /* Allocate space to store it */
    ListenTransFds =
        XNFreallocarray(ListenTransFds, ListenTransCount + 1, sizeof(int));
    ListenTransConns =
        XNFreallocarray(ListenTransConns, ListenTransCount + 1,
                        sizeof(XtransConnInfo));

    /* Store it */
    ListenTransConns[ListenTransCount] = ciptr;
    ListenTransFds[ListenTransCount] = fd;

    SetNotifyFd(fd, EstablishNewConnections_local, X_NOTIFY_READ, NULL);

    /* Increment the count */
    ListenTransCount++;
}

/* based on TRANS(SocketUNIXAccept) (XtransConnInfo ciptr, int *status) */
Bool
AddClientOnOpenFD(int fd)
{
    XtransConnInfo ciptr;
    CARD32 connect_time;
    char port[20];

    snprintf(port, sizeof(port), ":%d", atoi(display));
    ciptr = _XSERVTransReopenCOTSServer(5, fd, port);
    if (ciptr == NULL)
        return FALSE;

    _XSERVTransSetOption(ciptr, TRANS_NONBLOCKING, 1);
    ciptr->flags |= TRANS_NOXAUTH;

    connect_time = GetTimeInMillis();

    if (!AllocNewConnection(ciptr, fd, connect_time)) {
        ErrorConnMax(ciptr);
        return FALSE;
    }

    return TRUE;
}

Bool
listen_to_client(ClientPtr client)
{
    OsCommPtr oc = (OsCommPtr) client->osPrivate;

    if (oc->flags & OS_COMM_IGNORED)
        return FALSE;

    if (!GrabInProgress)
        return TRUE;

    if (client->index == GrabInProgress)
        return TRUE;

    if (oc->flags & OS_COMM_GRAB_IMPERVIOUS)
        return TRUE;

    return FALSE;
}

static void
set_poll_client(ClientPtr client)
{
    OsCommPtr oc = (OsCommPtr) client->osPrivate;

    if (oc->trans_conn) {
        if (listen_to_client(client))
            ospoll_listen(server_poll, oc->trans_conn->fd, X_NOTIFY_READ);
        else
            ospoll_mute(server_poll, oc->trans_conn->fd, X_NOTIFY_READ);
    }
}

static void
set_poll_clients(void)
{
    int i;

    for (i = 1; i < currentMaxClients; i++) {
        ClientPtr client = clients[i];
        if (client && !client->clientGone)
            set_poll_client(client);
    }
}
