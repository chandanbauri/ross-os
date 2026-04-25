# ROSS OS — Session Context

**Project:** R.O.S.S. (Rapid Operating System Shell) — a bare-metal x86_64 OS written in Rust (`no_std`).
**Repo:** `/Users/chandanbauri/personal/rust/ross-os`
**Author:** Chandan Bauri

---

## Architecture at a Glance

```
ross-os/
├── rb-loader/          UEFI bootloader (x86_64-unknown-uefi)
├── ross-kernel/        Kernel (x86_64-unknown-none, no_std)
│   └── src/
│       ├── main.rs         Kernel entry; initialises subsystems in order
│       ├── gdt.rs          GDT + TSS (ring-0 kernel stack for syscall/iretq)
│       ├── idt.rs          IDT, exception handlers, timer ISR, task switching
│       ├── pmm.rs          Physical memory manager (bitmap allocator)
│       ├── heap.rs         Kernel heap (linked-list allocator)
│       ├── paging.rs       4-level page tables, user address space creation
│       ├── task.rs         Task struct, Scheduler, spawn_process (ELF loader)
│       ├── idt.rs          timer_handler_stub (saves 15 GPRs, calls pick_next)
│       ├── syscall.rs      syscall/sysret via LSTAR MSR; dispatch for IDs 1-5
│       ├── ipc.rs          Kernel pipe ring buffers (sys_pipe/read/write)
│       ├── vfs.rs          VFS trait + open/create; two mounts: ramdisk + disk
│       ├── ramfs.rs        TAR-based RAM filesystem (read-only initrd)
│       ├── fat32.rs        FAT32 driver (read + write, 8.3 names, AtomicU32 nodes)
│       ├── pci.rs          PCI bus enumeration (port I/O config space)
│       ├── ahci.rs         AHCI SATA driver (DMA, read/write sectors)
│       ├── shell.rs        Interactive shell (help/ls/cat/write/exec/lspci/disk/…)
│       ├── writer.rs       Framebuffer text renderer
│       ├── keyboard.rs     PS/2 keyboard ring buffer
│       ├── pit.rs          PIT timer (100 Hz)
│       ├── pic.rs          8259 PIC (IRQ remapping, EOI)
│       ├── elf.rs          ELF64 header + program header parser
│       └── serial.rs       Serial port (COM1) debug output
├── ross-user/
│   └── init/src/main.rs    User-space init process; syscall(1, …) hello message
├── ross-common/            Shared BootInfo struct (bootloader ↔ kernel ABI)
├── boot.sh                 Build + QEMU launch script
└── phases/                 Per-phase roadmap docs
```

---

## Boot Sequence

1. UEFI firmware → `rb-loader.efi` (BOOTX64.EFI on FAT32 ESP)
2. Loader reads `kernel.elf` (flat binary) + `initrd.tar` from ESP
3. Loader calls kernel entry with `BootInfo` struct (framebuffer, memory map, ramdisk ptr)
4. Kernel init order in `main.rs`:
   - Clear BSS, set up GDT/TSS, load IDT
   - Init PMM from memory map, init heap
   - PCI enumerate → AHCI init
   - Scheduler: set main task, add heartbeat task
   - Syscall (LSTAR MSR)
   - VFS: mount ramdisk (TAR) + FAT32 disk at `/mnt/disk`
   - PIC/PIT/Keyboard
   - Draw splash screen, enter event loop

---

## Kernel Subsystem Notes

### Memory Layout
- Kernel: higher half (`0xFFFFFFFF80000000+`)
- `phys_to_virt(phys)` = `phys + HIGHER_HALF_OFFSET` (wraps for phys ≥ 2 GB)
- AHCI ABAR is at ~4 GB phys; stored as phys == virt via low-half identity map
- User stacks mapped at `0x0000_0000_0050_0000` (8 × 4 KB pages)

### GDT Segments (ring indices)
| Index | Selector | Description         |
|-------|----------|---------------------|
| 0     | 0x00     | Null                |
| 1     | 0x08     | Kernel Code (ring 0)|
| 2     | 0x10     | Kernel Data (ring 0)|
| 3     | 0x18     | User Data (ring 3)  |
| 4     | 0x20     | User Code (ring 3)  |
| 5+6   | 0x28     | TSS (128-bit)       |

User CS=0x23 (0x20|3), User SS=0x1B (0x18|3).

