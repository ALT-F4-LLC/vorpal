# Beyond Linux From Scratch 12.4-systemd: general/libunistring
# https://www.linuxfromscratch.org/blfs/view/12.4-systemd/general/libunistring.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

./configure --prefix=/usr    \
            --disable-static \
            --docdir=/usr/share/doc/libunistring-1.3 &&
make

# (as root)
make install
