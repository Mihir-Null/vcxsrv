/* Optional installed-WSL acceptance. Never registers/stops/starts a distro;
 * VM-id execution is explicit and goes through the production running gate. */
#include "../../xorg-server/hw/xwin/winwsldiscovery.h"
#include <stdio.h>
#include <wchar.h>

int wmain(int argc, wchar_t **argv)
{
    const winWslRunner *runner = winWslDefaultRunner();
    const wchar_t *args[] = {L"--list",L"--running",L"--quiet",NULL};
    const wchar_t *distro = argc > 1 ? argv[1] : L"NixOS";
    unsigned char raw[WIN_WSL_OUTPUT_CAP];
    wchar_t decoded[WIN_WSL_OUTPUT_CAP / 2 + 1], running[WIN_WSL_NAME_CAP];
    size_t bytes = 0;
    char guid[37];
    winWslResult result;
    if (!winWslValidName(distro)) {
        fputs("FAIL invalid selected distro\n",stderr);
        return 1;
    }
    result = runner->run(runner->context,args,raw,sizeof(raw),&bytes,WIN_WSL_COMMAND_MS);
    if (result != WIN_WSL_SUCCESS ||
        winWslDecode(raw,bytes,decoded,sizeof(decoded)/sizeof(decoded[0])) != WIN_WSL_SUCCESS) {
        fprintf(stderr,"FAIL actual WSL host list: status=%d bytes=%llu\n",result,(unsigned long long)bytes);
        return 1;
    }
    result = winWslQueryRunningDistro(runner,distro,running,WIN_WSL_COMMAND_MS);
    if (result != WIN_WSL_SUCCESS && result != WIN_WSL_STOPPED && result != WIN_WSL_MISSING) {
        fprintf(stderr,"FAIL actual selected running query: status=%d\n",result);
        return 1;
    }
    printf("PASS actual installed WSL host list: bytes=%llu selected-status=%d\n",(unsigned long long)bytes,result);
    if (argc > 2 && !wcscmp(argv[2],L"--query-vm-id")) {
        if (result != WIN_WSL_SUCCESS) {
            fprintf(stderr,"FAIL selected distro is not confirmed running; guest query skipped\n");
            return 1;
        }
        result = winWslQueryVmId(runner,distro,guid,WIN_WSL_COMMAND_MS);
        if (result != WIN_WSL_SUCCESS) {
            fprintf(stderr,"FAIL actual gated VM-id query: status=%d\n",result);
            return 1;
        }
        printf("PASS actual already-running selected WSL2 VM-id: %s\n",guid);
    }
    return 0;
}
