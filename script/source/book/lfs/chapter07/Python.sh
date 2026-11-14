# Linux From Scratch 12.4-systemd: chapter07/Python
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter07/Python.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

./configure --prefix=/usr       \
            --enable-shared     \
            --without-ensurepip \
            --without-static-libpython

make

make install
