# run.sh が読み込む、エージェントに問う root_dir の中身（mk1 は req.bin、mk2 は req2.bin 用）
mk1() {
  R=$1
  rm -rf $R; mkdir -p $R/sub
  echo hello > $R/a.txt
  printf 'b\n' > $R/sub/b.txt
  ln -s a.txt $R/link_to_a
  head -c 1572864 /dev/zero | tr '\0' 'x' > $R/big.bin
  for f in $R/a.txt $R/sub/b.txt $R/big.bin $R/sub $R; do touch -d '2020-01-02 03:04:05' $f; done
}
mk2() {
  R=$1
  rm -rf $R; mkdir -p $R/many $R/cyc $R/noperm $R/realdir
  seq 1 3000 | sed "s|^|$R/many/f|" | xargs touch
  echo x > $R/realdir/in.txt
  ln -s realdir $R/linkdir
  ln -s .. $R/cyc/up
  printf 'old content that is long\n' > $R/existing.txt
  # リンク先を辿れない symlink（REQ-scan-018。ループは cyc/up）
  ln -s existing.txt/inside $R/through_file
  # 名前を読めない symlink（REQ-scan-012・018）
  ln -s realdir "$R/$(printf 'link\377name')"
  mkfifo $R/fifo
  echo secret > $R/noperm/s.txt
  chmod 000 $R/noperm
  touch "$R/$(printf 'bad\377name')"
}
