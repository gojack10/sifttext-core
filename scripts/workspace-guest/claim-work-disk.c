/* Called after the public filesystem has been unmounted, in a private mount
 * namespace. Pin the kernel's exclusive claim for the ENTIRE private mount:
 * loop holds a reference to the open block file, not another open by name.
 * A live filesystem (including a detached mount) prevents the claim. No
 * sectors are reserved, copied, moved, or served from a different device. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/fs.h>
#include <linux/loop.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <unistd.h>

static const char disk[] = "/dev/vdb", target[] = "/run/workspace-disk";
static int unavailable(const char *reason) {
    fprintf(stderr, "index-phase private disk unavailable: %s: %s\n", reason, strerror(errno));
    return 77; /* no mount was installed */
}
int main(int argc, char **argv) {
    (void)argv;
    if (argc != 1 || geteuid() != 0) return 1;
    struct stat ns, init, st;
    if (stat("/proc/self/ns/mnt", &ns) || stat("/proc/1/ns/mnt", &init) ||
        (ns.st_dev == init.st_dev && ns.st_ino == init.st_ino)) return 1;
    int fd = open(disk, O_RDWR | O_EXCL | O_CLOEXEC | O_NOFOLLOW);
    if (fd < 0) return unavailable("block claim");
    int logical;
    if (fstat(fd, &st) || !S_ISBLK(st.st_mode) || ioctl(fd, BLKSSZGET, &logical)) return 1;
    int ctl = open("/dev/loop-control", O_RDWR | O_CLOEXEC);
    if (ctl < 0) return unavailable("loop control");
    int n = ioctl(ctl, LOOP_CTL_GET_FREE);
    if (n < 0) return unavailable("allocate loop");
    char node[64]; snprintf(node, sizeof(node), "/dev/loop%d", n);
    int loop = open(node, O_RDWR | O_CLOEXEC | O_NOFOLLOW);
    if (loop < 0) return unavailable("open loop");
    struct loop_config cfg = {0};
    cfg.fd = fd; cfg.block_size = logical;
    cfg.info.lo_flags = LO_FLAGS_AUTOCLEAR | LO_FLAGS_DIRECT_IO;
    if (ioctl(loop, LOOP_CONFIGURE, &cfg)) return unavailable("attach claimed block file");
    /* Keep the existing inode reader's device check: inside this namespace
     * its fixed pathname must name the exact device containing the checkout. */
    if (mount(node, disk, NULL, MS_BIND, NULL) ||
        mount(disk, target, "ext4", MS_NOSUID | MS_NODEV, NULL)) {
        perror("mount claimed work disk"); return 1;
    }
    fprintf(stderr, "index-phase exclusive private work disk mounted\n");
    return 0; /* ext4 retains loop; final unmount releases loop and its claim */
}
