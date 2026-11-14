# Linux From Scratch 12.4-systemd: chapter08/binutils
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter08/binutils.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

mkdir -v build
cd       build

../configure --prefix=/usr       \
             --sysconfdir=/etc   \
             --enable-ld=default \
             --enable-plugins    \
             --enable-shared     \
             --disable-werror    \
             --enable-64-bit-bfd \
             --enable-new-dtags  \
             --with-system-zlib  \
             --enable-default-hash-style=gnu

make tooldir=/usr

make -k check

grep '^FAIL:' $(find -name '*.log')

make tooldir=/usr install

rm -rfv /usr/lib/lib{bfd,ctf,ctf-nobfd,gprofng,opcodes,sframe}.a \
        /usr/share/doc/gprofng/
