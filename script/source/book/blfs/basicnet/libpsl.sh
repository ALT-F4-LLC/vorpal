# Beyond Linux From Scratch 12.4-systemd: basicnet/libpsl
# https://www.linuxfromscratch.org/blfs/view/12.4-systemd/basicnet/libpsl.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

mkdir build &&
cd    build &&

meson setup --prefix=/usr --buildtype=release &&

ninja

# (as root)
ninja install
