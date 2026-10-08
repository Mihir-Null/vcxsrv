/* WSL discovery boundary: no X server calls, no shell, no guest polling. */
#if !defined(_WIN32_WINNT) || _WIN32_WINNT < 0x0601
#undef _WIN32_WINNT
#define _WIN32_WINNT 0x0601
#endif
#include "winwsldiscovery.h"
#include <shellapi.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>
#include <ctype.h>

static DWORD remaining(DWORD start, DWORD budget)
{
    DWORD elapsed = GetTickCount() - start;
    return elapsed >= budget ? 0 : budget - elapsed;
}

BOOL winWslValidName(const wchar_t *name)
{
    size_t i, len;
    if (!name || !(len = wcslen(name)) || len >= WIN_WSL_NAME_CAP)
        return FALSE;
    for (i = 0; i < len; i++) {
        unsigned c = name[i];
        if (c < 32 || (c >= 127 && c <= 159) || c == 0xfeff)
            return FALSE;
        if (c >= 0xd800 && c <= 0xdbff) {
            unsigned low = name[++i];
            if (low < 0xdc00 || low > 0xdfff)
                return FALSE;
        }
        else if (c >= 0xdc00 && c <= 0xdfff)
            return FALSE;
    }
    return TRUE;
}

winWslResult winWslDecode(const unsigned char *raw, size_t bytes,
                         wchar_t *out, size_t cap)
{
    size_t i, offset = 0, len;
    BOOL wide = FALSE;
    int converted;
    if (!cap || bytes > WIN_WSL_OUTPUT_CAP)
        return WIN_WSL_UNSUPPORTED;
    if (!bytes) { out[0] = 0; return WIN_WSL_SUCCESS; }
    if (bytes >= 2 && raw[0] == 0xff && raw[1] == 0xfe) {
        wide = TRUE;
        offset = 2;
    }
    else if (bytes >= 2 && raw[0] == 0xfe && raw[1] == 0xff)
        return WIN_WSL_UNSUPPORTED;
    else {
        for (i = 0; i < bytes; i++)
            if (!raw[i]) { wide = TRUE; break; }
    }
    if (wide) {
        if (bytes % 2) return WIN_WSL_UNSUPPORTED;
        len = (bytes - offset) / 2;
        if (len >= cap) return WIN_WSL_UNSUPPORTED;
        for (i = 0; i < len; i++)
            out[i] = raw[offset + i * 2] | ((unsigned)raw[offset + i * 2 + 1] << 8);
        out[len] = 0;
    }
    else {
        if (bytes >= 3 && raw[0] == 0xef && raw[1] == 0xbb && raw[2] == 0xbf)
            offset = 3;
        if (bytes == offset) { out[0] = 0; return WIN_WSL_SUCCESS; }
        converted = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
            (const char *)raw + offset, (int)(bytes - offset), out, (int)cap - 1);
        if (!converted) return WIN_WSL_UNSUPPORTED;
        len = converted;
        out[len] = 0;
    }
    for (i = 0; i < len; i++) {
        unsigned c = out[i];
        if (!c || (c < 32 && c != '\r' && c != '\n') || (c >= 127 && c <= 159) || c == 0xfeff)
            return WIN_WSL_UNSUPPORTED;
        if (c >= 0xd800 && c <= 0xdbff) {
            if (++i >= len || out[i] < 0xdc00 || out[i] > 0xdfff)
                return WIN_WSL_UNSUPPORTED;
        }
        else if (c >= 0xdc00 && c <= 0xdfff)
            return WIN_WSL_UNSUPPORTED;
    }
    return WIN_WSL_SUCCESS;
}

BOOL winWslParseGuid(const unsigned char *raw, size_t bytes, char guid[37])
{
    wchar_t text[80];
    char parsed[37];
    size_t len, i;
    if (winWslDecode(raw, bytes, text, 80) != WIN_WSL_SUCCESS)
        return FALSE;
    len = wcslen(text);
    while (len && (text[len - 1] == '\r' || text[len - 1] == '\n'))
        text[--len] = 0;
    if (len != 36) return FALSE;
    for (i = 0; i < 36; i++) {
        unsigned c = text[i];
        if (i == 8 || i == 13 || i == 18 || i == 23) {
            if (c != '-') return FALSE;
        }
        else if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') ||
                   (c >= 'A' && c <= 'F')))
            return FALSE;
        parsed[i] = (char)c;
    }
    parsed[36] = 0;
    memcpy(guid, parsed, sizeof(parsed));
    return TRUE;
}

