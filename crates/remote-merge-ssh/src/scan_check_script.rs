//! 走査の中でディレクトリを確かめるスクリプト（docs/decision/records/2026-10-12-foundation-2.md の
//! C14）

/// find が出会ったディレクトリごとに、読めるか・検索できるかを確かめるスクリプト（変数 c）。結果は
/// find の一覧と混ざらないよう標準エラーに、NUL で区切った目印付きの記録で出す。
/// find の診断から推し量らない（診断を出さない find もある）。もうないかディレクトリでなくなって
/// いたら "G" を出す。読めて検索できないディレクトリ（`every_name` なら読めるディレクトリすべて）は、
/// その場で glob で子の名前を集める。glob が当たらずにパターンの文字列が返ったときは、その文字列を
/// 名前に持つ子があるかを確かめて見分ける。検索できるディレクトリでは test で、検索できない
/// ディレクトリでは一文字を [] で囲んだ glob で確かめる。シェルの関数は使わない（docs/remote-tools.md）
pub(crate) fn check_script(every_name: bool) -> String {
    let names_when = if every_name {
        "[ $r = 1 ]"
    } else {
        "[ $r = 1 ] && [ $x = 0 ]"
    };
    format!(
        concat!(
            "c='exec >&2\n",
            "for d do printf \"\\000@D\\000%s\\000\" \"$d\"\n",
            " if [ ! -d \"$d\" ]; then printf \"G\\000\"; else\n",
            " if [ -r \"$d\" ]; then r=1; else r=0; fi; if [ -x \"$d\" ]; then x=1; else x=0; fi\n",
            " printf \"%s%s\\000\" $r $x\n",
            " if {names_when}; then for p in \"$d\"/* \"$d\"/.[!.]* \"$d\"/..?*; do\n",
            "  case \"$p\" in\n",
            "   \"$d/*\") g=\"[*]\";; \"$d/.[!.]*\") g=\".[[]!.][*]\";; \"$d/..?*\") g=\"..[?][*]\";; *) g=;;\n",
            "  esac\n",
            "  if [ -z \"$g\" ]; then :\n",
            "  elif [ $x = 1 ]; then [ -e \"$p\" ] || [ -h \"$p\" ] || continue\n",
            "  else k=0; for q in \"$d\"/$g; do [ \"$q\" != \"$d/$g\" ] && k=1; done; [ $k = 1 ] || continue; fi\n",
            "  printf \"N%s\\000\" \"${{p##*/}}\"\n",
            " done; fi; fi; printf \"@E\\000\"\n",
            "done'\n",
        ),
        names_when = names_when
    )
}
