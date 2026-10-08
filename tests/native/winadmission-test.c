#include <assert.h>
#include <stdio.h>
#include "../../xorg-server/hw/xwin/winadmission.h"
static BOOL live = TRUE;
static int shows, wrongShows;
static BOOL expectTool;
static winAdmissionTicket ticket;
static HWND observed;
static int realShows, badRealShows;
static void CALLBACK hook(HWINEVENTHOOK hook, DWORD event, HWND hwnd, LONG object,
                          LONG child, DWORD thread, DWORD time) {
    wchar_t name[64];
    (void)hook; (void)event; (void)thread; (void)time;
    if (hwnd != observed || object != OBJID_WINDOW || child != CHILDID_SELF) return;
    realShows++;
    GetWindowTextW(hwnd,name,64);
    if (!IsWindowVisible(hwnd) || winAdmissionPending(hwnd) || wcscmp(name,L"ready") ||
        (GetWindowLongPtr(hwnd,GWL_EXSTYLE)&WS_EX_TOOLWINDOW)) badRealShows++;
}
static void pump(void) {
    MSG message;
    while (PeekMessage(&message,NULL,0,0,PM_REMOVE)) { TranslateMessage(&message); DispatchMessage(&message); }
}

static LRESULT CALLBACK proc(HWND h, UINT m, WPARAM w, LPARAM l) {
    if(m == WM_WIN_ADMISSION_QUERY) return live ? winAdmissionQuery(h,w) : 0;
    if(m == WM_WIN_ADMISSION_APPLY) return winAdmissionDispatch(h,(void*)l,live);
    if(m == WM_WIN_ADMISSION_READY) return winAdmissionComplete(h,w,l,live);
    if(m == WM_SHOWWINDOW && w) {
        wchar_t name[64]; GetWindowTextW(h,name,64);
        shows++;
        if(winAdmissionPending(h) || wcscmp(name,L"ready") ||
           (!!(GetWindowLongPtr(h,GWL_EXSTYLE)&WS_EX_TOOLWINDOW) != expectTool)) wrongShows++;
        /* Reentrancy must see the completion already consumed. */
        assert(!winAdmissionComplete(h,ticket.xid,ticket.token,TRUE));
    }
    return DefWindowProcW(h,m,w,l);
}
static void finalHints(HWND h, void *data) {
    SetWindowTextW(h,L"ready");
    SetWindowLongPtr(h,GWL_EXSTYLE,WS_EX_APPWINDOW);
    winAdmissionSetShowCommand(h,(int)(INT_PTR)data);
}
static void toolHints(HWND h, void *data) {
    (void)data;
    SetWindowTextW(h,L"ready");
    SetWindowLongPtr(h,GWL_EXSTYLE,WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE);
    winAdmissionSetShowCommand(h,SW_SHOWNOACTIVATE);
}
static DWORD WINAPI worker(void *data) {
    winAdmissionTicket copy = *(winAdmissionTicket *)data;
    winAdmissionApply(&copy,finalHints,(void*)SW_SHOWNOACTIVATE);
    assert(PostMessage(copy.hwnd,WM_WIN_ADMISSION_READY,copy.xid,copy.token));
    return 0;
}
static void waitWorker(HANDLE thread) {
    while (MsgWaitForMultipleObjects(1,&thread,FALSE,5000,QS_ALLINPUT) == WAIT_OBJECT_0+1) pump();
    assert(WaitForSingleObject(thread,0)==WAIT_OBJECT_0);
    pump(); CloseHandle(thread);
}
static HWND make(void) {
    return CreateWindowExW(WS_EX_TOOLWINDOW,L"admission-test",L"pending",WS_OVERLAPPEDWINDOW,40,40,200,120,NULL,NULL,GetModuleHandle(NULL),NULL);
}
int main(void) {
    WNDCLASSW wc={0}; HWND h; ULONG_PTR old; int i; HWINEVENTHOOK eventHook;
    wc.lpfnWndProc=proc; wc.hInstance=GetModuleHandle(NULL); wc.lpszClassName=L"admission-test";
    assert(RegisterClassW(&wc));
    h=make(); assert(h); observed=h;
    eventHook=SetWinEventHook(EVENT_OBJECT_SHOW,EVENT_OBJECT_SHOW,NULL,hook,
                             GetCurrentProcessId(),0,WINEVENT_OUTOFCONTEXT);
    assert(eventHook);
    ticket.hwnd=h; ticket.xid=42; ticket.token=winAdmissionBegin(h,42); assert(ticket.token);
    winPrepareNativeWindow(h);
    assert(!IsWindowVisible(h));
    assert(winAdmissionApply(&ticket,finalHints,(void*)SW_SHOWNOACTIVATE));
    assert(!IsWindowVisible(h)); /* UpdateState must not bypass the gate. */
    assert(!winAdmissionComplete(h,43,ticket.token,TRUE));
    assert(!winAdmissionComplete(h,42,ticket.token,FALSE));
    assert(winAdmissionComplete(h,42,ticket.token,TRUE));
    assert(IsWindowVisible(h)); assert(shows==1 && !wrongShows);
    pump(); assert(realShows==1 && !badRealShows);
    UnhookWinEvent(eventHook);
    assert(!winAdmissionComplete(h,42,ticket.token,TRUE));
    for(i=0;i<50;i++) {
        ShowWindow(h,SW_HIDE); old=ticket.token; winAdmissionCancel(h);
        ticket.token=winAdmissionBegin(h,42); assert(ticket.token!=old);
        assert(!winAdmissionComplete(h,42,old,TRUE));
        assert(winAdmissionApply(&ticket,finalHints,(void*)SW_SHOWMINNOACTIVE));
        assert(!IsWindowVisible(h));
        assert(winAdmissionComplete(h,42,ticket.token,TRUE)); assert(IsIconic(h));
    }
    assert(shows >= 1 && !wrongShows);
    winAdmissionCancel(h); assert(!winAdmissionApply(&ticket,finalHints,0)); DestroyWindow(h);
    /* A recycled native handle and even the same XID cannot accept the old token. */
    h=make(); old=ticket.token; ticket.hwnd=h; ticket.token=winAdmissionBegin(h,42);
    assert(!winAdmissionComplete(h,42,old,TRUE));
    /* Deliver a genuinely cross-thread native mutation and queued completion. */
    waitWorker(CreateThread(NULL,0,worker,&ticket,0,NULL));
    assert(IsWindowVisible(h)); assert(!wrongShows);
    ShowWindow(h,SW_HIDE);
    old=ticket.token; assert(PostMessage(h,WM_WIN_ADMISSION_READY,42,old));
    winAdmissionCancel(h); ticket.token=winAdmissionBegin(h,42);
    pump(); assert(!IsWindowVisible(h));
    live=FALSE; assert(!winAdmissionApply(&ticket,finalHints,0)); assert(!IsWindowVisible(h));
    winAdmissionCancel(h); DestroyWindow(h);
    live=TRUE;
    /* Owned dialogs retain owner and transient baseline style. */
    {
        HWND owner=make();
        h=CreateWindowExW(WS_EX_TOOLWINDOW,L"admission-test",L"pending",WS_POPUP,40,40,120,90,owner,NULL,GetModuleHandle(NULL),NULL);
        assert(h); ticket.hwnd=h; ticket.xid=99; ticket.token=winAdmissionBegin(h,99);
        winPrepareNativeWindow(h); assert(GetWindow(h,GW_OWNER)==owner);
        assert(GetWindowLongPtr(h,GWL_STYLE)&WS_SYSMENU);
        expectTool=TRUE;
        assert(winAdmissionApply(&ticket,toolHints,NULL));
        assert(!IsWindowVisible(h));
        assert(winAdmissionComplete(h,99,ticket.token,TRUE));
        assert(GetWindow(h,GW_OWNER)==owner);
        assert(GetWindowLongPtr(h,GWL_EXSTYLE)&WS_EX_NOACTIVATE);
        assert(!wrongShows);
        winAdmissionCancel(h); DestroyWindow(h); DestroyWindow(owner); expectTool=FALSE;
    }
    h=make(); ticket.hwnd=h; ticket.xid=101; ticket.token=winAdmissionBegin(h,101);
    assert(winAdmissionApply(&ticket,finalHints,(void*)SW_SHOWMAXIMIZED));
    assert(!IsWindowVisible(h)); assert(winAdmissionComplete(h,101,ticket.token,TRUE));
    assert(IsZoomed(h)); assert(!wrongShows); winAdmissionCancel(h); DestroyWindow(h);
    puts("PASS real native SHOW and owner admission: hidden state, consumed completion, stale XID/token, remap, minimize/maximize, owned/tool/noactivate, destruction, cross-thread completion");
    return 0;
}