/* Standard Windows argv escaping: double runs of backslashes before quotes
 * and before the closing quote. Metacharacters have no shell interpretation. */
BOOL winWslQuoteArgs(const wchar_t *const *args, wchar_t *out, size_t cap)
{
    size_t at = 0, a, i, n;
#define PUT(c) do { if (at + 1 >= cap) return FALSE; out[at++] = (c); } while (0)
    if (!cap) return FALSE;
    for (a = 0; args[a]; a++) {
        if (a) PUT(' ');
        PUT('"');
        for (i = 0; args[a][i];) {
            n = 0;
            while (args[a][i] == '\\') { n++; i++; }
            if (args[a][i] == '"' || !args[a][i]) {
                size_t j;
                for (j = 0; j < n * 2; j++) PUT('\\');
                if (!args[a][i]) break;
                PUT('\\'); PUT('"'); i++;
            }
            else {
                size_t j;
                for (j = 0; j < n; j++) PUT('\\');
                PUT(args[a][i++]);
            }
        }
        PUT('"');
    }
    out[at] = 0;
#undef PUT
    return TRUE;
}

BOOL winWslDistroArgument(int optionIndex, wchar_t name[WIN_WSL_NAME_CAP])
{
    int argc;
    wchar_t **argv = CommandLineToArgvW(GetCommandLineW(), &argc);
    BOOL ok = FALSE;
    /* Use the option position observed by the server's real parser, not a
     * scan which could mistake an -auth value for a distro option. */
    if (argv && optionIndex >= 1 && optionIndex + 1 < argc &&
        !wcscmp(argv[optionIndex], L"-wsldistro") &&
        winWslValidName(argv[optionIndex + 1])) {
        wcscpy(name, argv[optionIndex + 1]);
        ok = TRUE;
    }
    if (argv) LocalFree(argv);
    return ok;
}

BOOL winWslSystemPath(wchar_t *out, size_t cap)
{
    UINT len;
#ifndef _WIN64
    BOOL wow64 = FALSE;
    if (!IsWow64Process(GetCurrentProcess(), &wow64)) return FALSE;
    if (wow64) {
        len = GetWindowsDirectoryW(out, (UINT)cap);
        if (!len || len >= cap || len + 18 >= cap) return FALSE;
        wcscat(out, L"\\Sysnative\\wsl.exe");
        return TRUE;
    }
#endif
    len = GetSystemDirectoryW(out, (UINT)cap);
    if (!len || len >= cap || len + 8 >= cap) return FALSE;
    wcscat(out, L"\\wsl.exe");
    return TRUE;
}

