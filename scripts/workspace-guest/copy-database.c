/* Persist an unpublished SQLite page image. Do not allocate a second copy in
 * the guest page cache. Keep a short queue of contiguous aligned extents on
 * the same work disk, then wait for completion and fsync before activation.
 * Unsupported kernels/filesystems use a buffered copy to the same private fd. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/aio_abi.h>
#include <linux/falloc.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <unistd.h>

enum { CHUNK = 1 << 20, DEPTH = 8 };
static int read_all(int fd, char *buf, size_t len, off_t off) {
    while (len) {
        ssize_t n = pread(fd, buf, len, off);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0) { if (!n) errno = EIO; return -1; }
        buf += n; off += n; len -= n;
    }
    return 0;
}
static int buffered(int src, int dst, char *buf, off_t size) {
    if (fcntl(dst, F_SETFL, fcntl(dst, F_GETFL) & ~O_DIRECT) < 0) return -1;
    for (off_t off = 0; off < size;) {
        size_t len = size-off < CHUNK ? size-off : CHUNK;
        if (read_all(src,buf,len,off)) return -1;
        size_t done = 0;
        while (done < len) {
            ssize_t n = pwrite(dst,buf+done,len-done,off+done);
            if (n<0 && errno == EINTR) continue;
            if (n<=0) { if (!n) errno = EIO; return -1; }
            done+=n;
        }
        off+=len;
    }
    return 0;
}
static int copy(int src, int dst, char *buf, off_t size) {
    aio_context_t ctx = 0;
    if (size % 4096 || fcntl(dst,F_SETFL,fcntl(dst,F_GETFL)|O_DIRECT)<0 || syscall(SYS_io_setup,DEPTH,&ctx)<0)
        return buffered(src,dst,buf,size);
    int err = 0;
    for (off_t off=0; off<size && !err;) {
        struct iocb cb[DEPTH] = {{0}}, *queue[DEPTH];
        struct io_event events[DEPTH]; int count=0, sent=0, done=0;
        while (count<DEPTH && off<size) {
            size_t n=size-off<CHUNK ? size-off : CHUNK;
            char *slice=buf+(size_t)count*CHUNK;
            if (read_all(src,slice,n,off)) { err=errno; break; }
            cb[count].aio_lio_opcode=IOCB_CMD_PWRITE; cb[count].aio_fildes=dst;
            cb[count].aio_buf=(uintptr_t)slice; cb[count].aio_nbytes=n;
            cb[count].aio_offset=off; cb[count].aio_data=n;queue[count]=cb+count;
            count++; off+=n;
        }
        while (!err && sent<count) {
            long n=syscall(SYS_io_submit,ctx,(long)(count-sent),queue+sent);
            if (n<0 && errno==EINTR) continue;
            if (n<=0) {err=n<0?errno:EIO; break;}
            sent+=n;
        }
        while (done<sent) {
            long n=syscall(SYS_io_getevents,ctx,1L,(long)(sent-done),events,NULL);
            if (n<0 && errno==EINTR) continue;
            if (n<=0) {err=n<0?errno:EIO;break;}
            for (long i=0;i<n;i++) if (events[i].res!=(long long)events[i].data)
                err=events[i].res<0 ? (int)-events[i].res : EIO;
            done+=n;
        }
    }
    if (syscall(SYS_io_destroy,ctx)<0) return -1;
    if (err==EINVAL || err==ENOSYS || err==EOPNOTSUPP || err==EAGAIN)
        return buffered(src,dst,buf,size);
    if (err) {errno=err; return -1;}
    return 0;
}
int main(int argc,char **argv) {
    if (argc!=3) return 2;
    int src=open(argv[1],O_RDONLY|O_NOFOLLOW);struct stat st;
    if (src<0 || fstat(src,&st) || !S_ISREG(st.st_mode) || st.st_size<=0) return 1;
    void *buf=NULL; if (posix_memalign(&buf,4096,(size_t)CHUNK*DEPTH)) return 1;
    int dst=open(argv[2],O_RDWR|O_CREAT|O_EXCL|O_NOFOLLOW,0600);
    if (dst<0) {perror("open private index");return 1;}
    if (fallocate(dst,FALLOC_FL_KEEP_SIZE,0,st.st_size) && errno!=EOPNOTSUPP && errno!=ENOSYS) {perror("reserve private index");return 1;}
    if (copy(src,dst,buf,st.st_size) || fsync(dst) || close(dst)) {perror("persist private index");return 1;}
    free(buf);return close(src)!=0;
}
