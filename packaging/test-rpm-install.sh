#!/usr/bin/env bash
# Install the built RPM in a real RPM-based userspace and verify its payload
# and install scriptlets. The Angie dependency is omitted because it is hosted
# in a separate repository and is not needed to validate the panel package.
set -euo pipefail

RPM_FILE="${1:?usage: test-rpm-install.sh PACKAGE.rpm}"
IMAGE="${IMAGE:-fedora:43}"

command -v docker >/dev/null 2>&1 || {
    echo "docker is required" >&2
    exit 1
}
RPM_FILE="$(realpath "$RPM_FILE")"

docker image inspect "$IMAGE" >/dev/null 2>&1 || docker pull "$IMAGE"
docker run --rm --network bridge \
    -v "$RPM_FILE:/tmp/angie-panel.rpm:ro" \
    "$IMAGE" bash -euo pipefail -c '
        dnf -y install systemd shadow-utils
        mkdir -p /run/systemd/system
        cat > /usr/bin/systemctl <<'"'"'SYSTEMCTL'"'"'
#!/usr/bin/bash
printf "%s\n" "$*" >> /tmp/systemctl.log
SYSTEMCTL
        chmod +x /usr/bin/systemctl

        rpm -Uvh --nodeps /tmp/angie-panel.rpm
        grep -qxF "enable angie-panel.service" /tmp/systemctl.log

        : > /tmp/systemctl.log
        rpm -Uvh --replacepkgs --nodeps /tmp/angie-panel.rpm
        if grep -q "^enable " /tmp/systemctl.log; then
            echo "RPM upgrade re-enabled the service" >&2
            exit 1
        fi
        grep -qxF "try-restart angie-panel.service" /tmp/systemctl.log

        rpm -q angie-panel
        test -x /usr/bin/angie-panel
        test -x /usr/bin/apctl
        test -f /etc/angie-panel.toml
        test -f /usr/lib/systemd/system/angie-panel-apply.service
        getent passwd angie-panel >/dev/null
        test "$(stat -c "%U:%G:%a" /var/lib/angie-panel)" = "angie-panel:angie-panel:700"
        test "$(stat -c "%U:%G:%a" /var/lib/angie-panel/public)" = "angie-panel:angie-panel:755"
        rpm -ql angie-panel | grep -q "/snippets/block-exploits.conf$"
    '
