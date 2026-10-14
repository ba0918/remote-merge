#!/bin/bash
# console.log から VM の出力を取り出す: extract.sh <console.log> <out dir>
# 割り込んだカーネルの文言（serial8250: too much work for irqN）を除いてから戻す
set -e
mkdir -p $2
sed -n '/=== K2618 B64 BEGIN/,/=== K2618 B64 END/p' $1 | tr -d '\r' | grep -av '=== K2618' \
  | sed 's/serial8250: too much work for irq[0-9]*//g' | tr -d '\n' | base64 -d | tar xzf - -C $2
