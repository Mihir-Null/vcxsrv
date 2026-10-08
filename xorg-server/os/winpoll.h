#ifndef VCX_WINPOLL_H
#define VCX_WINPOLL_H
#define WINPOLL_IN 1
#define WINPOLL_PRI 2
#define WINPOLL_OUT 4
#define WINPOLL_ERR 8
#define WINPOLL_HUP 16
#define WINPOLL_NVAL 32
struct winpollfd {
    int fd;
    short events, revents;
};
/* Caller owns sockets exclusively during wait. They remain nonblocking.
 * Returns -1 on cancellation (WSAEINTR) or adapter failure. No callbacks. */
int winpoll_wait(struct winpollfd* fds, unsigned long count, int timeout, void* cancel);
#endif
