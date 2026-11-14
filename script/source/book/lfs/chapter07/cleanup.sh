# Linux From Scratch 12.4-systemd: chapter07/cleanup
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter07/cleanup.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

rm -rf /usr/share/{info,man,doc}/*

find /usr/{lib,libexec} -name \*.la -delete

rm -rf /tools

exit

mountpoint -q $LFS/dev/shm && umount $LFS/dev/shm
umount $LFS/dev/pts
umount $LFS/{sys,proc,run,dev}

cd $LFS
tar -cJpf $HOME/lfs-temp-tools-12.4-systemd.tar.xz .
