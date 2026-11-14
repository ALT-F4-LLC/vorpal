# Linux From Scratch 12.4-systemd: chapter06/xz
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter06/xz.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

./configure --prefix=/usr                     \
            --host=$LFS_TGT                   \
            --build=$(build-aux/config.guess) \
            --disable-static                  \
            --docdir=/usr/share/doc/xz-5.8.1

make

make DESTDIR=$LFS install

rm -v $LFS/usr/lib/liblzma.la
