# Linux From Scratch 12.4-systemd: chapter07/kernfs
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter07/kernfs.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

mkdir -pv $LFS/{dev,proc,sys,run}

mount -v --bind /dev $LFS/dev

mount -vt devpts devpts -o gid=5,mode=0620 $LFS/dev/pts
mount -vt proc proc $LFS/proc
mount -vt sysfs sysfs $LFS/sys
mount -vt tmpfs tmpfs $LFS/run

if [ -h $LFS/dev/shm ]; then
  install -v -d -m 1777 $LFS$(realpath /dev/shm)
else
  mount -vt tmpfs -o nosuid,nodev tmpfs $LFS/dev/shm
fi
