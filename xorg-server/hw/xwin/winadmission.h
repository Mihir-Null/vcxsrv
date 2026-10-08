/* Native owner-thread admission. No X server pointers cross the WM queue. */
#ifndef WINADMISSION_H
#define WINADMISSION_H
#include <windows.h>
#define WM_WIN_ADMISSION_QUERY (WM_APP + 0x310)
#define WM_WIN_ADMISSION_APPLY (WM_APP + 0x311)
#define WM_WIN_ADMISSION_READY (WM_APP + 0x312)
typedef struct { HWND hwnd; ULONG_PTR xid, token; } winAdmissionTicket;
typedef void (*winAdmissionMutation)(HWND, void *);
typedef struct {
    winAdmissionTicket ticket;
    winAdmissionMutation mutate;
    void *data;
} winAdmissionRequest;
ULONG_PTR winAdmissionBegin(HWND hwnd, ULONG_PTR xid);
void winAdmissionCancel(HWND hwnd);
ULONG_PTR winAdmissionQuery(HWND hwnd, ULONG_PTR xid);
BOOL winAdmissionPending(HWND hwnd);
BOOL winAdmissionApply(const winAdmissionTicket *ticket, winAdmissionMutation mutate, void *data);
BOOL winAdmissionDispatch(HWND hwnd, const winAdmissionRequest *request, BOOL live);
BOOL winAdmissionComplete(HWND hwnd, ULONG_PTR xid, ULONG_PTR token, BOOL live);
void winAdmissionSetShowCommand(HWND hwnd, int command);
void winPrepareNativeWindow(HWND hwnd);
#endif
