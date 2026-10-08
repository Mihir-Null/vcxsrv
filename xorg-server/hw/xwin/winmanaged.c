#include "winmanaged.h"
#include <stdio.h>
#include <string.h>
#include <X11/Xauth.h>

int
winVsockLegacyOverrides(int managed, int argc, char *argv[])
{
    int i, flags = 0;
    /* True managed conflicts were rejected by parsed option positions before
     * winLocalManaged was enabled. Values such as a distro/file named -vmid
     * must never be reinterpreted as legacy switches here. */
    if (managed)
        return 0;
    for (i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "-vmid"))
            flags |= WIN_VSOCK_MANUAL_VM;
        if (i + 1 < argc && !strcmp(argv[i + 1], "hyperv")) {
            if (!strcmp(argv[i], "-listen"))
                flags |= WIN_VSOCK_MANUAL_LISTEN;
            if (!strcmp(argv[i], "-nolisten"))
                flags |= WIN_VSOCK_MANUAL_NOLISTEN;
        }
    }
    return flags;
}

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
    /* The Windows argument boundary validates the original wide name. */
    if (options->distro && !*options->distro)
        return "-wsldistro needs a nonempty name";
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
    int malformed = 0;
    if (!path || !*path)
        return "-wslmanaged requires an authority file";
    file = fopen(path, "rb");
    if (!file)
        return "managed authority file is unreadable";
    for (;;) {
        /* XauReadAuth returns NULL for both clean EOF and a truncated record.
         * Establish the record boundary before calling it so any NULL after
         * an available byte is an error, including allocation/read failures. */
        int first = fgetc(file);
        if (first == EOF) {
            malformed = ferror(file) != 0;
            break;
        }
        if (ungetc(first, file) == EOF || (auth = XauReadAuth(file)) == NULL) {
            malformed = 1;
            break;
        }
        if (auth->name_length == 18 && auth->data_length == 16 &&
            !memcmp(auth->name, "MIT-MAGIC-COOKIE-1", 18) &&
            auth->number_length == strlen(display) &&
            !memcmp(auth->number, display, auth->number_length))
            valid = 1;
        XauDisposeAuth(auth);
    }
    if (ferror(file)) malformed = 1;
    fclose(file);
    if (malformed)
        return "managed authority file contains a malformed or unreadable record";
    return valid ? NULL : "managed authority file needs a 16-byte MIT-MAGIC-COOKIE-1 entry for this display";
}
