#!/bin/bash
# 手元で実行: CentOS 5 のカーネルで initramfs を qemu で起動し、出力を console.log に取る
# 使い方: [EXT3=1] [NOKVM=1] boot.sh <作業ディレクトリ>
# EXT3=1 なら ext3.img を IDE の hda として付ける。/dev/kvm がなければ NOKVM=1（遅い）
set -e
S=$1
KVM="--device /dev/kvm"; ACC="-enable-kvm"
if [ -n "$NOKVM" ]; then KVM=""; ACC=""; fi
DRIVE=""
if [ -n "$EXT3" ]; then
  # 毎回まっさらな ext3（CentOS 5 の既定に近い 128 バイトの inode。秒の精度の更新時刻）を作る
  rm -f "$S/ext3.img"; truncate -s 256M "$S/ext3.img"
  mke2fs -q -F -t ext3 -I 128 \
    -O ^metadata_csum,^64bit,^huge_file,^dir_nlink,^extra_isize,^orphan_file,^metadata_csum_seed \
    -E root_owner=0:0 "$S/ext3.img"
  DRIVE="-drive file=/s/ext3.img,if=ide,format=raw,index=0"
fi
set +e
docker run --rm $KVM -v "$S:/s" k2618-qemu timeout 3000 qemu-system-x86_64 $ACC -m 2048 -smp 2 \
  -kernel /s/kern/boot/vmlinuz-2.6.18-419.el5 -initrd /s/initramfs.cpio.gz $DRIVE \
  -append "console=ttyS0 rdinit=/init panic=-1 ${KAPPEND:-}" -nographic -no-reboot \
  > "$S/console.log" 2>&1
echo "qemu exit=$?"
