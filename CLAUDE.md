# ROSS OS — AI Session Context

> Read this file at the start of every session before touching any code.
> Details live in `docs/architecture.md` (internals) and `ROADMAP.md` (plan).

---

## Current State (Phase 7 complete, Phase 8 not started)

| Phase | What | Status |
|-------|------|--------|
| 1–6 | UEFI boot, PMM, paging, heap, PIT, keyboard, shell, multitasking, ELF, VFS | ✅ Done |
| 7.1 | PCI enumeration | ✅ Done |
| 7.2 | AHCI SATA driver (read + write DMA) | ✅ Done |
| 7.3 | FAT32 read/write driver, mounted at `/mnt/disk` | ✅ Done |
| 7.4 | IPC kernel pipes + syscall IDs 3/4/5 | ✅ Done |
| **8** | POSIX process foundation | ✅ Done |

---

## Open Bugs (fix before Phase 8)

### BUG-1: `write` — ~~cannot create files~~ FIXED
**Root cause 1 (resolved):** `build/disk.img` was all-zeros — disk now recreated as 64 MB FAT32.  
**Root cause 2 (resolved):** Shell passed bare filenames (`text.txt`) without prefix; `vfs::create` routed them to the read-only ramdisk instead of FAT32.  
**Fix:** Shell now has a cwd defaulting to `/mnt/disk`. Bare paths are resolved against cwd via `resolve()` in `shell.rs`. Added `cd`, `pwd` commands.  
**Usage:** `write test.txt hello` now works (resolves to `/mnt/disk/test.txt`). Full paths still work: `write /mnt/disk/test.txt hello`.

---

## Critical Non-Obvious Facts

### Memory map
- `KERNEL_VMA_BASE = 0xFFFFFFFF_80000000`  (`paging.rs:40`)
- `phys_to_virt(p) = p + 0xFFFFFFFF_80000000` — wraps around for phys ≥ 2 GB
- Lower half: 4 × 1 GB identity-mapped as 2 MB huge pages (covers phys 0..4 GB)
- **AHCI ABAR** (~4 GB phys) is reached via the low-half identity map: `ABAR_VIRT = abar_phys` directly (NOT via `phys_to_virt`) — `ahci.rs:124`

### GDT selectors
| Ring | Code | Data |
|------|------|------|
| 0 (kernel) | 0x08 | 0x10 |
| 3 (user)   | 0x23 | 0x1B |

### Task switching
- `timer_handler_stub` (global_asm in `idt.rs:113`) saves 15 GPRs, calls `task_timer_handler(rsp)->u128`
- Returns `new_rsp | (new_cr3 << 64)` packed in RAX:RDX
- After pop rdx/rcx, checks `[rsp+16]` (saved CS) for CPL==3; sets DS/ES/FS/GS=0x1B before iretq
- `gdt::set_tss_stack(kernel_stack_top)` called per user-task switch

### FAT32 interior mutability
- `Fat32Node.first_cluster` and `.size` are `AtomicU32` — node is behind `Arc<dyn VfsNode>` which requires `&self`
- Each node stores `parent_cluster` + `dir_entry_offset` to update the dir entry's size field in-place
- `cluster_size_bytes()` = 512 on the current disk (`spc=1`)

### disk.img creation (macOS)
- `newfs_msdos` needs a block device; `boot.sh` uses `hdiutil attach -imagekey diskimage-class=CRawDiskImage -nomount`
- Minimum viable FAT32: ≥ 32 MB. We use 64 MB.
- `parse_bpb` (`fat32.rs:28`) rejects if `fat16 != 0` at BPB offset 0x16.

---

## File Map (non-obvious roles only)

```
ross-kernel/src/
  main.rs          Kernel entry; init order: GDT→IDT→PMM→heap→PCI→AHCI→scheduler→syscall→VFS→PIC/PIT
  paging.rs        phys_to_virt, create_user_address_space, map_user_page, lookup_page
  task.rs          Task/Scheduler (VecDeque); spawn_process = ELF load + enqueue task
  idt.rs           timer_handler_stub (global_asm), task_timer_handler, exception handlers
  syscall.rs       LSTAR MSR setup; syscall_dispatch (IDs 1–5); syscall_handler_stub (global_asm)
  fat32.rs         BPB parse, cluster r/w, FAT chain alloc, Fat32Node VfsNode impl
  vfs.rs           open() / create() routing: /mnt/disk/* → disk_root, /* → root_node
  ipc.rs           Kernel pipe ring buffers (VecDeque, max 4 KB each, max 64 pipes)
  elf.rs           ElfHeader + ProgramHeader parser (used by task::spawn_process)
  pci.rs           Port I/O config space scan; find_by_class(class, subclass)
  ahci.rs          AHCI DMA init; read_sectors / write_sectors (max 8 sectors/call)
  ramfs.rs         Read-only TAR-based initrd (hello.txt, motd.txt, init.elf)

ross-user/init/src/main.rs    User init: syscall(1, msg) then spin loop

boot.sh            Build all crates + prepare disk.img + launch QEMU
```

---

## Syscall Table (Phase 8)

| ID | Name | Signature | Notes |
|----|------|-----------|-------|
| 1 | sys_log | (ptr, len) → 0 | serial debug |
| 2 | sys_uptime | () → ticks | 100 Hz |
| 3 | sys_pipe | () → fd | IPC pipe, returns fd |
| 4 | sys_write | (fd, ptr, len) → n | fd 1=framebuffer, 2=serial |
| 5 | sys_read | (fd, ptr, len) → n | fd 0=keyboard |
| 6 | sys_exit | (code) → ! | marks task Dead |
| 7 | sys_brk | (addr) → addr | 0=query, else extend heap |
| 8 | sys_mmap | (len) → addr | anon alloc after heap_end |
| 9 | sys_open | (path_ptr, flags) → fd | VFS open |
| 10 | sys_close | (fd) → 0 | |

---

## Build Commands

```bash
./boot.sh                                             # build everything + run QEMU

cargo build -p ross-kernel --target x86_64-unknown-none
cargo build -p rb-loader   --target x86_64-unknown-uefi
RUSTFLAGS="-C relocation-model=static -C link-arg=-Tross-user/linker.ld" \
    cargo build -p ross-init --target x86_64-unknown-none

# Recreate disk if blank:
rm build/disk.img && ./boot.sh
```

---

## Shell Commands

`help` `clear` `pwd` `cd [path]` `ls [path]` `cat <path>` `write <path> <content>` `exec <path>`  
`lspci` `disk` `memory` `uptime` `version` `reboot`

Default cwd = `/mnt/disk`. Relative paths resolved via `resolve()` in `shell.rs:29`.

---

## Next: Phase 9

See `ROADMAP.md` for the full plan.  
Goal: Linux ABI compatibility — run any statically-linked Linux x86_64 ELF unmodified.
