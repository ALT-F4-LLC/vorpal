# Linux From Scratch 12.4-systemd: chapter05/binutils-pass1
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter05/binutils-pass1.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

mkdir -v build
cd       build

../configure --prefix=$LFS/tools \
             --with-sysroot=$LFS \
             --target=$LFS_TGT   \
             --disable-nls       \
             --enable-gprofng=no \
             --disable-werror    \
             --enable-new-dtags  \
             --enable-default-hash-style=gnu

make

make install
