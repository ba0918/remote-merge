#!/usr/bin/env bash
# 渡されたファイルの変異テストをメモリ上限つきで毎回新しく実行し、その結果を kotowari mutants で読む。
#
# 使い方: scripts/mutants.sh [--re <regex>]... <file> [<file>...]
#
# --re を付けると、変異の名前（cargo mutants --list の表示）がその正規表現に一致するものに絞って回す。
# 対象の関数だけを確かめたいときに、ファイルの他の関数の変異に実行時間を使わないためのもの。
# cargo-mutants 27.1.0 では構造体のフィールドを消す変異がこの絞り込みで除かれないため、結果に混ざることがある。
#
# cargo-mutants は systemd-run --user のサービスとして起動する。変異テストの実行中にメモリを
# 使い切って WSL ごと落ちたことがあるため、上限はサービスの cgroup にかける。
# 上限と並列数は次の環境変数で上書きできる。
#   MUTANTS_MEMORY_HIGH      MemoryHigh（既定 35%）
#   MUTANTS_MEMORY_MAX       MemoryMax（既定 40%）
#   MUTANTS_MEMORY_SWAP_MAX  MemorySwapMax（既定 0）
#   MUTANTS_JOBS             cargo-mutants の並列数（既定 2）
#
# サービスは CPUWeight=idle と Nice=19 で動かし、他の作業が CPU を使うときはそちらを優先させる
# （変異テストのビルドとテストが CPU を占め、他の作業が止まりかけたため）。
#
# メモリ上限で強制終了されたテストを cargo-mutants は変異の検知と数えてしまう。そのため
# OOMPolicy=stop でサービス全体を止め、失敗として 0 以外で終了し、途中までの結果は読まない。
set -euo pipefail

filter_args=()
while [ "$#" -gt 0 ] && [ "$1" = "--re" ]; do
    if [ "$#" -lt 2 ]; then
        echo "usage: scripts/mutants.sh [--re <regex>]... <file> [<file>...]" >&2
        exit 64
    fi
    filter_args+=(--re "$2")
    shift 2
done

if [ "$#" -eq 0 ]; then
    echo "usage: scripts/mutants.sh [--re <regex>]... <file> [<file>...]" >&2
    exit 64
fi

memory_high="${MUTANTS_MEMORY_HIGH:-35%}"
memory_max="${MUTANTS_MEMORY_MAX:-40%}"
memory_swap_max="${MUTANTS_MEMORY_SWAP_MAX:-0}"
jobs="${MUTANTS_JOBS:-2}"

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

# 前回の結果を読み違えないよう、実行のたびに出力先を空にする
output_dir="target/mutants-run"
outcomes="$output_dir/mutants.out/outcomes.json"
rm -rf "$output_dir"
mkdir -p "$output_dir"

file_args=()
for file in "$@"; do
    file_args+=(--file "$file")
done

unit="remote-merge-mutants-$$"
stop_unit() {
    systemctl --user stop "$unit" 2>/dev/null || true
}
trap 'stop_unit; exit 130' INT TERM

echo "mutants.sh: unit=$unit MemoryHigh=$memory_high MemoryMax=$memory_max MemorySwapMax=$memory_swap_max jobs=$jobs" >&2

# サービスはユーザーの systemd の環境で動くため、このシェルが使っている toolchain とツールの選択を引き継ぐ。
# systemd-run に渡した値はプロセスの引数とサービスの Environment に残るため、認証情報を含みうる
# 変数（MISE_GITHUB_TOKEN など）を渡さないよう、引き継ぐ変数は秘密を含まない名前だけに限定する。
# MISE_GLOBAL_CONFIG_FILE は、mise の shim（cargo-nextest など）が使うバージョンを決めるのに要る。
env_args=(--setenv=PATH="$PATH")
for name in RUSTUP_TOOLCHAIN RUSTUP_HOME CARGO_HOME MISE_GLOBAL_CONFIG_FILE; do
    if [ -n "${!name:-}" ]; then
        env_args+=(--setenv="$name=${!name}")
    fi
done

# 標準出力は kotowari mutants の結果だけにするため、cargo-mutants の出力は標準エラーへ流す
set +e
systemd-run --user --unit "$unit" --wait --pipe --quiet --same-dir \
    "${env_args[@]}" \
    -p MemoryHigh="$memory_high" \
    -p MemoryMax="$memory_max" \
    -p MemorySwapMax="$memory_swap_max" \
    -p OOMPolicy=stop \
    -p CPUWeight=idle \
    -p Nice=19 \
    cargo mutants \
    --jobs "$jobs" \
    --all-features \
    --test-tool nextest \
    --output "$output_dir" \
    "${file_args[@]}" \
    "${filter_args[@]}" >&2
status=$?
set -e
trap - INT TERM

# 失敗したサービスは Result を読めるよう残っているので、読んでから片付ける
result="$(systemctl --user show -P Result "$unit" 2>/dev/null || true)"
systemctl --user reset-failed "$unit" 2>/dev/null || true

# cargo-mutants は見逃しで 2、タイムアウトで 3 を返す。これらは実行の完了として扱い、判定は kotowari mutants に任せる
case "$status" in
    0 | 2 | 3) ;;
    *)
        echo "mutants.sh: cargo-mutants did not finish (exit=$status, result=${result:-unknown}); results are not read" >&2
        exit 1
        ;;
esac
if [ "$result" = "oom-kill" ]; then
    echo "mutants.sh: the run was stopped by the memory limit (result=oom-kill); results are not read" >&2
    exit 1
fi

if [ ! -f "$outcomes" ]; then
    echo "mutants.sh: $outcomes was not written; results are not read" >&2
    exit 1
fi

exec kotowari mutants --tool cargo-mutants --format text "$outcomes"
