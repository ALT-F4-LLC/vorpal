# Linux From Scratch 12.4-systemd: chapter06/bash
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter06/bash.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

./configure --prefix=/usr                      \
            --build=$(sh support/config.guess) \
            --host=$LFS_TGT                    \
            --without-bash-malloc

make

make DESTDIR=$LFS install

ln -sv bash $LFS/bin/sh
