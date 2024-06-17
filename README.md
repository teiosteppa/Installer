# Hachimi Installer
Simple installer for Hachimi.

# Usage
The installer supports both GUI and CLI/Unattended mode. To start in GUI mode, just launch the application without any arguments.

## CLI
- Usage: `hachimi_installer.exe [OPTIONS] <SUBCOMMAND>`
- Subcommands:
    - install
    - uninstall
- Options:
    - `--target <filename or path>`: Specifies the install target, relative to the install dir. If it's an absolute path, the install dir will be ignored. Default: `dxgi.dll`
    - `--install-dir <path>`: Specifies the install directory.
    - `--sleep <milliseconds>`: Duration to sleep before starting the install process.
    - `--prompt-for-game-exit`: When enabled, the installer will display a dialog prompting the user to close the game if it is running. The dialog will continue to display until the user closes the game, or cancel the install process.

# Building
Put hachimi.dll in the root directory, build as any other rust application.

- **MSRV:** v1.77
- Features:
    - `compress_dll`: (Default) Compress the dll using zstd and decompress it during installation.

You might want to disable the default features during development.

# License
[MIT](LICENSE)