winWslResult winWslRunProcess(const wchar_t *application,
    const wchar_t *const *args, unsigned char *out, size_t cap,
    size_t *bytes, DWORD timeout)
{
    SECURITY_ATTRIBUTES sa = { sizeof(sa), NULL, TRUE };
    STARTUPINFOEXW si;
    PROCESS_INFORMATION pi = {0};
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits = {0};
    HANDLE readPipe = NULL, writePipe = NULL, input = INVALID_HANDLE_VALUE;
    HANDLE error = INVALID_HANDLE_VALUE, job = NULL, inherited[3];
    SIZE_T attributeBytes = 0;
    const wchar_t *argv[66];
    wchar_t *line = NULL;
    size_t count = 0, total = 0;
    DWORD start = GetTickCount(), exitCode = 1;
    BOOL created = FALSE, attributes = FALSE;
    winWslResult result = WIN_WSL_UNSUPPORTED;
    *bytes = 0;
    ZeroMemory(&si, sizeof(si));
    if (!application || !cap || cap > WIN_WSL_OUTPUT_CAP || !timeout)
        return timeout ? WIN_WSL_UNSUPPORTED : WIN_WSL_TIMEOUT;
    argv[count++] = application;
    while (*args) {
        if (count >= 65) goto done;
        argv[count++] = *args++;
    }
    argv[count] = NULL;
    line = malloc(32768 * sizeof(wchar_t));
    if (!line || !winWslQuoteArgs(argv, line, 32768)) goto done;
    if (!CreatePipe(&readPipe, &writePipe, &sa, 0) ||
        !SetHandleInformation(readPipe, HANDLE_FLAG_INHERIT, 0)) goto done;
    input = CreateFileW(L"NUL", GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
                        &sa, OPEN_EXISTING, 0, NULL);
    error = CreateFileW(L"NUL", GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE,
                        &sa, OPEN_EXISTING, 0, NULL);
    if (input == INVALID_HANDLE_VALUE || error == INVALID_HANDLE_VALUE) goto done;
    job = CreateJobObjectW(NULL, NULL);
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if (!job || !SetInformationJobObject(job, JobObjectExtendedLimitInformation,
                                        &limits, sizeof(limits))) goto done;
    InitializeProcThreadAttributeList(NULL, 1, 0, &attributeBytes);
    si.lpAttributeList = malloc(attributeBytes);
    if (!si.lpAttributeList || !InitializeProcThreadAttributeList(si.lpAttributeList,
                                                1, 0, &attributeBytes)) goto done;
    attributes = TRUE;
    inherited[0] = input; inherited[1] = writePipe; inherited[2] = error;
    if (!UpdateProcThreadAttribute(si.lpAttributeList, 0,
        PROC_THREAD_ATTRIBUTE_HANDLE_LIST, inherited, sizeof(inherited), NULL, NULL)) goto done;
    si.StartupInfo.cb = sizeof(si);
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES | STARTF_USESHOWWINDOW;
    si.StartupInfo.wShowWindow = SW_HIDE;
    si.StartupInfo.hStdInput = input;
    si.StartupInfo.hStdOutput = writePipe;
    si.StartupInfo.hStdError = error;
    if (!CreateProcessW(application, line, NULL, NULL, TRUE,
        CREATE_NO_WINDOW | CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT,
        NULL, NULL, &si.StartupInfo, &pi)) goto done;
    created = TRUE;
    if (!AssignProcessToJobObject(job, pi.hProcess) || ResumeThread(pi.hThread) == (DWORD)-1)
        goto done;
    CloseHandle(writePipe); writePipe = NULL;
    for (;;) {
        DWORD available = 0, got = 0, left;
        BOOL eof = FALSE;
        unsigned char chunk[4096];
        if (!PeekNamedPipe(readPipe, NULL, 0, NULL, &available, NULL)) {
            if (GetLastError() != ERROR_BROKEN_PIPE) goto done;
            eof = TRUE;
        }
        if (available) {
            DWORD amount = available > sizeof(chunk) ? sizeof(chunk) : available;
            if (!ReadFile(readPipe, chunk, amount, &got, NULL) || !got) goto done;
            if (got > cap - total) goto done;
            memcpy(out + total, chunk, got); total += got;
        }
        if (eof && WaitForSingleObject(pi.hProcess, 0) == WAIT_OBJECT_0) {
            if (!GetExitCodeProcess(pi.hProcess, &exitCode)) goto done;
            if (!exitCode) { result = WIN_WSL_SUCCESS; *bytes = total; }
            goto done;
        }
        left = remaining(start, timeout);
        if (!left) { result = WIN_WSL_TIMEOUT; goto done; }
        if (!available) Sleep(left < 5 ? left : 5);
    }
done:
    /* No unbounded EOF/read/wait after timeout. Closing the job kills descendants
     * too, including any retaining the pipe after the immediate child exits. */
    if (created && result != WIN_WSL_SUCCESS) TerminateProcess(pi.hProcess, 1);
    if (job) CloseHandle(job);
    if (pi.hThread) CloseHandle(pi.hThread);
    if (pi.hProcess) CloseHandle(pi.hProcess);
    if (attributes) DeleteProcThreadAttributeList(si.lpAttributeList);
    free(si.lpAttributeList); free(line);
    if (readPipe) CloseHandle(readPipe);
    if (writePipe) CloseHandle(writePipe);
    if (input != INVALID_HANDLE_VALUE) CloseHandle(input);
    if (error != INVALID_HANDLE_VALUE) CloseHandle(error);
    return result;
}

