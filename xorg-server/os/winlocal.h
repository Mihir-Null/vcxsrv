#ifndef VCX_WINLOCAL_H
#define VCX_WINLOCAL_H
/* Native Windows 11 x64 only. No winsock types escape this interface. */
#include <stddef.h>
#include <stdint.h>
int winlocal_socket_to_fd(uintptr_t socket_value);
/* Dispatch-thread preparation before publishing to workers. NULL selects the
 * current user's temp directory. Success retains an immutable UTF-8 endpoint
 * across server resets. No listening socket is created by prepare. */
int winlocal_prepare(const wchar_t* base_directory);
/* Owner-thread metadata only; worker connections take a protected snapshot. */
const char* winlocal_endpoint(void);
const wchar_t* winlocal_directory(void);
int winlocal_listen(void);
/* Raw socket, caller already holds Winsock and owns close. XCB callers must
 * instead use winLocalXcbConnect to transfer the Winsock reference correctly. */
int winlocal_connect(void);
int winlocal_listener_ready(void);
void winlocal_close_listener(int fd);
/* Dispatch thread, only after internal clients stop/join. Idempotent. Normal
 * xtrans listener close removes just the socket file; cleanup removes directory. */
void winlocal_cleanup(void);
/* Default zero. Task2b alone sets this after complete profile validation.
 * CreateWellKnownSockets then prepares/enables/requires winlocal; InitOutput
 * requires successful auth before the server-started mutex releases workers. */
extern int winLocalManaged;
#endif
