#!/bin/sh
# docker の中で root として実行: mkinitramfs.sh <staging> <out.cpio.gz>
set -e
ST=$1; OUT=$2
cd $ST
mkdir -p dev proc sys tmp bin
[ -e dev/console ] || mknod dev/console c 5 1
[ -e dev/null ] || mknod -m 666 dev/null c 1 3
[ -e dev/zero ] || mknod -m 666 dev/zero c 1 5
[ -e dev/random ] || mknod -m 666 dev/random c 1 8
[ -e dev/urandom ] || mknod -m 666 dev/urandom c 1 9
[ -e dev/tty ] || mknod -m 666 dev/tty c 5 0
[ -e dev/ttyS0 ] || mknod dev/ttyS0 c 4 64
chmod 1777 tmp
find . | cpio -o -H newc 2>/dev/null | gzip -9 > $OUT