/* Host-only compatibility check. The per-user Store WSL registration layout
 * is a compatibility dependency; unreadable/malformed/unknown data is NOT
 * permission to execute inside a possible WSL1 guest. No registry is changed. */
static unsigned hostVersion(void *context, const wchar_t *name)
{
    HKEY root, distro;
    DWORD i, version = 0;
    (void)context;
    if (RegOpenKeyExW(HKEY_CURRENT_USER,
        L"Software\\Microsoft\\Windows\\CurrentVersion\\Lxss", 0,
        KEY_READ, &root) != ERROR_SUCCESS) return 0;
    for (i = 0; i < 1024; i++) {
        wchar_t subkey[64], registered[WIN_WSL_NAME_CAP];
        DWORD chars = 64, bytes = sizeof(registered), type, value, size = sizeof(value);
        LONG status = RegEnumKeyExW(root, i, subkey, &chars, NULL, NULL, NULL, NULL);
        if (status == ERROR_NO_MORE_ITEMS) break;
        if (status != ERROR_SUCCESS) continue;
        if (RegOpenKeyExW(root, subkey, 0, KEY_READ, &distro) != ERROR_SUCCESS) continue;
        if (RegQueryValueExW(distro, L"DistributionName", NULL, &type,
            (BYTE *)registered, &bytes) == ERROR_SUCCESS && type == REG_SZ &&
            bytes >= 2 && bytes % 2 == 0 && bytes <= sizeof(registered) &&
            registered[bytes / 2 - 1] == 0 &&
            wcslen(registered) == bytes / 2 - 1 && !wcscmp(registered, name) &&
            RegQueryValueExW(distro, L"Version", NULL, &type, (BYTE *)&value,
                             &size) == ERROR_SUCCESS && type == REG_DWORD &&
            size == sizeof(value)) {
            version = value;
            RegCloseKey(distro);
            break;
        }
        RegCloseKey(distro);
    }
    RegCloseKey(root);
    return version;
}

static winWslResult systemRun(void *context, const wchar_t *const *args,
    unsigned char *out, size_t cap, size_t *bytes, DWORD timeout)
{
    wchar_t path[MAX_PATH];
    (void)context;
    if (!winWslSystemPath(path, MAX_PATH)) return WIN_WSL_UNSUPPORTED;
    return winWslRunProcess(path, args, out, cap, bytes, timeout);
}

const winWslRunner *winWslDefaultRunner(void)
{
    static const winWslRunner runner = { systemRun, hostVersion, NULL };
    return &runner;
}

/* Validate every name before using any. Names are preserved exactly: only
 * CR/LF delimiters are removed; spaces, case and Unicode remain significant. */
static winWslResult listDistro(const winWslRunner *runner, BOOL running,
    const wchar_t *selected, wchar_t found[WIN_WSL_NAME_CAP], DWORD timeout)
{
    const wchar_t *runningArgs[] = { L"--list", L"--running", L"--quiet", NULL };
    const wchar_t *allArgs[] = { L"--list", L"--quiet", NULL };
    unsigned char *raw = malloc(WIN_WSL_OUTPUT_CAP);
    wchar_t *text = malloc((WIN_WSL_OUTPUT_CAP / 2 + 1) * sizeof(wchar_t));
    wchar_t candidate[WIN_WSL_NAME_CAP] = L"", *p;
    size_t bytes = 0;
    winWslResult result = WIN_WSL_UNSUPPORTED;
    if (!raw || !text) goto done;
    result = runner->run(runner->context, running ? runningArgs : allArgs,
                        raw, WIN_WSL_OUTPUT_CAP, &bytes, timeout);
    if (result != WIN_WSL_SUCCESS) goto done;
    result = winWslDecode(raw, bytes, text, WIN_WSL_OUTPUT_CAP / 2 + 1);
    if (result != WIN_WSL_SUCCESS) goto done;
    result = WIN_WSL_MISSING;
    p = text;
    while (*p) {
        wchar_t *end = wcschr(p, '\n'), *next;
        if (end) { next = end + 1; *end = 0; }
        else { end = p + wcslen(p); next = end; }
        if (end > p && end[-1] == '\r') end[-1] = 0;
        if (!winWslValidName(p)) { result = WIN_WSL_UNSUPPORTED; goto done; }
        if ((!selected || !wcscmp(p, selected)) && !candidate[0]) {
            if (!running || runner->version(runner->context, p) == 2)
                wcscpy(candidate, p);
            else if (selected)
                result = WIN_WSL_UNSUPPORTED;
        }
        p = next;
    }
    if (candidate[0]) { wcscpy(found, candidate); result = WIN_WSL_SUCCESS; }
    else if (!selected && text[0]) result = WIN_WSL_UNSUPPORTED;
done:
    free(raw); free(text);
    return result;
}

