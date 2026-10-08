#include "winadmission.h"
/* All property mutation and token allocation happen on the HWND owner thread.
 * Tokens are process-unique, including across HWND and XID reuse. */
static const char tokenProp[] = "VcXsrvAdmissionToken";
static const char xidProp[] = "VcXsrvAdmissionXid";
static const char pendingProp[] = "VcXsrvAdmissionPending";
static const char showProp[] = "VcXsrvAdmissionShow";
static ULONG_PTR nextToken;
ULONG_PTR winAdmissionBegin(HWND hwnd, ULONG_PTR xid)
{
    ULONG_PTR token;
    if (nextToken == ~(ULONG_PTR)0) return 0;
    token = ++nextToken;
    if (!SetPropA(hwnd, xidProp, (HANDLE)xid) ||
        !SetPropA(hwnd, tokenProp, (HANDLE)token) ||
        !SetPropA(hwnd, pendingProp, (HANDLE)1) ||
        !SetPropA(hwnd, showProp, (HANDLE)(SW_SHOWNOACTIVATE + 1))) {
        winAdmissionCancel(hwnd);
        return 0;
    }
    return token;
}
void winAdmissionCancel(HWND hwnd)
{
    RemovePropA(hwnd, tokenProp);
    RemovePropA(hwnd, xidProp);
    RemovePropA(hwnd, pendingProp);
    RemovePropA(hwnd, showProp);
}
ULONG_PTR winAdmissionQuery(HWND hwnd, ULONG_PTR xid)
{
    if ((ULONG_PTR)GetPropA(hwnd, xidProp) != xid) return 0;
    return (ULONG_PTR)GetPropA(hwnd, tokenProp);
}
BOOL winAdmissionPending(HWND hwnd) { return GetPropA(hwnd, pendingProp) != NULL; }
BOOL winAdmissionApply(const winAdmissionTicket *ticket, winAdmissionMutation mutate, void *data)
{
    winAdmissionRequest request;
    request.ticket = *ticket; request.mutate = mutate; request.data = data;
    /* Synchronous delivery owns the stack payload until the callback returns.
     * The callback must never perform XCB requests or wait for the WM worker. */
    return (BOOL)SendMessage(ticket->hwnd, WM_WIN_ADMISSION_APPLY, 0, (LPARAM)&request);
}
BOOL winAdmissionDispatch(HWND hwnd, const winAdmissionRequest *request, BOOL live)
{
    if (!live || !request || hwnd != request->ticket.hwnd || !request->ticket.token ||
        winAdmissionQuery(hwnd, request->ticket.xid) != request->ticket.token) return FALSE;
    request->mutate(hwnd, request->data);
    return TRUE;
}
void winAdmissionSetShowCommand(HWND hwnd, int command)
{
    if (winAdmissionPending(hwnd)) {
        /* Store command+1 so SW_HIDE is also representable. */
        SetPropA(hwnd, showProp, (HANDLE)(INT_PTR)(command + 1));
    } else ShowWindow(hwnd, command);
}
BOOL winAdmissionComplete(HWND hwnd, ULONG_PTR xid, ULONG_PTR token, BOOL live)
{
    int command;
    if (!live || !token || winAdmissionQuery(hwnd, xid) != token || !winAdmissionPending(hwnd)) return FALSE;
    command = (int)(INT_PTR)GetPropA(hwnd, showProp) - 1;
    /* Consume before ShowWindow: USER calls the window procedure synchronously. */
    RemovePropA(hwnd, pendingProp);
    RemovePropA(hwnd, showProp);
    ShowWindow(hwnd, command);
    UpdateWindow(hwnd);
    return TRUE;
}
void winPrepareNativeWindow(HWND hwnd)
{
    if (GetParent(hwnd))
        SetWindowLongPtr(hwnd, GWL_STYLE, WS_POPUP | WS_OVERLAPPED | WS_SYSMENU | WS_CLIPCHILDREN | WS_CLIPSIBLINGS);
    else
        SetWindowLongPtr(hwnd, GWL_STYLE, (WS_POPUP | WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_CLIPSIBLINGS) & ~WS_CAPTION & ~WS_SIZEBOX);
}
