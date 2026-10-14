#!/bin/bash
# 手元の答えと VM の答えを比べる: compare.sh <作業ディレクトリ> <手元の出力> <VM の出力>
# エージェントの答え（resp*.bin）を probe で読み、走査の一覧は一項目ずつの行にし（枠の切れ目は更新時刻の
# 長さで変わる）、環境で変わる値（更新時刻・ディレクトリの大きさ・更新時刻の桁・作業ディレクトリの
# 実パス）を伏せてから比べる。root で走らせた答え（resp2-root）は、手元を一般の利用者で走らせるので
# 比べない。テストの結果（test-*.txt）は exit=0 であることを確かめる
set -e
S=$1; HOST=$2; VM=$3; WORK=${WORK:-$S/hostwork}
PROBE=$S/target/release/probe
norm() {
  LINES=1 "$PROBE" decode "$1" | sed -E \
    -e "s#$WORK/#<work>/#g" -e 's#/(tmp|mnt)/w/#<work>/#g' \
    -e 's/mtime_secs: -?[0-9]+, mtime_nanos: [0-9]+/mtime=*/g' \
    -e 's/mtime_digits: [0-9]+/mtime_digits=*/g' \
    -e 's/kind: Directory, size: [0-9]+/kind: Directory, size=*/g'
}
status=0
for host in "$HOST"/resp*.bin; do
  name=$(basename "$host")
  if [ "$name" = resp2-root.bin ]; then continue; fi
  if ! diff <(norm "$host") <(norm "$VM/$name") > "$VM/$name.diff"; then
    echo "DIFFERS: $name (see $VM/$name.diff)"; status=1
  else
    echo "same: $name ($(norm "$host" | grep -c -E '^(RESP|TREE END)') answers)"
  fi
done
for result in "$VM"/test-*.txt; do
  if tail -1 "$result" | grep -qx 'exit=0'; then
    echo "pass: $(basename "$result")"
  else
    echo "FAILED: $(basename "$result")"; status=1
  fi
done
exit $status
