# Authentication

Garage keeps `hyprpolkitagent` as its PolicyKit authentication backend. The
upstream 0.1.3 package embeds its QML interface inside the executable, so an
external theme cannot fix its oversized title and sparse layout.

`system/hyprpolkitagent/build` downloads that exact upstream release, verifies
the Arch package's SHA-256 checksum, substitutes Garage's QML front end, and
installs the resulting binary at
`~/.local/lib/garage/hyprpolkitagent`. The upstream BSD-3-Clause license is
retained beside the build assets.

The `hyprpolkitagent.service` drop-in selects the Garage build and forces Qt
Quick's Basic control backend so an unrelated toolkit style cannot change the
modal's geometry. The modal uses Garage's generated Qt palette and Plus Jakarta
Sans, with bounded typography, a compact request summary, stable error space,
and explicit blocked/authenticating states.

Only the interface is replaced. Passwords still pass directly from the local
QML field to upstream's polkit-qt authentication session; Garage does not log,
store, or relay them through Quickshell or its settings backend.
