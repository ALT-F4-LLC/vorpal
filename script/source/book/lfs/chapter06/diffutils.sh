# Linux From Scratch 12.4-systemd: chapter06/diffutils
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter06/diffutils.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

./configure --prefix=/usr   \
            --host=$LFS_TGT \
            gl_cv_func_strcasecmp_works=y \
            --build=$(./build-aux/config.guess)

make

make DESTDIR=$LFS install
