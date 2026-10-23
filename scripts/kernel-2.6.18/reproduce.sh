#!/bin/bash
# CentOS 5 のカーネル（2.6.18-419.el5、x86_64）を qemu で起動し、今の版の musl の remote-merge を
# エージェントとして起動・走査・読み書きさせ、手元と同じ答えを返すことと、単体テスト（手元の側だけで
# 動くものは除く。run.sh）が通ることを確かめる。エージェントと core のテストは root と一般の利用者で、
# protocol のテストは root で走らせる（docs/decision/records/2026-10-12-foundation.md の A12、
# 2026-10-12-foundation-5.md の F27）。tmpfs と ext3 の二通りで走らせる。
#
# 使い方: reproduce.sh <作業ディレクトリ>（リポジトリの外に置く。リポジトリには何も書かない）
# 作業ディレクトリに前もって置くもの:
#   dl/kernel-2.6.18-419.el5.x86_64.rpm（sha256 は下で確かめる。CentOS 5 の vault にある）
#   bb/busybox（静的にリンクした x86_64 の BusyBox。一般の利用者で走らせる run.sh の手順が chpst を
#   使うので、chpst を含むもの。確かめたのは v1.37.0 を defconfig でビルドしたもの。Alpine の
#   busybox-static には chpst がない）
# 要るもの: docker、rustup の x86_64-unknown-linux-musl、mke2fs、/dev/kvm（なければ NOKVM=1）
set -e
D=$(cd "$(dirname "$0")" && pwd); R=$(cd "$D/../.." && pwd)
S=$(mkdir -p "$1" && cd "$1" && pwd)
M="systemd-run --user --scope -q -p MemoryMax=8G --"
RPM=$S/dl/kernel-2.6.18-419.el5.x86_64.rpm
echo "6c4558babe0b37022c1999231ad64116767001ef414bd8b65d66918a56dcaed1  $RPM" | sha256sum -c -
docker build -q -t k2618-qemu "$D"
# カーネルと ext3 のモジュールを取り出す
rm -rf "$S/kern" "$S/kmod"; mkdir -p "$S/kern" "$S/kmod"
docker run --rm -v "$S:/s" k2618-qemu sh -c "set -e
  cd /s/kern && rpm2cpio /s/dl/$(basename "$RPM") | cpio -idm './boot/*'
  cd /s/kmod && rpm2cpio /s/dl/$(basename "$RPM") | cpio -idm '*/jbd.ko' '*/ext3.ko'
  chown -R $(id -u):$(id -g) /s/kern /s/kmod"
# 製品・テスト・要求を作る道具を作る
cd "$R"
CARGO_TARGET_DIR=$S/target $M cargo build --release --target x86_64-unknown-linux-musl -p remote-merge
CARGO_TARGET_DIR=$S/target $M cargo test --release --target x86_64-unknown-linux-musl \
  -p remote-merge-agent -p remote-merge-core -p remote-merge-protocol --no-run
CARGO_TARGET_DIR=$S/target $M cargo build --release --manifest-path "$D/probe/Cargo.toml"
"$S/target/release/probe" gen > "$S/req.bin"
"$S/target/release/probe" gen2 > "$S/req2.bin"
# 手元で同じ手順を走らせる
"$D/payload.sh" "$S" "$S/hostpayload"
# 前の回が権限を外したまま残したディレクトリも消せるよう、先に権限を戻す
if [ -d "$S/hostwork" ]; then chmod -R u+rwX "$S/hostwork"; fi
rm -rf "$S/hostwork" "$S/hostout"
sh "$S/hostpayload/run.sh" "$S/hostpayload" "$S/hostwork" "$S/hostout"
# VM で tmpfs と ext3 の二通りを走らせ、手元と比べる
for fs in tmpfs ext3; do
  if [ $fs = ext3 ]; then export EXT3=1; fi
  "$D/build-initramfs.sh" "$S"
  "$D/boot.sh" "$S"
  rm -rf "$S/vmout-$fs"; "$D/extract.sh" "$S/console.log" "$S/vmout-$fs"
  "$D/compare.sh" "$S" "$S/hostout" "$S/vmout-$fs/out"
done
