#!/bin/sh
# 使い方: run.sh <payload dir> <work dir> <out dir>（payload は payload.sh で作る）
# VM の中（busybox sh、root）と手元（一般の利用者）の両方で同じ手順を走らせる。
# VM の中では INVM=1 で、進み具合を console に出し、利用者 rm (uid 1000) でも走らせる
P=$1; W=$2; O=$3
B=$P/remote-merge
umask 022
[ -n "$INVM" ] && exec 2>/dev/console
mkdir -p $O $W
chmod 755 $W $O
step() { echo "STEP $(date +%s) ent=$(cat /proc/sys/kernel/random/entropy_avail): $*" >&2; }
# 利用者 rm として走らせる（手元では root でないのでそのまま）
asuser() { if [ "$(id -u)" = 0 ]; then chpst -u rm:rm "$@"; else "$@"; fi; }

uname -a > $O/uname.txt 2>&1
id > $O/id.txt
step version
timeout 120 $B --version > $O/version.txt 2>&1; echo "exit=$?" >> $O/version.txt
timeout 120 $B --help > $O/help.txt 2>&1; echo "exit=$?" >> $O/help.txt
timeout 120 $B agent --help > $O/agent-help.txt 2>&1; echo "exit=$?" >> $O/agent-help.txt

. $P/fixtures.sh
# agent <名前> <req> <root> [追加の引数...]: 標準入力をファイルから流す
agent() {
  n=$1; req=$2; r=$3; shift 3
  step "agent $n"
  "$@" timeout 120 $B agent --root $r < $req > $O/$n.bin 2> $O/$n.err
  echo "exit=$?" > $O/$n.exit; cat $O/$n.exit >&2
  (cd $r 2>/dev/null && ls -lnaR . ) > $O/$n.after.txt 2>&1
}

mk1 $W/r1; agent resp $P/req.bin $W/r1
step "agent pipe"
mk1 $W/r1p
cat $P/req.bin | timeout 120 $B agent --root $W/r1p 2> $O/resp-pipe.err | cat > $O/resp-pipe.bin
echo "exit=$?" > $O/resp-pipe.exit
agent resp-noroot $P/req.bin $W/no-such-dir
step "agent eof"
timeout 120 $B agent --root $W/r1 < /dev/null > $O/resp-eof.bin 2> $O/resp-eof.err; echo "exit=$?" > $O/resp-eof.exit

# 利用者 rm として
mkdir -p $W/u; chmod 777 $W/u
step "user fixtures"
asuser sh -c ". $P/fixtures.sh; mk1 $W/u/r1; mk2 $W/u/r2"
agent resp-user $P/req.bin $W/u/r1 asuser
agent resp2-user $P/req2.bin $W/u/r2 asuser

# root として req2（root のときは既定の利用者 1000 で所有者を付ける）
mk2 $W/r2
step "agent resp2-root"
timeout 120 $B agent --root $W/r2 --default-uid 1000 --default-gid 1000 < $P/req2.bin > $O/resp2-root.bin 2> $O/resp2-root.err
echo "exit=$?" > $O/resp2-root.exit; cat $O/resp2-root.exit >&2
(cd $W/r2 && ls -lnaR . ) > $O/resp2-root.after.txt 2>&1

step "random check"
(timeout 5 head -c 1 /dev/random > /dev/null; echo "dev_random_read_exit=$?") > $O/random.txt 2>&1
cat /proc/sys/kernel/random/entropy_avail >> $O/random.txt

# agent のテストのうち bridge_tests は、手元の側の SSH の中継を tokio の実行環境の上で確かめる。2.6.18 では
# 実行環境を作れず（ENOSYS）、手元の側はサーバの上では動かないので、ここでは走らせない
CLIENT_SIDE=agent::ssh_transport::bridge_tests::
for t in $P/tests/*; do
  n=$(basename $t)
  mkdir -p $W/t-$n
  case $n in
    remote_merge_core-*)
      step "test $n (skip proptest)"
      (cd $W/t-$n && HOME=$W/t-$n TMPDIR=$W/t-$n timeout 300 $t --skip an_entry_never_joins_back_to_the_root_dir_itself) > $O/test-$n.txt 2>&1
      echo "exit=$?" >> $O/test-$n.txt; tail -3 $O/test-$n.txt >&2
      step "test $n (proptest seeded)"
      (cd $W/t-$n && HOME=$W/t-$n TMPDIR=$W/t-$n PROPTEST_RNG_SEED=1 timeout 300 $t an_entry_never_joins_back_to_the_root_dir_itself) > $O/test-$n-proptest.txt 2>&1
      echo "exit=$?" >> $O/test-$n-proptest.txt; tail -3 $O/test-$n-proptest.txt >&2
      ;;
    remote_merge_agent-*)
      step "test $n (skip $CLIENT_SIDE)"
      (cd $W/t-$n && HOME=$W/t-$n TMPDIR=$W/t-$n timeout 300 $t --skip $CLIENT_SIDE) > $O/test-$n.txt 2>&1
      echo "exit=$?" >> $O/test-$n.txt; tail -3 $O/test-$n.txt >&2
      ;;
    *)
      step "test $n"
      (cd $W/t-$n && HOME=$W/t-$n TMPDIR=$W/t-$n timeout 300 $t) > $O/test-$n.txt 2>&1
      echo "exit=$?" >> $O/test-$n.txt; tail -3 $O/test-$n.txt >&2
      ;;
  esac
done
# 利用者 rm としてもテストを走らせる（root では権限の検査が素通りするため）
for t in $P/tests/remote_merge_core-* $P/tests/remote_merge_agent-*; do
  n=$(basename $t)
  # 祖先が root の 755 になるよう $W の直下に作り、rm に渡す
  rm -rf $W/ut-$n; mkdir -p $W/ut-$n; [ "$(id -u)" = 0 ] && chown 1000:1000 $W/ut-$n
  step "test-user $n"
  (cd $W/ut-$n && asuser env HOME=$W/ut-$n TMPDIR=$W/ut-$n timeout 300 $t --skip an_entry_never_joins_back_to_the_root_dir_itself --skip $CLIENT_SIDE) > $O/test-user-$n.txt 2>&1
  echo "exit=$?" >> $O/test-user-$n.txt; tail -3 $O/test-user-$n.txt >&2
done
step done
echo done > $O/finished
