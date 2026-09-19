# Keep ArchISO's remote startup-script support.
~/.automated_script.sh

case "$(tty)" in
    /dev/tty1 | /dev/ttyS0)
        exec /usr/local/bin/garage-install
        ;;
esac
