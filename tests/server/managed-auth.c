/* Exercise the actual ClientAuthorized implementation; only its dependencies
 * are controlled, to detect transport/host bypasses and cookie success. */
#include <dix-config.h>
#include <X11/Xwinsock.h>
#include <hvsocket.h>
#ifndef AF_HYPERV
#define AF_HYPERV 34
#endif
#include <X11/X.h>
#include <X11/Xproto.h>
#define XSERV_t
#define TRANS_SERVER
#include <X11/Xtrans/Xtrans.h>
#include <X11/Xtrans/Xtransint.h>
#include "dixstruct_priv.h"
#include "os/osdep.h"
#include "os/auth.h"
#include "os/audit.h"
#include <stdio.h>
#include <stdlib.h>

extern const char *ClientAuthorized(ClientPtr, unsigned int, char *, unsigned int, char *);
int winLocalManaged;
int auditTrailLevel;
static XID result;
static int checks, hostChecks;
static int peerFamily;

XID CheckAuthorization(unsigned int n, const char *p, unsigned int d, const char *s,
                       ClientPtr client, const char **reason)
{ (void)n;(void)p;(void)d;(void)s;(void)client;checks++;*reason="Bad cookie";return result; }
Bool InvalidHost(struct sockaddr *p, int n, ClientPtr client)
{ (void)p;(void)n;(void)client;hostChecks++;return FALSE; }
int _XSERVTransGetPeerAddr(XtransConnInfo conn,int *family,int *length,Xtransaddr **address)
{ (void)conn;*family=peerFamily;*length=sizeof(struct sockaddr);*address=calloc(1,*length);
  ((struct sockaddr *)*address)->sa_family=(ADDRESS_FAMILY)peerFamily;return 0; }
int GetLocalClientCreds(ClientPtr client, LocalClientCredRec **creds)
{ (void)client;(void)creds;return -1; }
void FreeLocalClientCreds(LocalClientCredRec *creds) { (void)creds; }
void AuditF(const char *format, ...) { (void)format; }
size_t strlcpy(char *dest,const char *source,size_t size)
{ size_t length=strlen(source);if(size){size_t n=length<size-1?length:size-1;memcpy(dest,source,n);dest[n]=0;}return length; }
void XdmcpOpenDisplay(int fd) { (void)fd; }
int XaceHookAuthAvail(ClientPtr client,XID id) { (void)client;(void)id;return 0; }

#define CHECK(x) do { if (!(x)) {fprintf(stderr,"FAIL line %d: %s\n",__LINE__,#x);exit(1);} } while(0)
int main(void)
{
    ClientRec client={0};
    OsCommRec priv={0};
    struct _XtransConnInfo conn={0};
    int family;
    client.osPrivate=&priv; priv.trans_conn=&conn;
    winLocalManaged=1; result=(XID)~0L;
    for(family=0;family<3;family++) {
        peerFamily=family==0?AF_HYPERV:family==1?AF_UNIX:AF_INET;
        checks=hostChecks=0; conn.flags=0;
        CHECK(ClientAuthorized(&client,0,NULL,0,NULL)!=NULL);
        CHECK(checks==1); CHECK(hostChecks==0);
        checks=hostChecks=0;conn.flags=TRANS_NOXAUTH;
        CHECK(ClientAuthorized(&client,0,NULL,0,NULL)!=NULL);
        CHECK(checks==1); CHECK(hostChecks==0);
    }
    result=42;checks=0;CHECK(ClientAuthorized(&client,18,"MIT-MAGIC-COOKIE-1",16,"test fixture") ==NULL);
    CHECK(checks==1); CHECK(priv.auth_id==42);
    winLocalManaged=0; result=(XID)~0L;conn.flags=0;hostChecks=0;
    CHECK(ClientAuthorized(&client,0,NULL,0,NULL)==NULL); CHECK(hostChecks==1);
    conn.flags=TRANS_NOXAUTH;checks=0;CHECK(ClientAuthorized(&client,0,NULL,0,NULL)==NULL);CHECK(checks==0);
    puts("PASS actual ClientAuthorized: strict managed AF_HYPERV/AF_UNIX/TCP, NOXAUTH/host bypass, valid auth, generic behavior");
    return 0;
}
