# Linux From Scratch 12.4-systemd: chapter06/tar
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter06/tar.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

./configure --prefix=/usr   \
            --host=$LFS_TGT \
            --build=$(build-aux/config.guess)

make

make DESTDIR=$LFS install
