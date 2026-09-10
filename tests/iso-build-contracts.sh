#!/usr/bin/env bash
# Classification: process-contract test (STAYS SHELL). Exercise the real build
# launcher with real Git operations and a recording Docker boundary.
set -euo pipefail

repository=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
fixture=$(mktemp -d)
trap 'status=$?; if ((status)); then cat "$fixture"/*.log >&2; fi; rm -rf -- "$fixture"' EXIT
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=Fixture GIT_AUTHOR_EMAIL=fixture@example.invalid
export GIT_COMMITTER_NAME=Fixture GIT_COMMITTER_EMAIL=fixture@example.invalid
export GARAGE_ISO_CACHE_DIR="$fixture/cache"
export GARAGE_ISO_OUT_DIR="$fixture/output"
unset GARAGE_ISO_PACMAN_CACHE GARAGE_ISO_OFFLINE_PACKAGE_CACHE
unset GARAGE_ISO_OFFLINE_TOOLCHAIN_CACHE GARAGE_ISO_WORK_DIR
export FIXTURE_ROOT="$fixture"
mkdir -p "$fixture/checkout/iso/profile" "$fixture/checkout/system/manifest" "$fixture/checkout/backend" "$fixture/bin"
cp "$repository/iso/build" "$fixture/checkout/iso/build"
printf 'committed profile\n' >"$fixture/checkout/iso/profile/profiledef.sh"
printf 'committed packages\n' >"$fixture/checkout/system/manifest/packages.list"
printf 'committed lockfile\n' >"$fixture/checkout/backend/Cargo.lock"
printf 'committed builder\n' >"$fixture/checkout/iso/container-build"
printf 'private.env\n' >"$fixture/checkout/.gitignore"
ln -s profile/profiledef.sh "$fixture/checkout/iso/profile-link"
git -C "$fixture/checkout" -c init.templateDir= init --quiet --initial-branch=main
git -C "$fixture/checkout" add iso .gitignore system/manifest/packages.list backend/Cargo.lock
git -C "$fixture/checkout" commit --quiet -m 'fixture: initial source'
export EXPECTED_COMMIT
EXPECTED_COMMIT=$(git -C "$fixture/checkout" rev-parse HEAD)
git -C "$fixture/checkout" tag -a fixture-release -m 'fixture release'
printf 'later packages\n' >"$fixture/checkout/system/manifest/packages.list"
git -C "$fixture/checkout" add system/manifest/packages.list
git -C "$fixture/checkout" commit --quiet -m 'fixture: advance main'
git -C "$fixture/checkout" checkout --quiet --detach "$EXPECTED_COMMIT"

# A detached worktree, a moving main, an annotated tag, all three kinds of
# local content, and source modes must not change the selected build inputs.
printf 'unstaged profile\n' >"$fixture/checkout/iso/profile/profiledef.sh"
printf 'staged lockfile\n' >"$fixture/checkout/backend/Cargo.lock"
git -C "$fixture/checkout" add backend/Cargo.lock
printf 'untracked\n' >"$fixture/checkout/untracked"
printf 'ignored fixture value\n' >"$fixture/checkout/private.env"
git -C "$fixture/checkout" status --porcelain=v1 --untracked-files=all >"$fixture/status.before"

cat >"$fixture/bin/docker" <<'DOCKER'
#!/usr/bin/env bash
set -euo pipefail
source_mount= bundle_mount= output_mount= variant=
while (($#)); do
    case "$1" in
        --volume)
            case "$2" in
                *:/garage:ro) source_mount=${2%:/garage:ro} ;;
                *:/bundle:ro) bundle_mount=${2%:/bundle:ro} ;;
                *:/out) output_mount=${2%:/out} ;;
            esac
            shift 2 ;;
        --env)
            case "$2" in GARAGE_ISO_VARIANT=*) variant=${2#*=} ;; esac
            shift 2 ;;
        *) shift ;;
    esac
 done
[[ -n $source_mount && -n $bundle_mount && -n $output_mount && -n $variant ]]
[[ $source_mount != "$FIXTURE_ROOT/checkout" ]]
[[ $(cat "$source_mount/iso/profile/profiledef.sh") == 'committed profile' ]]
[[ $(cat "$source_mount/system/manifest/packages.list") == 'committed packages' ]]
[[ $(cat "$source_mount/backend/Cargo.lock") == 'committed lockfile' ]]
[[ $(cat "$source_mount/iso/container-build") == 'committed builder' ]]
[[ -x $source_mount/iso/build && -L $source_mount/iso/profile-link ]]
[[ ! -e $source_mount/private.env && ! -e $source_mount/untracked && ! -e $source_mount/.git ]]
[[ $(cat "$bundle_mount/source-commit") == "$EXPECTED_COMMIT" ]]
clone="$FIXTURE_ROOT/clone-$variant"
git clone --quiet --branch main "$bundle_mount/garage.bundle" "$clone"
[[ $(git -C "$clone" rev-parse HEAD) == "$EXPECTED_COMMIT" ]]
[[ $(cat "$clone/backend/Cargo.lock") == 'committed lockfile' ]]
printf '%s\n' "$source_mount" >"$FIXTURE_ROOT/last-source"
if [[ ${FAIL_DOCKER:-0} == 1 ]]; then exit 23; fi
printf 'fixture ISO\n' >"$output_mount/garage-$variant-fixture.iso"
DOCKER
chmod +x "$fixture/bin/docker"
export PATH="$fixture/bin:$PATH"

"$fixture/checkout/iso/build" offline fixture-release >"$fixture/offline.log" 2>&1
[[ ! -e $(cat "$fixture/last-source") ]]
[[ -f $GARAGE_ISO_OUT_DIR/garage-offline-fixture.iso.sha256 ]]
"$fixture/checkout/iso/build" netinstall >"$fixture/netinstall.log" 2>&1
[[ ! -e $(cat "$fixture/last-source") ]]
[[ -f $GARAGE_ISO_OUT_DIR/garage-netinstall-fixture.iso.sha256 ]]
git -C "$fixture/checkout" status --porcelain=v1 --untracked-files=all >"$fixture/status.after"
cmp "$fixture/status.before" "$fixture/status.after"

rm -rf "$fixture/clone-offline"
if FAIL_DOCKER=1 "$fixture/checkout/iso/build" offline "$EXPECTED_COMMIT" >"$fixture/failure.log" 2>&1; then
    printf 'A failed container was reported as a successful build.\n' >&2
    exit 1
else
    [[ $? == 23 ]]
fi
[[ ! -e $(cat "$fixture/last-source") ]]
if "$fixture/checkout/iso/build" offline nonexistent-ref >"$fixture/invalid.log" 2>&1; then
    printf 'An invalid source ref was accepted.\n' >&2
    exit 1
fi
printf 'ISO builds isolate committed inputs, preserve checkout state, and clean failed snapshots.\n'
