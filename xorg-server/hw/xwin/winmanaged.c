#include "winmanaged.h"
#include <stdio.h>
#include <string.h>
#include <X11/Xauth.h>

int
winManagedObserve(winManagedOptions *options, int argc, char *argv[], int i)
{
    const char *arg = argv[i];
    const char *value = i + 1 < argc ? argv[i + 1] : NULL;
    if (!strcmp(arg, "-wslmanaged")) {
        options->enabled = 1;
        return 1;
    }
    if (!strcmp(arg, "-wsldistro")) {
        options->distro = value ? value : "";
        if (!value || !*value) options->conflict = "-wsldistro needs a nonempty name";
        return value ? 2 : 1;
    }
    if (!strcmp(arg, "-auth")) options->authority = value;
    if (!strcmp(arg, "-ac") || !strcmp(arg, "-nowslvsock") ||
        !strcmp(arg, "-noclipboard") || !strcmp(arg, "-rootless") ||
        !strcmp(arg, "-fullscreen") || !strcmp(arg, "-nodecoration") ||
        !strcmp(arg, "-vmid") || !strcmp(arg, "-displayfd") || !strcmp(arg, "-query") ||
        !strcmp(arg, "-broadcast") || !strcmp(arg, "-indirect"))
        options->conflict = arg;
    if (value && !strcmp(arg, "-listen") &&
        (!strcmp(value, "tcp") || !strcmp(value, "inet") || !strcmp(value, "inet6")))
        options->conflict = "-listen tcp/inet/inet6";
    if (value && !strcmp(arg, "-nolisten") &&
        (!strcmp(value, "winlocal") || !strcmp(value, "hyperv") || !strcmp(value, "local")))
        options->conflict = "-nolisten winlocal/hyperv/local";
    return 0;
}

const char *
winManagedValidate(const winManagedOptions *options, int noListenAll,
                   int defeatAccessControl)
{
    const unsigned char *p;
    /* The current char **argv and wsl.exe capture path support only a narrow
     * ASCII token. Reject other names rather than querying a different distro.
     * Task 3 replaces this boundary with lossless wide-character discovery. */
    if (options->distro) {
        if (!*options->distro || strlen(options->distro) >= 64)
            return "-wsldistro name must contain 1 to 63 ASCII token characters";
        for (p = (const unsigned char *) options->distro; *p; p++)
            if (!((*p >= 'A' && *p <= 'Z') || (*p >= 'a' && *p <= 'z') ||
                  (*p >= '0' && *p <= '9') || *p == '_' || *p == '-' || *p == '.'))
                return "-wsldistro currently requires an ASCII token name";
    }
    if (!options->enabled)
        return NULL;
    if (!options->authority || !*options->authority)
        return "-wslmanaged requires -auth <authority-file>";
    if (defeatAccessControl)
        return "-wslmanaged forbids -ac";
    if (noListenAll)
        return "-wslmanaged requires its local and Hyper-V listeners";
    if (options->conflict)
        return options->conflict;
    return NULL;
}

const char *
winManagedAuthorityError(const char *path, const char *display)
{
    FILE *file;
    Xauth *auth;
    int valid = 0;
    if (!path || !*path)
        return "-wslmanaged requires an authority file";
    file = fopen(path, "rb");
    if (!file)
        return "managed authority file is unreadable";
    while ((auth = XauReadAuth(file)) != NULL) {
        if (auth->name_length == 18 && auth->data_length == 16 &&
            !memcmp(auth->name, "MIT-MAGIC-COOKIE-1", 18) &&
            auth->number_length == strlen(display) &&
            !memcmp(auth->number, display, auth->number_length))
            valid = 1;
        XauDisposeAuth(auth);
    }
    if (ferror(file)) valid = 0;
    fclose(file);
    return valid ? NULL : "managed authority file needs a 16-byte MIT-MAGIC-COOKIE-1 entry for this display";
}
