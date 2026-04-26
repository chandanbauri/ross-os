# Phase 8 — POSIX Process Foundation

## Goal

Run a statically-linked musl-compiled `hello world` ELF from the ROSS shell via `exec`.

```
ross> exec /mnt/disk/hello
Hello from Phase 8!
```

The output must appear on the framebuffer and serial port; the process must exit cleanly.

---

## Why Phase 8 Matters

Every compatibility layer in Phases 9–16 sits on top of the abstractions built here:
- **File descriptors** — Windows HANDLE, Linux fd, macOS fd — all map to FDs
- **Process lifecycle** — exit/wait is needed for shells and launchers in all three ABIs
- **mmap / brk** — ELF loaders, dynamic linkers, and JIT translators all depend on it
- **Extended syscalls** — Linux/Windows/macOS all expect: open/read/write/close/exit/mmap

Phase 8 builds these once, correctly, so they don't need to be re-built per-ABI.

---

## Prerequisites (Current State)

Already working:
- `spawn_process`: loads ELF segments, creates Task, enqueues in scheduler (`task.rs:202`)
- 5 syscalls: log, uptime, sys_pipe, sys_pipe_write, sys_pipe_read
- FAT32 read/write at `/mnt/disk` (fix BUG-1 first — see `CLAUDE.md`)
- IPC pipes: kernel ring buffer, max 4 KB, 64 pipes (`ipc.rs`)

Missing:
- No file descriptor table per process
- No `sys_exit` — process runs forever after `_start` returns
- No `sys_write(fd=1, ...)` for stdout
- No `sys_mmap` / `sys_brk` — user-space allocators cannot function
- No `sys_open` / `sys_read(fd, ...)`

---

## Milestones

### M8.1 — File Descriptor Table
**Acceptance:** `sys_write(1, "hello\n", 6)` from a user ELF prints to framebuffer.

Files:
- NEW `ross-kernel/src/fd.rs` — `FdTable` struct (64-slot array), `FdEntry` enum
  - `FdEntry::Stdout` → framebuffer writer
  - `FdEntry::Stderr` → serial COM1
  - `FdEntry::Stdin`  → keyboard ring buffer (`kbuf.rs`)
  - `FdEntry::Pipe(usize)` → wraps an IPC pipe ID
  - `FdEntry::VfsFile { node: Arc<dyn VfsNode>, offset: usize }`
- MODIFY `task.rs` — add `fd_table: FdTable` field; `spawn_process` pre-opens FD 0/1/2
- MODIFY `syscall.rs` — repurpose ID 4 as `sys_write(fd, ptr, len)` dispatching to `FdEntry`

### M8.2 — sys_exit
**Acceptance:** A user binary that calls `syscall(6, 0)` terminates; scheduler doesn't crash.

Files:
- MODIFY `task.rs` — add `TaskState::Dead`; `Scheduler::pick_next` skips Dead tasks and drops them
- MODIFY `syscall.rs` — ID 6 `sys_exit(code)`: set `current_task.state = Dead`, call `pick_next`

Implementation notes:
- Kernel stack and page table leak until Phase 9 adds cleanup — acceptable for now
- Serial print `[SYSCALL] exit(code)` for debug

### M8.3 — sys_brk / sys_mmap
**Acceptance:** `sys_brk(0)` returns the current heap break; `sys_brk(higher)` maps new pages.

Files:
- MODIFY `task.rs` — add `heap_end: u64` (set by `spawn_process` to first page after last PT_LOAD)
- NEW `ross-kernel/src/mmap.rs` — `alloc_anon(cr3, base, size, flags)`: loop `pmm::alloc_page` + `paging::map_user_page`
- MODIFY `syscall.rs`:
  - ID 7 `sys_brk(addr)` → extend heap, return new break; `brk(0)` returns current break
  - ID 8 `sys_mmap(addr,len,prot,flags,fd,off)` → anon only in Phase 8 (`MAP_ANONYMOUS|MAP_PRIVATE`)

Implementation notes:
- `sys_brk(0)` is how musl's malloc discovers the heap start — must work first
- Only anonymous mmap in Phase 8; file-backed mmap comes in Phase 9

### M8.4 — sys_open / sys_read / sys_close
**Acceptance:** User binary can open `/mnt/disk/hello.txt`, read bytes, close the fd.

Files:
- MODIFY `syscall.rs`:
  - ID 9  `sys_open(path_ptr, flags)` → `vfs::open`, insert `FdEntry::VfsFile` into task FdTable, return fd
  - ID 10 `sys_close(fd)` → remove FdEntry
  - Update ID 5 to `sys_read(fd, ptr, len)` dispatching to `FdEntry`
- MODIFY `fd.rs` — `VfsFile` entry with tracked offset

