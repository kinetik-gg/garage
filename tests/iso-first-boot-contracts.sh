#!/usr/bin/env bash
# Classification: process-contract test (STAYS SHELL). Run the installed entry
# function with a temporary home/payload and fake privileged system commands.
set -euo pipefail

repository=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
fixture=$(mktemp -d)
trap 'status=$?; if ((status)); then cat "$fixture"/*.log >&2; fi; rm -rf -- "$fixture"' EXIT
export FIXTURE_ROOT="$fixture"
export FIRST_BOOT_SCRIPT="$repository/iso/airootfs/opt/garage-installer/garage-first-boot"
mkdir -p "$fixture/bin"

cat >"$fixture/bin/sudo" <<'SUDO'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FIXTURE_ROOT/privileged.log"
case "$1" in
    install)
        [[ $* == "install -m 0644 $FIXTURE_ROOT/offline/pacman.conf.online /etc/pacman.conf" ]]
        [[ ${FAIL_RESTORE:-0} == 0 ]] || exit 31
        cp "$FIXTURE_ROOT/offline/pacman.conf.online" "$FIXTURE_ROOT/pacman.conf"
        ;;
    rm)
        if [[ $* == "rm -rf -- $FIXTURE_ROOT/offline" ]]; then
            if [[ ${FAIL_PAYLOAD_REMOVE:-0} == 1 ]]; then
                rm -f -- "$FIXTURE_ROOT/offline/pacman.conf.online"
                exit 32
            fi
            rm -rf -- "$FIXTURE_ROOT/offline"
        elif [[ $* == "rm -f -- $FIXTURE_ROOT/profile-hook $FIXTURE_ROOT/installer-user" ]]; then
            [[ ${FAIL_HOOK_REMOVE:-0} == 0 ]] || exit 33
            rm -f -- "$FIXTURE_ROOT/profile-hook" "$FIXTURE_ROOT/installer-user"
        else
            printf 'Unexpected privileged deletion: %s\n' "$*" >&2
            exit 90
        fi
        ;;
    *) printf 'Unexpected privileged command: %s\n' "$*" >&2; exit 90 ;;
esac
SUDO
printf '#!/usr/bin/env bash\nexit 0\n' >"$fixture/bin/clear"
chmod +x "$fixture/bin/sudo" "$fixture/bin/clear"
export PATH="$fixture/bin:$PATH"

cat >"$fixture/runner" <<'RUNNER'
#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=/dev/null
source "$FIRST_BOOT_SCRIPT"
# Test fixture paths replace only the physical payload/hook locations. System
# commands are intercepted; /etc/pacman.conf remains the real command target.
offline_root="$FIXTURE_ROOT/offline"
profile_hook="$FIXTURE_ROOT/profile-hook"
installer_user_file="$FIXTURE_ROOT/installer-user"
first_boot "$@"
RUNNER

