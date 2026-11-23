
## Installing

Aster is in early development: back up your notes folder before trying it on notes you care about.

| System | Download | How |
|---|---|---|
| macOS 11+ (Apple silicon and Intel) | `Aster-<version>-macos-universal.dmg` | Open it and drag Aster to Applications. Aster isn't notarized by Apple yet, so the first time, open it, then go to **System Settings → Privacy & Security** and click **Open Anyway**. |
| Linux (any distribution) | `Aster-<version>-x86_64.flatpak` | `flatpak install --user Aster-<version>-x86_64.flatpak` |
| Debian 12+, Ubuntu 22.04+ | `aster_<version>_amd64.deb` | `sudo apt install ./aster_<version>_amd64.deb` |
| Fedora | `aster-<version>-1.x86_64.rpm` | `sudo dnf install ./aster-<version>-1.x86_64.rpm` |
| Other Linux | `aster-<version>-linux-x86_64.tar.gz` | Extract it and run `./install.sh` |

On Linux, Aster needs a Wayland session (X11 isn't supported). `SHA256SUMS` has the checksums of every file.
