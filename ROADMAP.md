# ROSS OS — Long-Term Roadmap

## Vision

Run **Windows, Linux, and macOS applications natively on ROSS, regardless of binary architecture** (x86_64, ARM64, x86_32, ARMv7).

This means:
- An ELF compiled for Linux x86_64 runs without modification
- A PE32+ compiled for Windows x64 runs via a Win32 compatibility layer
- A Mach-O compiled for macOS arm64 runs via architecture translation + BSD syscall layer
- No recompilation, no source required

This is the same class of goal as **Wine** (Windows on Linux), **Darling** (macOS on Linux), and **Apple Rosetta 2** (ARM64 translation), combined into a single OS.

---

## Phase Map

| Phase | Name | Goal | Status |
|-------|------|------|--------|
| 1 | UEFI Boot | Framebuffer, memory map | ✅ |
| 2 | PMM | Bitmap physical allocator | ✅ |
| 3 | Virtual Memory | 4-level paging, heap | ✅ |
| 4 | Shell | Interactive terminal | ✅ |
| 5 | Higher Half + Syscall | LSTAR/SYSRET, user ring-3 | ✅ |
| 6 | Multitasking + ELF | Preemptive scheduler, ELF loader, VFS | ✅ |
| 7 | Hardware & Storage | PCI, AHCI, FAT32, IPC pipes | ✅ |
| **8** | **POSIX Foundation** | File descriptors, extended syscalls, run musl hello | 🔲 |
| 9 | Linux ABI (static) | Run any statically-linked Linux x86_64 ELF | 🔲 |
| 10 | Dynamic Linking | ELF shared libraries, ld.so equivalent | 🔲 |
| 11 | Linux ABI (dynamic) | Run dynamically-linked Linux binaries (libc, libstdc++) | 🔲 |
| 12 | Windows PE Compatibility | PE32+ loader, NTDLL/kernel32 stubs, run Win32 console apps | 🔲 |
| 13 | macOS Mach-O Compatibility | Mach-O loader, BSD syscall layer, ObjC runtime stubs | 🔲 |
| 14 | Architecture Translation | ARM64 ↔ x86_64 binary translation (JIT recompilation) | 🔲 |
| 15 | Graphics Stack | Vulkan + DXVK (DX→Vulkan) + MoltenVK (Metal→Vulkan) | 🔲 |
| 16 | Full App Compatibility | Run real-world GUI apps (browser, IDE, game) | 🔲 |

---

## Phase 8 — POSIX Foundation
> Detail: `phases/phase.8.md`

**Goal:** Run a statically-linked musl-compiled `hello world` end-to-end.

The current kernel has 5 syscalls and no file descriptors. Phase 8 builds the syscall surface and process model that every compatibility layer in later phases will sit on top of.

Key deliverables:
- Per-process file descriptor table (FDs 0/1/2 = stdin/stdout/stderr)
- `sys_open`, `sys_close`, `sys_read` (fd), `sys_write` (fd), `sys_lseek`, `sys_fstat`
- `sys_exit` (clean process teardown), `sys_mmap`/`sys_munmap`, `sys_brk`
- A second ELF userspace binary (`ross-hello`) that uses musl-style startup

---

## Phase 9 — Linux ABI: Static Binaries

**Goal:** A Linux x86_64 binary compiled with `musl-gcc -static` runs unmodified.

Strategy: Implement the Linux syscall ABI (int 0x80 / syscall instruction, syscall numbers matching Linux 5.x x86_64 ABI). Start with the ~40 syscalls used by 90% of statically-linked programs.

Key deliverables:
- Linux syscall numbers mapped to ROSS kernel equivalents
- `/dev/null`, `/dev/zero`, `/proc/self/maps` stubs
- `clone` (thread creation, light version)
- `execve` — replace current process image with new ELF
- `waitpid` / `exit_group`
- Milestone: `ls`, `cat`, `echo` from a musl-based busybox static binary run

---

## Phase 10 — Dynamic Linking

**Goal:** Load and resolve ELF shared libraries at runtime.

