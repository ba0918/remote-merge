#!/bin/bash
# VM の中と手元で同じ手順を走らせるための荷物を集める: payload.sh <作業ディレクトリ> <置き場所>
# 作業ディレクトリには reproduce.sh が作った target/（musl の製品とテスト）と req.bin・req2.bin がある
set -e
D=$(cd "$(dirname "$0")" && pwd); S=$1; P=$2
T=$S/target/x86_64-unknown-linux-musl/release
rm -rf "$P"; mkdir -p "$P/tests"
cp "$T/remote-merge" "$S/req.bin" "$S/req2.bin" "$D/run.sh" "$D/fixtures.sh" "$P/"
for t in "$T"/deps/remote_merge_agent-* "$T"/deps/remote_merge_core-* "$T"/deps/remote_merge_protocol-*; do
  case $t in *.d|*.rlib|*.rmeta) continue;; esac
  if [ -x "$t" ]; then cp "$t" "$P/tests/"; fi
done
