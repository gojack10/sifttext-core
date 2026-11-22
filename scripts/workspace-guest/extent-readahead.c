/* Diagnostic: read ahead the extent span of a large, fragmented ext4 directory on the disposable guest device. No fixture offsets are supplied. */
#define _GNU_SOURCE
#include <fcntl.h>
#include <unistd.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <linux/fiemap.h>
#include <linux/fs.h>
int main(int argc,char **argv){
 if(argc!=3)return 2;
 int dir=open(argv[1],O_DIRECTORY|O_RDONLY),dev=open(argv[2],O_RDONLY);
 struct stat d,b; if(dir<0||dev<0||fstat(dir,&d)||fstat(dev,&b)||!S_ISBLK(b.st_mode)||d.st_dev!=b.st_rdev)return 3;
 unsigned n=1024; struct fiemap *m=calloc(1,sizeof(*m)+n*sizeof(struct fiemap_extent)); if(!m)return 4;
 m->fm_length=~0ULL; m->fm_extent_count=n;
 if(ioctl(dir,FS_IOC_FIEMAP,m)<0||m->fm_mapped_extents<2)return 5;
 unsigned long long lo=~0ULL,hi=0; for(unsigned i=0;i<m->fm_mapped_extents;i++) {struct fiemap_extent e=m->fm_extents[i];if(e.fe_flags & (FIEMAP_EXTENT_UNKNOWN|FIEMAP_EXTENT_DELALLOC))return 6; if(e.fe_physical<lo)lo=e.fe_physical; if(e.fe_physical+e.fe_length>hi)hi=e.fe_physical+e.fe_length;}
 if(hi-lo>224ULL*1024*1024)return 7;
 const size_t window=512*1024; for(unsigned long long p=lo;p<hi;p+=window) {size_t length=hi-p<window?hi-p:window; if(readahead(dev,p,length)<0)return 8;}
 fprintf(stderr,"index-phase readahead range_bytes=%llu directory_extents=%u\n",hi-lo,m->fm_mapped_extents);return 0;
}
