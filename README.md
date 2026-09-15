# Hydra

Hydra is a statically typed, data-oriented systems programming language focused on explicit semantics, predictable performance, and low-level control without giving up strong compile-time guarantees.

The compiler is written in Rust and uses LLVM for native code generation.

---

## 🚀 Current Status

Hydra is under active development and already implements a substantial portion of its core language and compiler pipeline.

The compiler currently includes:

* Static type checking and type inference
* Structs and extension-based methods
* Generic structs and functions with monomorphization
* References, mutable references, and raw pointers
* Ownership, move checking, and borrow checking
* Fixed-size arrays and slices
* Compile-time bounds checking when indexes and lengths are known
* Explicit numeric and pointer casts
* Range-based `for` loops, `foreach`, `while`, and general control flow
* Value-producing `if` expressions
* Module inclusion and qualified paths
* Compiler intrinsics for low-level operations
* HIR and MIR intermediate representations
* MIR optimization passes
* LLVM-based native code generation

Hydra's standard library is also beginning to provide the foundations expected from a systems language, including memory layout operations, pointer primitives, allocation, and an early generic `Vec<T>` implementation.

Hydra is still experimental. Language syntax, compiler internals, diagnostics, and standard-library APIs may change as the language develops.

---

## 🛠️ Installation

### 🐧 Linux

#### Part 1: Prerequisites

1. **Install Rust:** Run the installation command provided at [rustup.rs](https://rustup.rs/).
2. **Install Clang:** Ensure `clang` is installed on your system using your distribution's package manager. For example:

   * *Ubuntu/Debian:* `sudo apt install clang`
   * *Fedora:* `sudo dnf install clang`
   * *Arch Linux:* `sudo pacman -S clang`

#### Part 2: Building and Installing

1. **Clone the Repository:** Open your terminal and grab the source code:

   ```bash
   git clone https://github.com/hydra-language/hydra.git
   cd hydra
   ```
2. **Build the Project:** Compile the compiler in release mode:

   ```bash
   cargo build --release
   ```
3. **System Path Configuration:** Create a symbolic link to make the `hydrac` command globally accessible. (Ensure `~/.local/bin` is in your system `$PATH`):

   ```bash
   ln -s $(pwd)/target/release/hydrac ~/.local/bin/hydrac
   ```

🎉 **You're all set!** Check out the `examples` folder in the source code to run some sample programs and get a feel for the syntax.

---

### 🪟 Windows

#### Part 1: Prerequisites

1. **Install Rust:** Download and install the Rust toolchain from [rustup.rs](https://rustup.rs/).
2. **Install C++ Build Tools:** Download the Visual Studio C++ Build Tools. During the installation process, ensure you select the "Desktop development with C++" workload.
3. **Install LLVM 14:**

   * Navigate to the [mun-lang LLVM 14 releases page](https://github.com/mun-lang/llvm-package-windows/releases/tag/v14.0.6).
   * Download the `llvm-14.0.6-windows-x64-msvc17-md.7z` file.
   * Extract the contents to a permanent location on your drive.
4. **Set the LLVM Environment Variable:**

   * Copy the path to the extracted LLVM folder (the folder containing the `bin` directory).
   * Open PowerShell and run the following command, replacing `"YOUR_COPIED_PATH"` with your actual directory path:

     ```powershell
     [Environment]::SetEnvironmentVariable("LLVM_SYS_140_PREFIX", "YOUR_COPIED_PATH", "User")
     ```

#### Part 2: Building from Source

1. **Clone the Repository:** Open your terminal or PowerShell and grab the source code:

   ```bash
   git clone https://github.com/hydra-language/hydra.git
   cd hydra
   ```
2. **Build the Project:** Compile the compiler in release mode using Cargo:

   ```bash
   cargo build --release
   ```

#### Part 3: System Path Configuration

To run the `hydrac` command globally from any terminal, you need to create a symbolic link to your WindowsApps folder (which is already on your system PATH).

1. Open a **new PowerShell window as Administrator**.
2. Run the following command. Make sure you have `$env:USERPROFILE` set and replace the `path\to` with the path to your cloned repository:

   ```powershell
   New-Item -ItemType SymbolicLink `
     -Path "$env:USERPROFILE\AppData\Local\Microsoft\WindowsApps\hydrac.exe" `
     -Target "$env:USERPROFILE\path\to\hydra\target\release\hydrac.exe"
   ```

🎉 **You're all set!** Check out the `examples` folder in the source code to run some sample programs and get a feel for the syntax.

---

## 📜 Documentation

For a detailed look at Hydra's syntax, examples, language constructs, and planned features, see the official [**Grammar Reference**](docs/grammar.md).

## 🤝 Contributing

Contributions are highly welcome! Since the language is in its early stages, there is plenty of room for discussion and improvement. If you have ideas, find issues with the current specification, or would like to help with the implementation, please open an issue or submit a pull request to get involved.
