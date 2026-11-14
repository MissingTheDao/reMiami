#!/usr/bin/env bash
# The scenarios that differ per UI style, run in every style (PC, PS2, Xbox), so a fault that only
# shows in one style can't sit unseen: the default suite (scripts/test.sh) runs in the PC style only.
#
# usage: scripts/styles.sh [--bless] [pc|ps2|xbox ...]          (default: all three)
#   Needs the console imports (REVC_IMPORT_DIR, an import made with `revc --import`; a style whose
#   import is missing is skipped and reported) and per-style references, made once with --bless
#   from a known-good build and kept outside the repository (they show the player's own discs):
#   REVC_STYLE_REFS (default rust/target/style-refs), one folder per style.
#   Per style:
#     saveload, savegame         against that style's own references
#     cutscenes                  the new game's opening (fails on textures no dictionary has)
#   PS2 style, besides: the hotel cutscene with the volumes up, recorded; fails when no PS2 stream
#   came from the import or the recording is nearly silent (RMS below 0.005; measured 0.005-0.16).
#   PS2 and Xbox styles, besides: menuwalk (no menu path reaches a PC menu page but the listed ones).
#   PC style, besides: formats and spots with the imports present and REVC_TEST_IMPORT_GUARD=1
#   (the PC style must read nothing of them but the allowlist).
#   REVC_TEST_BIN, REVC_GAME_DIR as for scripts/test.sh. Ends with a table; exit 1 if any failed.
set -uo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
BLESS=() STYLES=()
for a in "$@"; do
  case "$a" in
    --bless) BLESS=(--bless) ;;
    pc|ps2|xbox) STYLES+=("$a") ;;
    *) echo "unknown argument $a"; exit 2 ;;
  esac
done
(( ${#STYLES[@]} )) || STYLES=(pc ps2 xbox)
IMP=${REVC_IMPORT_DIR:?REVC_IMPORT_DIR: an import cache made with revc --import}
REFS=${REVC_STYLE_REFS:-$ROOT/rust/target/style-refs}
OUT=$ROOT/rust/target/test-output
CFG=$(mktemp -d); trap 'rm -rf "$CFG"' EXIT
declare -A RESULT
run() { # name, then env assignments and scenarios for test.sh
  local name=$1; shift
  if env "$@" "$ROOT/scripts/test.sh" "${BLESS[@]}" "${SCEN[@]}" > "$CFG/$name.out" 2>&1; then RESULT[$name]=PASS; else RESULT[$name]=FAIL; fi
  grep -E "^== |image diff" "$CFG/$name.out" | sed "s/^/  [$name] /"
}
for style in "${STYLES[@]}"; do
  case $style in pc) n=0 kind= ;; ps2) n=1 kind=ps2 ;; xbox) n=2 kind=xbox ;; esac
  if [[ -n "$kind" && ! -f "$IMP/$kind/IMPORTED.txt" ]]; then
    RESULT[$style]=SKIPPED; echo "== $style: no $kind import in $IMP, skipped"; continue
  fi
  # the test config with this style (the default config otherwise)
  sed "s/^UiStyle=.*/UiStyle=$n/" "$ROOT/tests/config/reVC.ini" > "$CFG/$style.ini"
  grep -q '^UiStyle=' "$CFG/$style.ini" || sed -i "/^\[Graphics\]/a UiStyle=$n" "$CFG/$style.ini"
  mkdir -p "$REFS/$style"
  echo "== $style"
  SCEN=(saveload savegame); run "$style-saves" REVC_IMPORT_DIR="$IMP" REVC_TEST_CONFIG="$CFG/$style.ini" REVC_TEST_REF_ROOT="$REFS/$style"
  [[ ${#BLESS[@]} -gt 0 ]] && continue
  SCEN=(cutscenes); run "$style-opening" REVC_IMPORT_DIR="$IMP" REVC_TEST_CONFIG="$CFG/$style.ini" REVC_TEST_CUT_NEW=1 REVC_TEST_CUT_MS=600000 REVC_TEST_TIMEOUT=1500
  grep -a -o "FAIL[^[]*" "$OUT/cutscenes.log" 2>/dev/null | head -5 | sed "s/^/  [$style-opening] /"
  if [[ $style != pc ]]; then
    # no menu path in a console style may show a PC menu page (testdrive.rs menuwalk)
    SCEN=(menuwalk); run "$style-menuwalk" REVC_IMPORT_DIR="$IMP" REVC_TEST_CONFIG="$CFG/$style.ini"
  fi
  if [[ $style == ps2 ]]; then
    # the PS2 streams must play (R5b): the hotel cutscene with the music and effects volumes up,
    # recorded; it fails below an energy floor or when no stream came from the import
    sed "s/^SfxVolume=.*/SfxVolume=100/; s/^MusicVolume=.*/MusicVolume=100/" "$CFG/$style.ini" > "$CFG/$style-loud.ini"
    SCEN=(cutscenes); run ps2-streams REVC_IMPORT_DIR="$IMP" REVC_TEST_CONFIG="$CFG/$style-loud.ini" REVC_AUDIO_PAN_LOG=1 REVC_TEST_WAV="$CFG/ps2-streams.wav" REVC_TEST_CUT_MS=60000 REVC_TEST_TIMEOUT=300
    if [[ ${RESULT[ps2-streams]} == PASS ]]; then
      vb=$(grep -a -c "revc-audio\] stream .*/ps2/audio/.*\.vb" "$OUT/cutscenes.log")
      rms=$(python3 -I - "$CFG/ps2-streams.wav" <<'PY'
import struct, sys, math
d = open(sys.argv[1], 'rb').read(); j = d.find(b'data'); n = struct.unpack_from('<I', d, j + 4)[0]
v = struct.unpack('<%df' % (n // 4), d[j + 8:j + 8 + n]); print('%.4f' % math.sqrt(sum(x * x for x in v) / max(1, len(v))))
PY
)
      echo "  [ps2-streams] $vb PS2 stream(s) opened from the import, recording RMS $rms"
      if (( vb == 0 )) || python3 -c "import sys; sys.exit(0 if float('$rms') < 0.005 else 1)"; then RESULT[ps2-streams]=FAIL; fi
    fi
  fi
  if [[ $style == pc ]]; then
    SCEN=(formats spots); run pc-guard REVC_IMPORT_DIR="$IMP" REVC_TEST_IMPORT_GUARD=1
  fi
done
echo; echo "| check | result |"; echo "|---|---|"
failed=0
for k in $(printf '%s\n' "${!RESULT[@]}" | sort); do
  echo "| $k | ${RESULT[$k]} |"; [[ ${RESULT[$k]} == FAIL ]] && failed=1
done
exit $failed
