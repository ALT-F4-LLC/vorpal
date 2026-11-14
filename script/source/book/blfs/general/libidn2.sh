# Beyond Linux From Scratch 12.4-systemd: general/libidn2
# https://www.linuxfromscratch.org/blfs/view/12.4-systemd/general/libidn2.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

./configure --prefix=/usr --disable-static &&
make

# (as root)
make install
