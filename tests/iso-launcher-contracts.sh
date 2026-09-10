#!/usr/bin/env bash
# Classification: process-contract test (STAYS SHELL). Inspect the arguments
# delivered to Archinstall by the real launcher, without starting an installer.
set -euo pipefail

repository=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
fixture=$(mktemp -d)
trap 'rm -rf -- "$fixture"' EXIT
export FIXTURE_ROOT="$fixture"
mkdir -p "$fixture/bin"
cat >"$fixture/bin/cat" <<'CAT'
#!/usr/bin/env bash
if [[ ${1-} == /opt/garage-installer/variant ]]; then
    printf '%s\n' "$FIXTURE_VARIANT"
else
    exec /usr/bin/cat "$@"
fi
CAT
cat >"$fixture/bin/archinstall" <<'ARCHINSTALL'
#!/usr/bin/env bash
[[ -f $FIXTURE_ROOT/keyring-ready ]] || exit 90
printf '%s\n' "$@" >"$FIXTURE_ROOT/args"
ARCHINSTALL
cat >"$fixture/bin/systemctl" <<'SYSTEMCTL'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FIXTURE_ROOT/services"
case "$*" in
    'stop systemd-time-wait-sync.service')
        [[ $FIXTURE_VARIANT == offline ]]
        touch "$FIXTURE_ROOT/time-wait-stopped"
        ;;
    'start pacman-init.service')
        if [[ $FIXTURE_VARIANT == offline ]]; then
            [[ -f $FIXTURE_ROOT/time-wait-stopped ]]
        fi
        [[ ${FAIL_KEYRING:-0} == 0 ]] || exit 42
        touch "$FIXTURE_ROOT/keyring-ready"
        ;;
    *) exit 90 ;;
esac
SYSTEMCTL
printf '#!/usr/bin/env bash\nexit 0\n' >"$fixture/bin/clear"
printf '#!/usr/bin/env bash\nexit 0\n' >"$fixture/bin/zsh"
chmod +x "$fixture/bin/"*

for variant in offline netinstall; do
    rm -f "$fixture/keyring-ready" "$fixture/time-wait-stopped" "$fixture/services"
    printf '\nshell\n' | FIXTURE_VARIANT="$variant" PATH="$fixture/bin:$PATH" \
        bash "$repository/iso/airootfs/usr/local/bin/garage-install" >"$fixture/output"
    grep -qx -- '--skip-version-check' "$fixture/args"
    grep -qx -- '/opt/garage-installer/config.json' "$fixture/args"
    grep -qx -- '/opt/garage-installer/plugin.py' "$fixture/args"
    for option in --offline --no-pkg-lookups --skip-ntp --skip-wkd; do
        if [[ $variant == offline ]]; then
            grep -qx -- "$option" "$fixture/args"
        else
            ! grep -qx -- "$option" "$fixture/args"
        fi
    done
    if [[ $variant == netinstall ]]; then
        [[ $(cat "$fixture/services") == 'start pacman-init.service' ]]
    fi
    rm -f "$fixture/args" "$fixture/keyring-ready"
    printf '\nshell\n' | FAIL_KEYRING=1 FIXTURE_VARIANT="$variant" PATH="$fixture/bin:$PATH" \
        bash "$repository/iso/airootfs/usr/local/bin/garage-install" >"$fixture/output"
    [[ ! -e $fixture/args ]]
    grep -q 'Installer exited with status 42' "$fixture/output"
done
printf 'ISO launcher contracts passed.\n'
