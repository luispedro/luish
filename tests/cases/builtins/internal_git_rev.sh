# `__luish_internal print-git-rev` and `print-git-rev-short` print the git
# revision luish was built from: a commit's hash (full or abbreviated), with
# `-dirty` if the sources differed from it, or `unknown` outside a checkout.
rev=$(__luish_internal print-git-rev)
short=$(__luish_internal print-git-rev-short)
case $rev in
unknown) [ "$short" = unknown ] && echo ok ;;
*)
	hash=${rev%-dirty} shash=${short%-dirty}
	case $hash in *[!0-9a-f]*) echo "bad rev: $rev" ;; esac
	[ ${#hash} -eq 40 ] || [ ${#hash} -eq 64 ] || echo "bad length: $rev"
	[ "${rev#"$hash"}" = "${short#"$shash"}" ] || echo "dirty differs: $rev $short"
	case $hash in "$shash"?*) echo ok ;; *) echo "not a prefix: $rev $short" ;; esac
	;;
esac
__luish_internal print-git-rev x 2>&1; echo "status $?"
__luish_internal print-git-rev-short x 2>&1; echo "status $?"
