# Linux From Scratch 12.4-systemd: chapter04/creatingminlayout
# https://www.linuxfromscratch.org/lfs/view/12.4-systemd/chapter04/creatingminlayout.html
# Commands extracted under the book's MIT License; see script/source/book/LICENSE.

mkdir -pv $LFS/{etc,var} $LFS/usr/{bin,lib,sbin}

for i in bin lib sbin; do
  ln -sv usr/$i $LFS/$i
done

case $(uname -m) in
  x86_64) mkdir -pv $LFS/lib64 ;;
esac

mkdir -pv $LFS/tools
