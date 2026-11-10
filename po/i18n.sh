#!/bin/sh
# Translation upkeep. See docs/translating.md.
#   po/i18n.sh update           extract strings into myne.pot and merge them into every .po
#   po/i18n.sh install [DIR]    compile the .po files into DIR (default ~/.local/share/locale)
set -eu
cd "$(dirname "$0")/.."

case "${1:-}" in
update)
	common="--package-name=Myne --msgid-bugs-address=https://github.com/calmecac-dev/myne/issues --from-code=UTF-8 --add-comments=TRANSLATORS: --sort-by-file"
	xgettext $common -L Rust -k -kgettext -kngettext:1,2 -o po/myne.pot $(grep '\.rs$' po/POTFILES)
	# Blueprint's `_("...")` is read as C, which has the same syntax for it.
	xgettext $common -L C -k -k_ -j -o po/myne.pot $(grep '\.blp$' po/POTFILES)
	for lang in $(cat po/LINGUAS); do
		msgmerge -q --no-fuzzy-matching --update --backup=none "po/$lang.po" po/myne.pot
	done
	;;
install)
	dir="${2:-$HOME/.local/share/locale}"
	for lang in $(cat po/LINGUAS); do
		mkdir -p "$dir/$lang/LC_MESSAGES"
		msgfmt --check -o "$dir/$lang/LC_MESSAGES/myne.mo" "po/$lang.po"
	done
	;;
*)
	sed -n '2,4p' "$0" >&2
	exit 1
	;;
esac