Key deliverables:
- `ld-ross.so` — a minimal dynamic linker (or port of musl's `ldso`)
- `PT_INTERP` segment handling in ELF loader
- PLT/GOT patching, `dlopen` / `dlsym`
- Port of musl libc as a shared object
- Milestone: a dynamically-linked "hello world" (depends on libc.so)

---

## Phase 11 — Linux ABI: Dynamic Binaries

**Goal:** Run typical dynamically-linked Linux binaries.

Key deliverables:
- Full musl libc (or glibc stub layer)
- libstdc++ / libc++ basics
- `vDSO` stub for `clock_gettime`
- `/proc`, `/sys` virtual filesystems (minimal)
- Milestone: run `python3 -c "print('hello')"` from a Linux ARM build (with architecture translation from Phase 14 if needed)

---

## Phase 12 — Windows PE Compatibility

**Goal:** Run Windows console applications (PE32+, x64).

Strategy: Modelled on Wine. The kernel loads PE binaries directly; user-space DLL stubs (`ntdll.dll`, `kernel32.dll`, `msvcrt.dll`) translate Win32 calls to ROSS syscalls.

Key deliverables:
- PE32+ ELF loader (handle `.text`, `.data`, `.bss`, import table)
- TEB/PEB (Thread/Process Environment Block) setup
- `ntdll.dll` stub: `NtAllocateVirtualMemory`, `NtCreateFile`, `NtReadFile`, `NtWriteFile`, `NtClose`, `NtTerminateProcess`
- `kernel32.dll` stub: `CreateFile`, `ReadFile`, `WriteFile`, `ExitProcess`, `GetCommandLine`
- `msvcrt.dll` / `ucrtbase.dll` stub for C runtime
- Milestone: `cmd.exe /c "echo hello"` or a simple Win32 console app runs

---

## Phase 13 — macOS Mach-O Compatibility

**Goal:** Run macOS command-line binaries (Mach-O, arm64 or x86_64).

Strategy: Modelled on Darling. Implement the XNU BSD syscall layer (macOS uses BSD-derived syscalls, syscall numbers differ from Linux).

Key deliverables:
- Mach-O binary loader (LC_SEGMENT_64, LC_MAIN, LC_LOAD_DYLIB)
- XNU BSD syscall layer (~80 core syscalls)
- `libSystem.B.dylib` stub (wraps ROSS syscalls)
- Mach IPC ports (minimal — for `NSLog`, `CFRunLoop`)
- Objective-C runtime stubs (`libobjc.A.dylib`)
- Milestone: `swift hello.swift` compiled binary runs and prints output

---

## Phase 14 — Architecture Translation

**Goal:** Run ARM64 (AArch64) binaries on x86_64 ROSS, and vice versa.

Strategy: JIT binary translation similar to Apple Rosetta 2 or QEMU user-mode emulation. Translate basic blocks on first execution; cache translated code.

Key deliverables:
- ARM64 instruction decoder (all base ISA instructions)
- x86_64 code generator for each decoded ARM64 instruction
- Translation cache (invalidated on `icache` flush syscall)
- Register mapping (ARM64 x0-x30 ↔ x86_64 registers + spill slots)
- Milestone: an ARM64 Linux static binary runs on x86_64 ROSS

---

## Phase 15 — Graphics Stack

**Goal:** Run GUI applications.

Key deliverables:
- Vulkan ICD (software renderer or virtio-gpu backend)
- DXVK (DirectX 9/10/11/12 → Vulkan translation) ported or linked
- MoltenVK (Metal → Vulkan translation)
- Window manager (compositor, basic WM protocol)
- Milestone: a simple OpenGL/Vulkan demo renders to screen

---

## Phase 16 — Full Application Compatibility

**Goal:** Run a real-world application (browser, IDE, or native game).

This phase ties all prior phases together: process model + Linux ABI + Windows PE or macOS Mach-O + architecture translation + graphics stack.

---

## Architecture Decisions (locked in)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Kernel language | Rust (`no_std`) | Memory safety without GC |
| Syscall convention | `syscall` / LSTAR MSR | x86_64 ABI standard |
| Process isolation | Per-process CR3 (4-level PT) | Already implemented |
| Compatibility approach | Kernel-level ABI layers (not full VM) | Performance over isolation |
| Graphics | Vulkan as the universal target | DXVK/MoltenVK/Zink all target Vulkan |
| Architecture translation | JIT (basic-block) not interpretation | ≥ 50% native speed target |
