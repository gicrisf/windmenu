# windmenu
Fast like the wind, a WINdows DMENU-like launcher

# build

You need:
- Cargo to build the rust binaries
- NSIS to build the installer

Then, from your shell:

```
cargo build --release
.\build-installer.ps1
.\windmenu-installer.exe
```

Finally, follow the wizard and you're done.
Windmenu should work right out of the box, no configuration required.
The powershell script automatically downloads the release artifacts of wlines (specifically, a fork I made to work well with windmenu).
I'm assuming the a x64 architecture right now. If you need to compile for another architecture, just `git clone` my fork of wlines and run `make` to compile it. It's a simple C program with minimal dependencies. Let me know if you encounter any problem (but keep in mind that many things could change, since this is still in heavy development).