**IMPORTANT — user pointer safety:**  
`path_ptr` is a user virtual address. To read from kernel:
1. `paging::lookup_page(cr3, vaddr)` → physical page
2. `phys_to_virt(phys)` → kernel virtual address
3. Read bytes until null terminator (bounded by page size)  
**Never dereference user pointers directly from ring-0 code.**

### M8.5 — ross-hello: the acceptance test binary
**Acceptance:** `exec /mnt/disk/hello` prints "Hello from Phase 8!" and exits cleanly.

Files:
- NEW `ross-user/hello/Cargo.toml`
- NEW `ross-user/hello/src/main.rs`:
  ```rust
  #[unsafe(no_mangle)]
  pub extern "C" fn _start() -> ! {
      let msg = b"Hello from Phase 8!\n";
      syscall(4, 1, msg.as_ptr() as u64, msg.len() as u64); // sys_write(stdout)
      syscall(6, 0, 0, 0);  // sys_exit(0)
      loop {}
  }
  ```
- MODIFY `boot.sh` — after building, copy `ross-hello` onto `build/disk.img`:
  ```bash
  DEV=$(hdiutil attach -imagekey diskimage-class=CRawDiskImage -nomount build/disk.img | awk '{print $1}')
  mkdir -p /tmp/rossmnt && mount -t msdos $DEV /tmp/rossmnt
  cp target/.../ross-hello /tmp/rossmnt/hello
  umount /tmp/rossmnt && hdiutil detach $DEV
  ```
- MODIFY workspace `Cargo.toml` — add `ross-user/hello` to members

---

## Revised Syscall Table After Phase 8

| ID | Name | Signature | Notes |
|----|------|-----------|-------|
| 1 | sys_log | (ptr, len) → 0 | serial debug (keep for kernel-only use) |
| 2 | sys_uptime | () → ticks | |
| 3 | sys_pipe | () → pipe_fd | returns an fd (not raw pipe ID) |
| 4 | sys_write | (fd, ptr, len) → n | fd 1=stdout, fd 2=stderr, fd≥3=file/pipe |
| 5 | sys_read | (fd, ptr, len) → n | fd 0=stdin, fd≥3=file/pipe |
| 6 | sys_exit | (code) → ! | terminates current task |
| 7 | sys_brk | (addr) → addr | heap management |
| 8 | sys_mmap | (addr,len,prot,flags,fd,off) → addr | anon only for now |
| 9 | sys_open | (path_ptr, flags) → fd | |
| 10 | sys_close | (fd) → 0 | |

---

## Implementation Order

1. Fix BUG-1 (disk.img) — see `CLAUDE.md`. Verify with `write /mnt/disk/test.txt hello`.
2. `fd.rs` + FdTable in Task struct — build + test compile.
3. `sys_exit` (ID 6) + `TaskState::Dead` — simplest, unblocks testing.
4. `sys_write(fd=1, ...)` → framebuffer — proves fd table works.
5. `sys_brk` (ID 7) — `brk(0)` must return heap_end set by ELF loader.
6. `sys_mmap` anonymous (ID 8) — needed by musl malloc.
7. `ross-hello` binary — use only sys_write + sys_exit.
8. Integrate into `boot.sh` disk image copy step.
9. `sys_open` / `sys_read` / `sys_close` (IDs 9/5/10).

---

## Files Summary

| Action | File | Change |
|--------|------|--------|
| CREATE | `ross-kernel/src/fd.rs` | FdTable, FdEntry enum |
| CREATE | `ross-kernel/src/mmap.rs` | `alloc_anon` helper |
| CREATE | `ross-user/hello/Cargo.toml` | New binary crate |
| CREATE | `ross-user/hello/src/main.rs` | Test binary |
| MODIFY | `ross-kernel/src/task.rs` | Add `fd_table`, `heap_end` to Task |
| MODIFY | `ross-kernel/src/syscall.rs` | IDs 4–10 |
| MODIFY | `ross-kernel/src/idt.rs` | Skip/drop Dead tasks in pick_next |
| MODIFY | `boot.sh` | Copy hello binary onto disk image |
| MODIFY | `Cargo.toml` | Add ross-user/hello to workspace |

---

## Definition of Done

- [ ] `write /mnt/disk/test.txt hello` → file created, `cat` reads it back (BUG-1 fixed)
- [ ] `exec /mnt/disk/hello` prints "Hello from Phase 8!" on framebuffer
- [ ] Process exits cleanly; scheduler continues running other tasks
- [ ] `sys_brk(0)` returns a valid non-zero address
- [ ] Serial log shows `[SYSCALL] sys_exit(0)` on clean exit
- [ ] No kernel panic or triple fault on any of the above
