# Linux From Scratch 12.4-systemd: chapter06/binutils-pass2
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter06/binutils-pass2.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

sed '6031s/$add_dir//' -i ltmain.sh

mkdir -v build
cd       build

../configure                   \
    --prefix=/usr              \
    --build=$(../config.guess) \
    --host=$LFS_TGT            \
    --disable-nls              \
    --enable-shared            \
    --enable-gprofng=no        \
    --disable-werror           \
    --enable-64-bit-bfd        \
    --enable-new-dtags         \
    --enable-default-hash-style=gnu

make

make DESTDIR=$LFS install

rm -v $LFS/usr/lib/lib{bfd,ctf,ctf-nobfd,opcodes,sframe}.{a,la}
