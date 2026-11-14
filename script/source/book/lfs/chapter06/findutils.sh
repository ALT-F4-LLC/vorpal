# Linux From Scratch 12.4-systemd: chapter06/findutils
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter06/findutils.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

./configure --prefix=/usr                   \
            --localstatedir=/var/lib/locate \
            --host=$LFS_TGT                 \
            --build=$(build-aux/config.guess)

make

make DESTDIR=$LFS install