reset_fixture() {
    rm -rf -- "${fixture:?}/home" "$fixture/offline"
    mkdir -p "$fixture/home/repositories/garage" "$fixture/home/.local/state/garage-installer"
    state="$fixture/home/.local/state/garage-installer"
    touch "$state/pending" "$fixture/profile-hook" "$fixture/installer-user"
    : >"$fixture/bootstrap.log"
    : >"$fixture/privileged.log"
    rm -f "$fixture/pacman.conf"
    cat >"$fixture/home/repositories/garage/bootstrap.sh" <<'BOOTSTRAP'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "${GARAGE_OFFLINE_ROOT:-none}|${CARGO_NET_OFFLINE:-none}|${GARAGE_FORCE:-0}" >>"$FIXTURE_ROOT/bootstrap.log"
# Bootstrap inherits the wrapper's lock descriptor, so wrapper death cannot
# make a live child invisible to an explicit retry.
[[ -e /proc/$$/fd/9 ]]
if [[ -d $FIXTURE_ROOT/offline ]]; then
    [[ $GARAGE_OFFLINE_ROOT == "$FIXTURE_ROOT/offline" && $CARGO_NET_OFFLINE == true ]]
    [[ $RUSTUP_DIST_SERVER == http://127.0.0.1:9 ]]
    [[ $RUSTUP_UPDATE_ROOT == http://127.0.0.1:9/rustup ]]
fi
if [[ ${EXPECT_FORCE:-0} == 1 ]]; then [[ $GARAGE_FORCE == 1 ]]; fi
exit "${FAIL_BOOTSTRAP:-0}"
BOOTSTRAP
    chmod +x "$fixture/home/repositories/garage/bootstrap.sh"
}

invoke() (
    export HOME="$fixture/home"
    unset GARAGE_OFFLINE_ROOT CARGO_NET_OFFLINE GARAGE_FORCE RUSTUP_DIST_SERVER RUSTUP_UPDATE_ROOT
    exec bash "$fixture/runner" "$@"
)

expect_failure() {
    local expected=$1
    shift
    if invoke "$@" >"$fixture/run.log" 2>&1; then
        printf 'Expected failure %s but installer succeeded.\n' "$expected" >&2
        exit 1
    else
        [[ $? == "$expected" ]]
    fi
    if grep -Fq 'installation is complete' "$fixture/run.log"; then
        printf 'Failed installation printed completion.\n' >&2
        exit 1
    fi
}

add_offline_payload() {
    mkdir -p "$fixture/offline"
    printf 'online pacman configuration\n' >"$fixture/offline/pacman.conf.online"
}

# A netinstall completes once; subsequent normal logins are inert.
reset_fixture
invoke >"$fixture/run.log" 2>&1
[[ ! -f $state/pending && ! -f $state/running && ! -f $state/failed ]]
[[ -f $state/bootstrapped && ! -f $fixture/profile-hook ]]
invoke >"$fixture/run.log" 2>&1
[[ $(wc -l <"$fixture/bootstrap.log") == 1 ]]
expect_failure 1 --retry

# Failed offline bootstrap preserves its payload and does not auto-run again.
reset_fixture
add_offline_payload
FAIL_BOOTSTRAP=17 expect_failure 17
[[ -f $state/failed && ! -f $state/pending && ! -f $state/bootstrapped ]]
[[ -d $fixture/offline && ! -s $fixture/privileged.log ]]
grep -Fq '/usr/local/lib/garage/first-boot --retry' "$fixture/run.log"
invoke >"$fixture/run.log" 2>&1
[[ $(wc -l <"$fixture/bootstrap.log") == 1 ]]
EXPECT_FORCE=1 invoke --retry >"$fixture/run.log" 2>&1
[[ $(wc -l <"$fixture/bootstrap.log") == 2 ]]
[[ ! -d $fixture/offline && ! -f $state/failed && ! -f $state/running ]]
grep -Fxq 'online pacman configuration' "$fixture/pacman.conf"

# Each cleanup boundary can fail independently. Retry must skip bootstrap,
# finish cleanup, preserve the recovery marker, and avoid a false success.
for failure in FAIL_RESTORE FAIL_PAYLOAD_REMOVE FAIL_HOOK_REMOVE; do
    reset_fixture
    add_offline_payload
    case "$failure" in
        FAIL_RESTORE) status=31 ;;
        FAIL_PAYLOAD_REMOVE) status=32 ;;
        FAIL_HOOK_REMOVE) status=33 ;;
    esac
    export "$failure=1"
    expect_failure "$status"
    unset "$failure"
    [[ -f $state/failed && -f $state/bootstrapped ]]
    invoke --retry >"$fixture/run.log" 2>&1
    [[ $(wc -l <"$fixture/bootstrap.log") == 1 ]]
    [[ ! -d $fixture/offline && ! -f $state/failed && ! -f $state/running ]]
    [[ ! -f $fixture/profile-hook && ! -f $fixture/installer-user ]]
done

# A stale running marker is recoverable only once the owning process releases
# its lock. A rejected concurrent retry cannot alter state or invoke bootstrap.
reset_fixture
mv "$state/pending" "$state/running"
exec 8>"$state/lock"
flock -n 8
expect_failure 1 --retry
[[ -f $state/running && ! -s $fixture/bootstrap.log ]]
flock -u 8
exec 8>&-
EXPECT_FORCE=1 invoke --retry >"$fixture/run.log" 2>&1
[[ ! -f $state/running && $(wc -l <"$fixture/bootstrap.log") == 1 ]]

# Smoke the actual executable's argument dispatch and inert-login path too.
HOME="$fixture/home" bash "$FIRST_BOOT_SCRIPT"
if HOME="$fixture/home" bash "$FIRST_BOOT_SCRIPT" --invalid >"$fixture/run.log" 2>&1; then
    exit 1
else
    [[ $? == 2 ]]
fi
printf 'First boot retries preserve offline inputs, serialize bootstrap, and recover all cleanup failures.\n'
