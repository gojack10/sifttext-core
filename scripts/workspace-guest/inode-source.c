/* Read-only source for the managed ext4 guest. Stream only the small, ordinary
 * authored layout. Core must discard the ENTIRE stream on a nonzero exit.
 * Match both immutable Git subtree IDs before success. Never freeze a mount. */
#include <ext2fs/ext2fs.h>
#include <openssl/sha.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <unistd.h>
static ext2_filsys fs;
static unsigned files, syms, dirs;
static unsigned long long bytes;
struct item {
  char *key;
  char mode[7];
  unsigned char hash[20];
};
struct list {
  struct item *v;
  size_t n, cap;
  const char *path;
  int depth;
};
static void ck(errcode_t e) {
  if (e) {
    fprintf(stderr, "ext-tree error %ld\n", (long)e);
    exit(2);
  }
}
static int by_name(const void *a, const void *b) {
  return strcmp(((const struct item *)a)->key, ((const struct item *)b)->key);
}
static void header(SHA_CTX *c, const char *kind, unsigned long n) {
  char buf[64];
  int len = snprintf(buf, sizeof buf, "%s %lu", kind, n);
  SHA1_Init(c);
  SHA1_Update(c, buf, (size_t)len + 1);
}
static void hex(const unsigned char hash[20], char out[41]) {
  static const char digit[] = "0123456789abcdef";
  for (int k = 0; k < 20; k++) {
    out[2*k] = digit[hash[k] >> 4];
    out[2*k+1] = digit[hash[k] & 15];
  }
  out[40] = 0;
}
static int uuid_name(const char *s) {
  if (strlen(s) != 36)
    return 0;
  for (int n = 0; n < 36; n++)
    if (n == 8 || n == 13 || n == 18 || n == 23
            ? s[n] != '-'
            : !strchr("0123456789abcdef", s[n]))
      return 0;
  return 1;
}
static void emit(const char *path, int kind, const unsigned char hash[20],
                 const void *data, unsigned size) {
  char id[41];
  hex(hash, id);
  if (printf("%d %s %u %s\n", kind, id, size, path) < 0 ||
      (size && fwrite(data, 1, size, stdout) != size))
    exit(10);
}
static int tree(ext2_ino_t ino, int depth, const char *path,
                unsigned char out[20]);
