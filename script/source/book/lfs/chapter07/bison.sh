# Linux From Scratch 12.4-systemd: chapter07/bison
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter07/bison.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

./configure --prefix=/usr \
            --docdir=/usr/share/doc/bison-3.8.2

make

make install
