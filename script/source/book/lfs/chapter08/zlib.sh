# Linux From Scratch 12.4-systemd: chapter08/zlib
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter08/zlib.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

./configure --prefix=/usr

make

make check

make install

rm -fv /usr/lib/libz.a