winWslResult winWslQueryRunningDistro(const winWslRunner *runner,
    const wchar_t *selected, wchar_t name[WIN_WSL_NAME_CAP], DWORD timeout)
{
    wchar_t found[WIN_WSL_NAME_CAP];
    DWORD start = GetTickCount(), left;
    winWslResult result;
    if (!timeout) return WIN_WSL_TIMEOUT;
    if (selected && !winWslValidName(selected)) return WIN_WSL_UNSUPPORTED;
    result = listDistro(runner, TRUE, selected, found, timeout);
    if (result == WIN_WSL_SUCCESS) { wcscpy(name, found); return result; }
    if (result != WIN_WSL_MISSING || !selected) return result;
    left = remaining(start, timeout);
    if (!left) return WIN_WSL_TIMEOUT;
    result = listDistro(runner, FALSE, selected, found, left);
    return result == WIN_WSL_SUCCESS ? WIN_WSL_STOPPED : result;
}

winWslResult winWslQueryVmId(const winWslRunner *runner,
    const wchar_t *distro, char guid[37], DWORD timeout)
{
    wchar_t running[WIN_WSL_NAME_CAP];
    const wchar_t *args[] = { L"--distribution", distro, L"--exec", L"wslinfo", L"--vm-id", NULL };
    unsigned char raw[256];
    size_t bytes = 0;
    DWORD start = GetTickCount(), left;
    winWslResult result;
    if (!distro || !winWslValidName(distro)) return WIN_WSL_UNSUPPORTED;
    result = winWslQueryRunningDistro(runner, distro, running, timeout);
    if (result != WIN_WSL_SUCCESS) return result;
    left = remaining(start, timeout);
    if (!left) return WIN_WSL_TIMEOUT;
    /* Membership is rechecked immediately before execution, but list/exec is
     * not atomic. A concurrent stop after this check may still start WSL. */
    result = runner->run(runner->context, args, raw, sizeof(raw), &bytes, left);
    if (result != WIN_WSL_SUCCESS) return result;
    return winWslParseGuid(raw, bytes, guid) ? WIN_WSL_SUCCESS : WIN_WSL_UNSUPPORTED;
}

BOOL winWslObserveVm(winWslDiscoveryState *state, BOOL present, DWORD tick)
{
    if (!present) {
        if (state->wasUp || state->dirty || state->bound[0]) state->serial++;
        state->wasUp = FALSE; state->dirty = FALSE;
        state->pending[0] = state->bound[0] = 0; state->stateTick = 0;
        return FALSE;
    }
    if (!state->wasUp) { state->wasUp = TRUE; state->stateTick = 0; return TRUE; }
    if (state->bound[0] || state->dirty) return FALSE;
    state->stateTick += tick;
    if (state->stateTick < 1000) return FALSE;
    state->stateTick = 0;
    return TRUE;
}

BOOL winWslPublishGuid(winWslDiscoveryState *state, const char *guid, unsigned serial)
{
    char checked[37];
    if (!state->wasUp || serial != state->serial ||
        !winWslParseGuid((const unsigned char *)guid, strlen(guid), checked)) return FALSE;
    if (!_stricmp(guid, state->bound)) return FALSE;
    strcpy(state->pending, checked); state->dirty = TRUE;
    return TRUE;
}

BOOL winWslAcceptGuid(winWslDiscoveryState *state, const char *guid, unsigned serial)
{
    if (!state->wasUp || !state->dirty || serial != state->serial ||
        strcmp(guid, state->pending)) return FALSE;
    strcpy(state->bound, guid); state->dirty = FALSE;
    return TRUE;
}
