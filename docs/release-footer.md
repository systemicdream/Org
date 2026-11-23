
## Installing

Org is in early development: back up your notes folder before trying it on notes you care about.

| System | Download | How |
|---|---|---|
| macOS 11+ (Apple silicon and Intel) | `Org-<version>-macos-universal.dmg` | Open it and drag Org to Applications. Org isn't notarized by Apple yet, so the first time, open it, then go to **System Settings → Privacy & Security** and click **Open Anyway**. |
| Linux (any distribution) | `Org-<version>-x86_64.flatpak` | `flatpak install --user Org-<version>-x86_64.flatpak` |
| Debian 12+, Ubuntu 22.04+ | `org_<version>_amd64.deb` | `sudo apt install ./org_<version>_amd64.deb` |
| Fedora | `org-<version>-1.x86_64.rpm` | `sudo dnf install ./org-<version>-1.x86_64.rpm` |
| Other Linux | `org-<version>-linux-x86_64.tar.gz` | Extract it and run `./install.sh` |

On Linux, Org needs a Wayland session (X11 isn't supported). `SHA256SUMS` has the checksums of every file.