static int entry(ext2_ino_t dir, int type, struct ext2_dir_entry *d, int off,
                 int bs, char *buf, void *ctx) {
  (void)dir;
  (void)off;
  (void)bs;
  (void)buf;
  if (type == DIRENT_DOT_FILE || type == DIRENT_DOT_DOT_FILE || !d->inode)
    return 0;
  struct list *l = ctx;
  if (l->depth >= 4 || files + syms + dirs >= 131072)
    exit(3);
  int n = ext2fs_dirent_name_len(d);
  if (n < 1 || n > 255)
    exit(3);
  struct ext2_inode i;
  ck(ext2fs_read_inode(fs, d->inode, &i));
  char name[256], path[256];
  memcpy(name, d->name, n);
  name[n] = 0;
  if ((int)strlen(name) != n ||
      (!uuid_name(name) && strcmp(name, ".keep") && strcmp(name, "node.md") &&
       strcmp(name, "children") && strcmp(name, "children.order")))
    exit(3);
  if (snprintf(path, sizeof path, "%s/%s", l->path, name) >= (int)sizeof path)
    exit(3);
  struct item item = {0};
  int isdir = S_ISDIR(i.i_mode);
  if (isdir && !uuid_name(name) && strcmp(name, "children"))
    exit(3);
  if (isdir) {
    dirs++;
    if (!tree(d->inode, l->depth + 1, path, item.hash))
      return 0;
    strcpy(item.mode, "40000");
    emit(path, 0, item.hash, NULL, 0);
  } else {
    unsigned len = i.i_size;
    if (len > 1048576 || i.i_size_high)
      exit(4);
    SHA_CTX c;
    header(&c, "blob", len);
    bytes += len;
    if (bytes > 64ULL * 1024 * 1024)
      exit(4);
    unsigned char *data = malloc(len ? len : 1);
    if (!data)
      exit(8);
    if (S_ISLNK(i.i_mode)) {
      syms++;
      strcpy(item.mode, "120000");
    } else if (S_ISREG(i.i_mode)) {
      files++;
      if ((i.i_mode & 0100) || i.i_links_count != 1)
        exit(5);
      strcpy(item.mode, "100644");
    } else
      exit(5);
    if (S_ISLNK(i.i_mode) && ext2fs_is_fast_symlink(&i)) {
      if (len > sizeof i.i_block)
        exit(6);
      memcpy(data, i.i_block, len);
    } else if (len) {
      ext2_file_t f;
      ck(ext2fs_file_open2(fs, d->inode, &i, 0, &f));
      unsigned left = len;
      while (left) {
        unsigned got;
        ck(ext2fs_file_read(f, data + len - left, left, &got));
        if (!got)
          exit(7);
        left -= got;
      }
      ck(ext2fs_file_close(f));
    }
    SHA1_Update(&c, data, len);
    SHA1_Final(item.hash, &c);
    emit(path, S_ISLNK(i.i_mode) ? 2 : 1, item.hash, data, len);
    free(data);
  }
  item.key = malloc(n + 2);
  if (!item.key)
    exit(8);
  memcpy(item.key, d->name, n);
  item.key[n] = isdir ? '/' : 0;
  item.key[n + 1] = 0;
  if (l->n == l->cap) {
    l->cap = l->cap ? 2 * l->cap : 8;
    l->v = realloc(l->v, l->cap * sizeof *l->v);
    if (!l->v)
      exit(8);
  }
  l->v[l->n++] = item;
  return 0;
}
static int tree(ext2_ino_t ino, int depth, const char *path,
                unsigned char out[20]) {
  struct list l = {.path = path, .depth = depth};
  ck(ext2fs_dir_iterate2(fs, ino, 0, 0, entry, &l));
  if (!l.n)
    return 0;
  qsort(l.v, l.n, sizeof *l.v, by_name);
  size_t sz = 0;
  for (size_t j = 0; j < l.n; j++) {
    struct item *t = l.v + j;
    size_t len = strlen(t->key);
    if (t->key[len - 1] == '/')
      t->key[len - 1] = 0;
    sz += strlen(t->mode) + 1 + strlen(t->key) + 1 + 20;
  }
  SHA_CTX c;
  header(&c, "tree", sz);
  for (size_t j = 0; j < l.n; j++) {
    struct item *t = l.v + j;
    SHA1_Update(&c, t->mode, strlen(t->mode));
    SHA1_Update(&c, " ", 1);
    SHA1_Update(&c, t->key, strlen(t->key) + 1);
    SHA1_Update(&c, t->hash, 20);
    free(t->key);
  }
  free(l.v);
  SHA1_Final(out, &c);
  return 1;
}
int main(int argc, char **argv) {
  struct stat root, dev;
  if (argc != 3 || getuid() != geteuid() || getgid() != getegid() ||
      stat("/workspace/knowledge", &root) || stat("/dev/vdb", &dev) ||
      !S_ISBLK(dev.st_mode) || dev.st_rdev != root.st_dev)
    return 1;
  struct rlimit mem = {128UL << 20, 128UL << 20};
  if (setrlimit(RLIMIT_AS, &mem))
    return 1;
  alarm(5);
  ck(ext2fs_open("/dev/vdb", EXT2_FLAG_64BITS, 0, 0, unix_io_manager, &fs));
  const char *p[] = {"knowledge/nodes", "knowledge/trees"};
  for (int k = 0; k < 2; k++) {
    ext2_ino_t ino;
    unsigned char h[20];
    const char *layout[] = {".projection-volume/data/knowledge/nodes", ".projection-volume/data/knowledge/trees"};
    ck(ext2fs_namei(fs, 2, 2, layout[k], &ino));
    if (!tree(ino, 0, p[k] + 10, h))
      exit(9);
    char id[41];
    hex(h, id);
    if (strcmp(id, argv[k + 1]))
      exit(9);
    emit(p[k] + 10, 0, h, NULL, 0);
  }
  ck(ext2fs_close(fs));
  return fflush(stdout) != 0;
}