### Task Switching
- Preemptive at 100 Hz via PIT IRQ0 → `timer_handler_stub` (global_asm)
- Stub saves 15 GPRs, calls `task_timer_handler(rsp) -> u128` (returns new_rsp | new_cr3<<64)
- After `pop rdx/rcx`, checks `[rsp+16]` (saved CS) for CPL==3; if so sets DS/ES/FS/GS=0x1B
- TSS.RSP0 updated per-task via `gdt::set_tss_stack(kernel_stack_top)`

### Syscall Table
| ID | Name       | Args                        | Returns          |
|----|------------|-----------------------------|------------------|
| 1  | sys_log    | ptr, len, –                 | 0                |
| 2  | sys_uptime | –                           | ticks (u64)      |
| 3  | sys_pipe   | –                           | pipe_id or !0    |
| 4  | sys_write  | pipe_id, buf_ptr, len       | bytes or !0      |
| 5  | sys_read   | pipe_id, buf_ptr, len       | bytes or !0      |

### FAT32 Driver
- Mounted at `/mnt/disk` via `vfs::mount_disk`
- Nodes use `AtomicU32` for `first_cluster`/`size` (interior mutability behind `Arc<dyn VfsNode>`)
- Each node carries `parent_cluster` + `dir_entry_offset` for in-place size updates
- `write()`: lazy first-cluster allocation, read-modify-write per cluster, updates dir-entry size
- `create()`: finds free 8.3 slot, writes dir entry, returns node with parent info
- `to_8_3()`: converts "foo.txt" → `['F','O','O',' ',' ',' ',' ',' ','T','X','T']`

### VFS Routing
- `open("/mnt/disk/…")` → `disk_root` (Fat32Node)
- `open("/…")` → `root_node` (TarFileSystem, read-only)
- `create(path)` splits on last `/` to find parent, calls `parent.create(name)`

---

## Shell Commands
`help` `clear` `ls [path]` `cat <path>` `write <path> <content>` `exec <path>` `lspci` `disk` `memory` `uptime` `version` `reboot`

---

## Phase Status

| Phase | Name                        | Status      |
|-------|-----------------------------|-------------|
| 1     | UEFI boot + framebuffer     | Complete    |
| 2     | Memory management (PMM)     | Complete    |
| 3     | Paging, heap, PIT, keyboard | Complete    |
| 4     | Shell                       | Complete    |
| 5     | Higher-half, syscall        | Complete    |
| 6     | Multitasking, ELF, VFS      | Complete    |
| 7.1   | PCI enumeration             | Complete    |
| 7.2   | AHCI storage driver         | Complete    |
| 7.3   | FAT32 read/write filesystem | Complete*   |
| 7.4   | IPC pipes (kernel + syscall)| Complete    |
| 7.4   | IPC end-to-end (two ELFs)   | Pending     |

\* FAT32 write requires a proper 64 MB FAT32 `build/disk.img` — `boot.sh` now handles this automatically.

---

## Current Open Items

1. **FAT32 write smoke test** — run `write /mnt/disk/test.txt hello` then `cat /mnt/disk/test.txt` to confirm the full create→write→read path works end-to-end.

2. **Phase 7.4 end-to-end IPC** — update `ross-init` to call `sys_pipe`, write to it, add a second ELF (`ross-reader`) that reads the same pipe ID. Milestone: two ELF binaries exchange data through a kernel pipe.

3. **`exec` stability** — user-mode ELF runs (preemptive scheduler queues it), but the init binary currently spins on `nop`. Need to confirm `[SYSCALL LOG] Hello from Userland ELF!` appears in serial before the loop.

---

## Key Build Commands

```bash
# Build + run everything
./boot.sh

# Kernel only
cargo build -p ross-kernel --target x86_64-unknown-none

# User init binary
RUSTFLAGS="-C relocation-model=static -C link-arg=-Tross-user/linker.ld" \
    cargo build -p ross-init --target x86_64-unknown-none

# Recreate disk (if blank/wrong format)
rm build/disk.img && ./boot.sh
```

---

## disk.img Notes

- Must be **64 MB**, formatted as **FAT32** (not FAT12/16).
- `parse_bpb` rejects images with `fat16 != 0` at BPB offset 0x16.
- macOS: `newfs_msdos -F 32` needs a block device — `boot.sh` uses `hdiutil attach` to loop-mount the file first.
- If `build/disk.img` is all zeros, delete it and re-run `boot.sh`.
