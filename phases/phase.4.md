# ROSS Roadmap: Phase 4 (Shell & Basic Commands)

This phase moves ROSS from an interactive display to a functional command-line environment.

## 1. Line Discipline & Editing
- [ ] Implement Backspace support in the `kbuf` and `writer`.
- [ ] Implement a command buffer to store the current line.
- [ ] **Milestone:** Be able to type a full line and delete characters without visual artifacts.

## 2. Command Framework
- [ ] Create a registry of internal kernel commands.
- [ ] Implement a string parser to split input into command and arguments.
- [ ] **Milestone:** Execute a simple `help` command that lists available functions.

## 3. System Commands
- [ ] Implement `clear`: Reset the screen and reposition the cursor.
- [ ] Implement `memory`: Show Detailed PMM statistics (Total, Used, Free pages).
- [ ] Implement `uptime`: Show precise system time since boot.
- [ ] **Milestone:** Provide a set of tools to inspect system state.

## 4. Visual Polish
- [ ] Implement a proper shell prompt `ross>`.
- [ ] Support color tags in the writer for errors (Red) and success (Green).
- [ ] **Milestone:** An environment that looks and feels like a professional terminal.
