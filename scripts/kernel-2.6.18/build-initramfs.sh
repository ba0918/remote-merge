#!/bin/bash
# 手元で実行: 荷物と BusyBox と init から initramfs を作る: [EXT3=1] build-initramfs.sh <作業ディレクトリ>
# EXT3=1 なら ext3 のモジュールも入れ、VM の中で IDE の hda を ext3 として使う
set -e
D=$(cd "$(dirname "$0")" && pwd); S=$1
ST=$S/staging
rm -rf "$ST"; mkdir -p "$ST/bin"
"$D/payload.sh" "$S" "$ST/payload"
cp "$S/bb/busybox" "$ST/bin/busybox"
cp "$D/init" "$ST/init"; chmod 755 "$ST/init"
if [ -n "$EXT3" ]; then
  mkdir -p "$ST/payload/kmod"
  cp $(find "$S/kmod" -name jbd.ko -o -name ext3.ko) "$ST/payload/kmod/"
  touch "$ST/payload/use_ext3"
fi
cp "$D/mkinitramfs.sh" "$S/mkinitramfs.sh"
docker run --rm -v "$S:/s" k2618-qemu sh -c \
  "sh /s/mkinitramfs.sh /s/staging /s/initramfs.cpio.gz && chown $(id -u):$(id -g) /s/initramfs.cpio.gz && rm -rf /s/staging /s/mkinitramfs.sh"
