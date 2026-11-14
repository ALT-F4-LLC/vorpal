# Linux From Scratch 12.4-systemd: chapter06/file
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter06/file.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

mkdir build
pushd build
  ../configure --disable-bzlib      \
               --disable-libseccomp \
               --disable-xzlib      \
               --disable-zlib
  make
popd

./configure --prefix=/usr --host=$LFS_TGT --build=$(./config.guess)

make FILE_COMPILE=$(pwd)/build/src/file

make DESTDIR=$LFS install

rm -v $LFS/usr/lib/libmagic.la
