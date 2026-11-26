## Code Understanding — IMPORTANT

**YOU MUST use `codebase-memory` tools FIRST** whenever you need to understand the project.

Before running any Grep, Glob, or file search, call `codebase-memory` for:
- "How does X work?"
- "Where is Y defined?"
- "What calls Z?"
- Tracing paths between components
- Architecture questions

**Only fall back to Grep/Glob if `codebase-memory` returns no result or the repo is not indexed.**

This is a hard rule, not a preference.
## Comment & tài liệu

- **KHÔNG** thêm comment vào code khi không được yêu cầu.
- **KHÔNG** tạo mới hoặc sửa `README.md` (hay bất kỳ file tài liệu `.md` nào) khi không được yêu cầu.
- **KHÔNG** code unit test khi chưa có yêu cầu


## Agent Routing

Use subagents (via the `Agent` tool) for these task types. Do not handle them entirely inline when they are non-trivial.

| Task           | Agent    | When to use                                                                      |
| -------------- | -------- | -------------------------------------------------------------------------------- |
| Implementation | `coder`  | Writing new features, fixing bugs, refactoring code                              |
| Code review    | `review` | Reviewing diffs, PRs, or any code for correctness, style, and issues             |
| Testing        | `tester` | Writing unit/integration tests, verifying test coverage, debugging failing tests |
| Cli-command    | `cli-command` | Running shell commands, CLI operations, npm/package manager commands, git operations, and any terminal-based tasks |

**Routing rules:**

- If the user asks to **implement or fix** something → delegate to `coder`.
- If the user asks to **review** code, a PR, or a diff → delegate to `review`.
- If the user asks to **write tests** or **verify behavior** → delegate to `tester`.
- For multi-step tasks (e.g. implement then test), chain agents sequentially: `coder` → `tester`.
- For simple, one-liner questions or lookups, handle inline — no need to spawn a subagent.