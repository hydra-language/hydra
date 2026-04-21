# Hydra

A statically-typed, data-oriented systems programming language.

---

## 🚀 Current Status
Hydra has an active LLVM-based backend and is currently capable of compiling complex, data-oriented programs.

The language successfully supports structs, methods, pointers, references, type casting, and complex control flow.

It is currently sophisticated enough to compile a fully working raytracer (check out the `examples/raytracing` directory!)

NOTE: there is a stack bug if building on windows that does not allow the raytracer to work properly but it works fine on linux

---

## 🛠️ Installation

### 🐧 Linux

#### Part 1: Prerequisites
1.  **Install Rust:** Run the installation command provided at [rustup.rs](https://rustup.rs/).
2.  **Install Clang:** Ensure `clang` is installed on your system using your distribution's package manager. For example:
    * *Ubuntu/Debian:* `sudo apt install clang`
    * *Fedora:* `sudo dnf install clang`
    * *Arch Linux:* `sudo pacman -S clang`

#### Part 2: Building and Installing
1.  **Clone the Repository:** Open your terminal and grab the source code:
    ```bash
    git clone https://github.com/hydra-language/hydra.git
    cd hydra
    ```
2.  **Build the Project:** Compile the compiler in release mode:
    ```bash
    cargo build --release
    ```
3.  **System Path Configuration:** Create a symbolic link to make the `hydrac` command globally accessible. (Ensure `~/.local/bin` is in your system `$PATH`):
    ```bash
    ln -s $(pwd)/target/release/hydrac ~/.local/bin/hydrac
    ```

🎉 **You're all set!** Check out the `examples` folder in the source code to run some sample programs and get a feel for the syntax.

---

### 🪟 Windows

#### Part 1: Prerequisites
1.  **Install Rust:** Download and install the Rust toolchain from [rustup.rs](https://rustup.rs/).
2.  **Install C++ Build Tools:** Download the Visual Studio C++ Build Tools. During the installation process, ensure you select the "Desktop development with C++" workload.
3.  **Install LLVM 14:**
    * Navigate to the [mun-lang LLVM 14 releases page](https://github.com/mun-lang/llvm-package-windows/releases/tag/v14.0.6).
    * Download the `llvm-14.0.6-windows-x64-msvc17-md.7z` file.
    * Extract the contents to a permanent location on your drive.
4.  **Set the LLVM Environment Variable:**
    * Copy the path to the extracted LLVM folder (the folder containing the `bin` directory). 
    * Open PowerShell and run the following command, replacing `"YOUR_COPIED_PATH"` with your actual directory path:
        ```powershell
        [Environment]::SetEnvironmentVariable("LLVM_SYS_140_PREFIX", "YOUR_COPIED_PATH", "User")
        ```

#### Part 2: Building from Source
1.  **Clone the Repository:** Open your terminal or PowerShell and grab the source code:
    ```bash
    git clone https://github.com/hydra-language/hydra.git
    cd hydra
    ```
2.  **Build the Project:** Compile the compiler in release mode using Cargo:
    ```bash
    cargo build --release
    ```

#### Part 3: System Path Configuration
To run the `hydrac` command globally from any terminal, you need to create a symbolic link to your WindowsApps folder (which is already on your system PATH).

1.  Open a **new PowerShell window as Administrator**.
2.  Run the following command. Make sure you have `$env:USERPROFILE` set and replace the `path\to` with the path to your cloned repository:
    ```powershell
    New-Item -ItemType SymbolicLink `
      -Path "$env:USERPROFILE\AppData\Local\Microsoft\WindowsApps\hydrac.exe" `
      -Target "$env:USERPROFILE\path\to\hydra\target\release\hydrac.exe"
    ```

🎉 **You're all set!** Check out the `examples` folder in the source code to run some sample programs and get a feel for the syntax.

---

## 📜 Documentation

For a detailed look at syntax, examples, and language constructs, please refer to the official [**Grammar Reference (grammar.md)**](grammar.md).

## 🤝 Contributing

Contributions are highly welcome! Since the language is in its early stages, there is plenty of room for discussion and improvement. If you have ideas, find issues with the current specification, or would like to help with the implementation, please open an issue or submit a pull request to get involved.
