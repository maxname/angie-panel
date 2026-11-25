#!/bin/bash

set -e

# Ensure script is executed with root privileges
if [ "$EUID" -ne 0 ]; then
  echo "Error: Please run as root or with sudo."
  exit 1
fi

echo "==> Detecting operating system environment..."

# Identify OS
if [ -f /etc/os-release ]; then
    # shellcheck source=/etc/os-release
    . /etc/os-release
    OS=$ID
else
    echo "Error: Unsupported OS or /etc/os-release not found."
    exit 1
fi

echo "System detected: $OS $VERSION_ID"

# Execute distribution-specific installation
case "$OS" in
    ubuntu|debian)
        echo "==> Configuring Debian/Ubuntu repositories..."
        apt-get update
        apt-get install -y ca-certificates curl gnupg lsb-release

        curl -fsSL https://angie.software/keys/angie-signing.gpg | gpg --dearmor -o /etc/apt/trusted.gpg.d/angie-signing.gpg
        
        CODENAME=$(lsb_release -cs 2>/dev/null || grep VERSION_CODENAME /etc/os-release | cut -d= -f2)
        
        echo "deb https://download.angie.software/angie/$OS/ $CODENAME main" > /etc/apt/sources.list.d/angie.list
        
        echo "==> Installing Angie..."
        apt-get update
        apt-get install -y angie
        ;;

    centos|rhel|rocky|almalinux|fedora)
        echo "==> Configuring RPM-family repositories..."
        dnf install -y dnf-plugins-core 2>/dev/null || yum install -y yum-utils
        
        cat <<EOF > /etc/yum.repos.d/angie.repo
[angie]
name=Angie repo
baseurl=https://download.angie.software/angie/rhel/\$releasever/\$basearch/
gpgcheck=1
enabled=1
gpgkey=https://angie.software/keys/angie-signing.gpg
EOF

        echo "==> Installing Angie..."
        if command -v dnf &> /dev/null; then
            dnf install -y angie
        else
            yum install -y angie
        fi
        ;;

    alpine)
        echo "==> Configuring Alpine Linux repositories..."
        apk update
        apk add curl ca-certificates
        
        curl -fsSL -o /etc/apk/keys/angie-signing.rsa.pub https://angie.software/keys/angie-signing.rsa.pub
        
        ALPINE_VER=$(echo "$VERSION_ID" | cut -d. -f1,2)
        REPO_URL="https://download.angie.software/angie/alpine/v$ALPINE_VER/main"
        
        if ! grep -q "$REPO_URL" /etc/apk/repositories; then
            echo "$REPO_URL" >> /etc/apk/repositories
        fi
        
        apk update
        echo "==> Installing Angie..."
        apk add angie
        ;;

    *)
        echo "Error: Operating system '$OS' is not supported by this installer (supports RPM, Debian, and Alpine only)."
        echo "Please refer to: https://en.angie.software/angie/docs/installation/oss_packages/"
        exit 1
        ;;
esac

echo "==> Enabling and starting Angie service..."

# Service management
if command -v systemctl &> /dev/null; then
    systemctl enable --now angie
    systemctl status angie --no-pager
elif [ "$OS" = "alpine" ]; then
    rc-update add angie default
    service angie start
fi

echo "=========================================="
echo " Angie installation completed successfully!"
echo "=========================================="