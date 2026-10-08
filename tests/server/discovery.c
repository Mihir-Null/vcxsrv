#include "../../xorg-server/hw/xwin/winwsldiscovery.h"
#include <shellapi.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>
#define CHECK(x) do { if (!(x)) { fprintf(stderr,"FAIL line %d: %s\n",__LINE__,#x); exit(1); } } while (0)
static const char vm1[]="12345678-1234-5678-9012-123456789abc";
static const char vm2[]="abcdef01-1234-5678-9012-123456789abc";
static const wchar_t selected[]=L"Work \x65e5\x672c";
static void quoteHostOptions(void)
{
    const wchar_t *args[]={L"C:\\Windows\\system32\\wsl.exe",L"--list",L"--running",L"--quiet",NULL};
    wchar_t line[256];
    CHECK(winWslQuoteArgs(args,line,256));
    CHECK(!wcscmp(line,L"C:\\Windows\\system32\\wsl.exe --list --running --quiet"));
}
static void parser(void)
{
    const wchar_t quiet[]=L"NixOS\r\nWork \x65e5\x672c\r\n";
    const wchar_t bom[]=L"\xfeffNixOS\r\n";
    const unsigned char bad[]={0x00,0xd8,0x41,0x00};
    wchar_t out[100]; char guid[37]="unchanged";
    CHECK(winWslDecode((const unsigned char *)quiet,sizeof(quiet)-2,out,100)==WIN_WSL_SUCCESS);
    CHECK(!wcscmp(out,quiet));
    CHECK(winWslDecode((const unsigned char *)bom,sizeof(bom)-2,out,100)==WIN_WSL_SUCCESS);
    CHECK(!wcscmp(out,L"NixOS\r\n"));
    CHECK(winWslDecode((const unsigned char *)quiet,sizeof(quiet)-3,out,100)==WIN_WSL_UNSUPPORTED);
    CHECK(winWslDecode(bad,sizeof(bad),out,100)==WIN_WSL_UNSUPPORTED);
    CHECK(winWslDecode((const unsigned char *)quiet,sizeof(quiet)-2,out,3)==WIN_WSL_UNSUPPORTED);
    CHECK(winWslValidName(selected)); CHECK(!winWslValidName(L""));
    CHECK(!winWslValidName(L"name\nother")); CHECK(!winWslValidName(L"name\x0085")); CHECK(!winWslValidName(L"\xd800"));
    CHECK(winWslParseGuid((const unsigned char *)"12345678-1234-5678-9012-123456789abc\r\n",38,guid));
    CHECK(!strcmp(guid,vm1));
    CHECK(!winWslParseGuid((const unsigned char *)"12345678-1234-5678-9012-123456789abz",36,guid));
    CHECK(!strcmp(guid,vm1));
    CHECK(!winWslParseGuid((const unsigned char *)"12345678-1234-5678-9012-123456789abc\0junk",41,guid));
    CHECK(winWslParseGuid((const unsigned char *)L"\xfeff" L"12345678-1234-5678-9012-123456789abc\r\n",78,guid));
    puts("PASS parser: UTF16 BOM/no BOM, CRLF, Unicode spaces, GUID, malformed/truncated/capacity");
}
typedef struct { int lists, guests; unsigned version; BOOL stopped, missing, stopRecheck, invalid; winWslResult failure; unsigned nixVersion; DWORD delay, guestBudget; } Fake;
static unsigned version(void *context,const wchar_t *name)
{ Fake *f=context; CHECK(!wcscmp(name,selected)||!wcscmp(name,L"NixOS")); return !wcscmp(name,L"NixOS") && f->nixVersion ? f->nixVersion : f->version; }
static winWslResult run(void *context,const wchar_t *const *args,unsigned char *out,size_t cap,size_t *bytes,DWORD timeout)
{
    Fake *f=context; CHECK(timeout>0&&timeout<=WIN_WSL_COMMAND_MS);
    if(f->failure!=WIN_WSL_SUCCESS) return f->failure;
    if(!wcscmp(args[0],L"--list")) {
        const wchar_t *names=L"NixOS\r\nWork \x65e5\x672c\r\n";
        BOOL running=!wcscmp(args[1],L"--running");
        CHECK(!wcscmp(args[running?2:1],L"--quiet")); CHECK(args[running?3:2]==NULL);
        f->lists++; if(f->delay) Sleep(f->delay);
        if(f->missing || (running&&(f->stopped||(f->stopRecheck&&f->lists>1)))) names=L"NixOS\r\n";
        *bytes=wcslen(names)*2; CHECK(*bytes<=cap); memcpy(out,names,*bytes);
    } else {
        CHECK(!wcscmp(args[0],L"--distribution")); CHECK(!wcscmp(args[1],selected));
        CHECK(!wcscmp(args[2],L"--exec")); CHECK(!wcscmp(args[3],L"wslinfo"));
        CHECK(!wcscmp(args[4],L"--vm-id")&&args[5]==NULL);
        f->guests++; f->guestBudget=timeout; *bytes=36; memcpy(out,f->invalid?"bad-guid-invalid-invalid-invalid-id!":vm1,36);
    }
    return WIN_WSL_SUCCESS;
}
static void queries(void)
{
    Fake f={0}; winWslRunner runner={run,version,&f}; wchar_t name[WIN_WSL_NAME_CAP]=L"unchanged"; char guid[37]="unchanged";
    f.version=2;
    CHECK(winWslQueryRunningDistro(&runner,selected,name,WIN_WSL_COMMAND_MS)==WIN_WSL_SUCCESS);
    CHECK(!wcscmp(name,selected));
    CHECK(winWslQueryVmId(&runner,selected,guid,WIN_WSL_COMMAND_MS)==WIN_WSL_SUCCESS);
    CHECK(!strcmp(guid,vm1)&&f.guests==1);
    f.guests=0;f.stopped=TRUE;
    CHECK(winWslQueryVmId(&runner,selected,guid,WIN_WSL_COMMAND_MS)==WIN_WSL_STOPPED);
    CHECK(f.guests==0&&!strcmp(guid,vm1));
    f.stopped=FALSE;f.missing=TRUE;
    CHECK(winWslQueryRunningDistro(&runner,selected,name,WIN_WSL_COMMAND_MS)==WIN_WSL_MISSING);
    f.missing=FALSE;f.version=1;
    CHECK(winWslQueryRunningDistro(&runner,NULL,name,WIN_WSL_COMMAND_MS)==WIN_WSL_UNSUPPORTED);
    CHECK(winWslQueryVmId(&runner,selected,guid,WIN_WSL_COMMAND_MS)==WIN_WSL_UNSUPPORTED&&f.guests==0);
    f.version=0;
    CHECK(winWslQueryRunningDistro(&runner,NULL,name,WIN_WSL_COMMAND_MS)==WIN_WSL_UNSUPPORTED);
    f.version=2;f.lists=0;f.stopRecheck=TRUE;
    CHECK(winWslQueryRunningDistro(&runner,selected,name,WIN_WSL_COMMAND_MS)==WIN_WSL_SUCCESS);
    CHECK(winWslQueryVmId(&runner,name,guid,WIN_WSL_COMMAND_MS)==WIN_WSL_STOPPED&&f.guests==0);
    f.stopRecheck=FALSE;f.invalid=TRUE;
    CHECK(winWslQueryVmId(&runner,selected,guid,WIN_WSL_COMMAND_MS)==WIN_WSL_UNSUPPORTED);
    CHECK(!strcmp(guid,vm1));
    f.invalid=FALSE; f.nixVersion=1;
    CHECK(winWslQueryRunningDistro(&runner,NULL,name,WIN_WSL_COMMAND_MS)==WIN_WSL_SUCCESS);
    CHECK(!wcscmp(name,selected));
    f.delay=40;
    CHECK(winWslQueryVmId(&runner,selected,guid,100)==WIN_WSL_SUCCESS);
    CHECK(f.guestBudget>0&&f.guestBudget<=60); f.delay=0;
    f.version=9; CHECK(winWslQueryVmId(&runner,selected,guid,100)==WIN_WSL_UNSUPPORTED); f.version=2;
    CHECK(winWslQueryRunningDistro(&runner,L"nixos",name,100)==WIN_WSL_MISSING);
    f.failure=WIN_WSL_TIMEOUT;
    CHECK(winWslQueryVmId(&runner,selected,guid,WIN_WSL_COMMAND_MS)==WIN_WSL_TIMEOUT);
    puts("PASS queries: exact selected name, stopped/missing/WSL1/unknown, recheck stops guest, invalid/timeout unchanged");
}
static void lifecycle(void)
{
    winWslDiscoveryState s={0}; Fake f={0};winWslRunner r={run,version,&f};char guid[37];unsigned serial;int i;
    f.version=2;
    CHECK(winWslObserveVm(&s,TRUE,500)); serial=s.serial;
    CHECK(winWslQueryVmId(&r,selected,guid,5000)==WIN_WSL_SUCCESS);
    CHECK(winWslPublishGuid(&s,guid,serial)&&s.dirty);
    for(i=0;i<40;i++)CHECK(!winWslObserveVm(&s,TRUE,500));
    CHECK(f.guests==1);CHECK(winWslAcceptGuid(&s,guid,serial));
    for(i=0;i<100;i++)CHECK(!winWslObserveVm(&s,TRUE,500));
    CHECK(f.guests==1&&!s.dirty&&!strcmp(s.bound,vm1));
    CHECK(!winWslObserveVm(&s,FALSE,500));CHECK(!s.dirty&&!s.pending[0]&&!s.bound[0]);
    CHECK(!winWslPublishGuid(&s,vm1,serial));CHECK(!winWslAcceptGuid(&s,vm1,serial));
    CHECK(winWslObserveVm(&s,TRUE,500));CHECK(winWslPublishGuid(&s,vm2,s.serial));
    CHECK(s.dirty&&!strcmp(s.pending,vm2)&&!s.bound[0]);
    CHECK(!winWslObserveVm(&s,FALSE,500));CHECK(!s.dirty);
    puts("PASS lifecycle: pending and bound suppress queries, disappearance invalidates, new id requests owner rebind");
}
static int child(int argc,char **argv)
{
    DWORD wrote; const char *mode=argv[1];
    if(!strcmp(mode,"stall")){DWORD pid=GetCurrentProcessId();WriteFile(GetStdHandle(STD_OUTPUT_HANDLE),&pid,sizeof(pid),&wrote,NULL);Sleep(10000);return 0;}
    if(!strcmp(mode,"flood")){char chunk[4096];int i;memset(chunk,'x',sizeof(chunk));for(i=0;i<50;i++)WriteFile(GetStdHandle(STD_OUTPUT_HANDLE),chunk,sizeof(chunk),&wrote,NULL);return 0;}
    if(!strcmp(mode,"descendant")) {
        wchar_t path[MAX_PATH],line[MAX_PATH+40];STARTUPINFOW si={0};PROCESS_INFORMATION pi={0};
        GetModuleFileNameW(NULL,path,MAX_PATH);swprintf(line,MAX_PATH+40,L"\"%ls\" stall",path);si.cb=sizeof(si);si.dwFlags=STARTF_USESTDHANDLES;si.hStdInput=GetStdHandle(STD_INPUT_HANDLE);si.hStdOutput=GetStdHandle(STD_OUTPUT_HANDLE);si.hStdError=GetStdHandle(STD_ERROR_HANDLE);
        CHECK(CreateProcessW(path,line,NULL,NULL,TRUE,CREATE_NO_WINDOW,NULL,NULL,&si,&pi));CloseHandle(pi.hThread);CloseHandle(pi.hProcess);return 0;
    }
    if(!strcmp(mode,"handle")) {
        HANDLE raw=(HANDLE)(ULONG_PTR)_strtoui64(argv[2],NULL,10), candidate=NULL;
        /* Numeric slots can be reused by unrelated child startup handles.
         * Capture any candidate before opening new handles, then compare the
         * actual object against a duplicate of the parent's known event. */
        if(DuplicateHandle(GetCurrentProcess(),raw,GetCurrentProcess(),&candidate,
                           0,FALSE,DUPLICATE_SAME_ACCESS)) {
            typedef BOOL (WINAPI *CompareFn)(HANDLE,HANDLE);
            CompareFn compare=(CompareFn)GetProcAddress(GetModuleHandleW(L"KernelBase.dll"),"CompareObjectHandles");
            HANDLE parent=OpenProcess(PROCESS_DUP_HANDLE,FALSE,(DWORD)strtoul(argv[3],NULL,10)),known=NULL;
            CHECK(compare&&parent);
            CHECK(DuplicateHandle(parent,raw,GetCurrentProcess(),&known,0,FALSE,DUPLICATE_SAME_ACCESS));
            CHECK(!compare(candidate,known));
            CloseHandle(known);CloseHandle(parent);CloseHandle(candidate);
        } else CHECK(GetLastError()==ERROR_INVALID_HANDLE);
        return 0;
    }
    if(!strcmp(mode,"wide-arg")) {
        wchar_t name[WIN_WSL_NAME_CAP];CHECK(argc==4);CHECK(winWslDistroArgument(2,name));
        CHECK(!wcscmp(name,selected));WriteFile(GetStdHandle(STD_OUTPUT_HANDLE),name,(DWORD)wcslen(name)*2,&wrote,NULL);return 0;
    }
    if(!strcmp(mode,"echo")) {
        int count;wchar_t **wide=CommandLineToArgvW(GetCommandLineW(),&count);int i;CHECK(wide&&count==5);
        CHECK(!wcscmp(wide[2],selected));CHECK(!wcscmp(wide[3],L"a\"b\\"));CHECK(!wcscmp(wide[4],L"& | $(literal)"));
        CHECK(GetConsoleWindow()==NULL);
        for(i=2;i<count;i++)WriteFile(GetStdHandle(STD_OUTPUT_HANDLE),wide[i],(DWORD)(wcslen(wide[i])+1)*2,&wrote,NULL);
        WriteFile(GetStdHandle(STD_ERROR_HANDLE),"ignored",7,&wrote,NULL);LocalFree(wide);return 0;
    }
    if(!strcmp(mode,"nonzero"))return 3;
    return 7;
}
static void process(void)
{
    wchar_t path[MAX_PATH],system[MAX_PATH],handleText[40],parentText[40];
    SECURITY_ATTRIBUTES sa={sizeof(sa),NULL,TRUE}; HANDLE unrelated=CreateEventW(&sa,TRUE,FALSE,NULL), descendant;
    DWORD pid=0; const wchar_t *handles[]={L"handle",handleText,parentText,NULL};unsigned char out[256];size_t bytes;DWORD start;
    const wchar_t *echo[]={L"echo",selected,L"a\"b\\",L"& | $(literal)",NULL};
    const wchar_t *wide[]={L"wide-arg",L"-wsldistro",selected,NULL};
    const wchar_t *stall[]={L"stall",NULL},*flood[]={L"flood",NULL},*desc[]={L"descendant",NULL},*fail[]={L"nonzero",NULL};
    CHECK(unrelated);swprintf(handleText,40,L"%llu",(unsigned long long)(ULONG_PTR)unrelated);
    swprintf(parentText,40,L"%lu",GetCurrentProcessId());
    GetModuleFileNameW(NULL,path,MAX_PATH);CHECK(winWslSystemPath(system,MAX_PATH));CHECK(wcsstr(system,L"\\wsl.exe"));
    CHECK(winWslRunProcess(path,echo,out,sizeof(out),&bytes,5000)==WIN_WSL_SUCCESS);CHECK(bytes>0);
    CHECK(!memcmp(out,selected,(wcslen(selected)+1)*2));
    CHECK(winWslRunProcess(path,wide,out,sizeof(out),&bytes,5000)==WIN_WSL_SUCCESS);CHECK(bytes==wcslen(selected)*2&&!memcmp(out,selected,bytes));
    CHECK(winWslRunProcess(path,handles,out,sizeof(out),&bytes,5000)==WIN_WSL_SUCCESS);CloseHandle(unrelated);
    CHECK(winWslRunProcess(path,fail,out,sizeof(out),&bytes,5000)==WIN_WSL_UNSUPPORTED);
    CHECK(winWslRunProcess(path,flood,out,sizeof(out),&bytes,5000)==WIN_WSL_UNSUPPORTED&&bytes==0);
    start=GetTickCount();CHECK(winWslRunProcess(path,stall,out,sizeof(out),&bytes,200)==WIN_WSL_TIMEOUT);CHECK(GetTickCount()-start<1500);
    memset(out,0,sizeof(out));start=GetTickCount();CHECK(winWslRunProcess(path,desc,out,sizeof(out),&bytes,5000)==WIN_WSL_TIMEOUT);CHECK(GetTickCount()-start<6500);
    memcpy(&pid,out,sizeof(pid));CHECK(pid);descendant=OpenProcess(SYNCHRONIZE,FALSE,pid);
    if(descendant){CHECK(WaitForSingleObject(descendant,1000)==WAIT_OBJECT_0);CloseHandle(descendant);}
    puts("PASS real hidden Unicode runner: exact argv, separate stderr, nonzero, output cap, timeout/descendant pipe deadline");
}
int main(int argc,char **argv)
{
    if(argc>1) { if(!strcmp(argv[1],"test-parser")){parser();return 0;} if(!strcmp(argv[1],"test-queries")){queries();return 0;} if(!strcmp(argv[1],"test-lifecycle")){lifecycle();return 0;} if(!strcmp(argv[1],"test-process")){process();return 0;} return child(argc,argv); }
    quoteHostOptions();parser();queries();lifecycle();process();return 0;
}
