## Bug Fixes
- Daemon detection now correctly excludes parent processes from the running process check
  - This improves compatibility with package managers that use wrapper/shim executables (such as Scoop)

## NSIS Installer Improvements
- Updated all startup methods to use unified daemon management (`daemon all start`)
  - Registry autostart now uses single entry with proper daemon command
  - Task Scheduler uses daemon management with correct arguments
  - Startup folder VBS scripts updated to use daemon command
  - Finish page correctly launches both daemons through management system
- Ensures consistent daemon lifecycle management across all installation methods
