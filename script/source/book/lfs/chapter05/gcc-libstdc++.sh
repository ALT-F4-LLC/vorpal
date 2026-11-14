# Linux From Scratch 12.4-systemd: chapter05/gcc-libstdc++
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter05/gcc-libstdc++.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

mkdir -v build
cd       build

../libstdc++-v3/configure      \
    --host=$LFS_TGT            \
    --build=$(../config.guess) \
    --prefix=/usr              \
    --disable-multilib         \
    --disable-nls              \
    --disable-libstdcxx-pch    \
    --with-gxx-include-dir=/tools/$LFS_TGT/include/c++/15.2.0

make

make DESTDIR=$LFS install

rm -v $LFS/usr/lib/lib{stdc++{,exp,fs},supc++}.la
