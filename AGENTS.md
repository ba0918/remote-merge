# Agent Instructions

## Core

- 示された目的に仕える。依頼された範囲を広げない。
- 確認済みのこと、推測したこと、未検証のことを区別する。
- 何かを変えたら、その変更に合った方法で検証する。
- 不可逆・破壊的・外部から見える操作は、承認なしに行わない。
- プロジェクト固有の指示がこれより具体的なら、そちらを適用する。

## Rule Routing

| When | Read |
|---|---|
| Always | ba0918-design, ba0918-placement, ba0918-readability, ba0918-secrets |
| commit | ba0918-commit |
| delegate | ba0918-delegation |
| design | ba0918-reuse |
| diff-review | ba0918-diff-review |
| implement | ba0918-tdd |
| release | ba0918-release |
| review | ba0918-verification |

各ルールはスキル名で参照する。当てはまるルールはすべて、それが規定する作業を始める前に読む。

## Project Context

このリポジトリが何か、ビルドとテストの方法、ここだけに適用される慣習は `PROJECT.md` にある。
変更を加える前に読む。
