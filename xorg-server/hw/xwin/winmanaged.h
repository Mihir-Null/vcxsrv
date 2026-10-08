/* Final managed-profile policy, independent of Windows UI initialization. */
#ifndef WINMANAGED_H
#define WINMANAGED_H

typedef struct {
    int enabled;
    const char *authority;
    const char *distro;
    const char *conflict;
} winManagedOptions;

/* Observe only actual option positions visited by ddxProcessArgument.
 * Returns consumed arguments for the new options, zero for generic options. */
int winManagedObserve(winManagedOptions *, int argc, char *argv[], int i);
const char *winManagedValidate(const winManagedOptions *, int noListenAll,
                              int defeatAccessControl);
const char *winManagedAuthorityError(const char *path, const char *display);

enum {
    WIN_VSOCK_MANUAL_VM = 1,
    WIN_VSOCK_MANUAL_LISTEN = 2,
    WIN_VSOCK_MANUAL_NOLISTEN = 4
};
/* Legacy raw option detection is used only outside the validated profile. */
int winVsockLegacyOverrides(int managed, int argc, char *argv[]);

#endif
