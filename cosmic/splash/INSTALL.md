# glasskit Plymouth splash

`glasskit-cosmic install splash` builds the frames into `~/.cache/glasskit/splash` and prints these commands with the right paths (it never runs sudo).

Preview first: open `preview.html`. Regenerate with `python3 gen.py`.

## Install
```
cd <repo>/cosmic/splash
sudo cp -r glasskit /usr/share/plymouth/themes/
sudo update-alternatives --install /usr/share/plymouth/themes/default.plymouth default.plymouth /usr/share/plymouth/themes/glasskit/glasskit.plymouth 200
sudo update-alternatives --set default.plymouth /usr/share/plymouth/themes/glasskit/glasskit.plymouth
sudo update-initramfs -u
```

## Test before reboot (from a spare TTY, Ctrl+Alt+F3)
```
sudo plymouthd --debug --debug-file=/tmp/plymouth.log --tty=/dev/tty63
sudo plymouth --show-splash; sleep 8; sudo plymouth quit
```
Errors land in `/tmp/plymouth.log`.

## Revert
```
sudo update-alternatives --set default.plymouth /usr/share/plymouth/themes/pop-basic/pop-basic.plymouth
sudo update-initramfs -u
```
