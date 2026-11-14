# Linux From Scratch 12.4-systemd: chapter05/linux-headers
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter05/linux-headers.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

make mrproper

make headers
find usr/include -type f ! -name '*.h' -delete
cp -rv usr/include $LFS/usr
