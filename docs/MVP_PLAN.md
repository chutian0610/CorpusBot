# CorpusBot MVP 拆解与实施路径

本文拆解 `.design.md` 中的 MVP 目标，作为第一版实施的工作清单。MVP 只验证核心闭环：

> 用户创建一个 Workspace，顺序导入五到十个 Markdown Source Version；LLM 编译并维护 Wiki Page。随后用户通过 BM25 检索和 LLM 获得带可验证引用的回答，并能查看基础健康检查结果。MVP 的第一用户是个人研究者，桌面端是主交付，CLI 是开发和自动化入口。

## 1. MVP 范围

### 1.1 做

| 领域           | MVP 内容                                                                                                                                      |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| 工程骨架       | Cargo workspace、Rust core/store/search/ingest/lint/agent crate、CLI crate、Tauri 2 + React 前端骨架                                          |
| Wiki 模型      | Markdown、frontmatter、Wikilink、`wiki/index.md`、`wiki/log.md`、核心不变量                                                                   |
| 模板           | 先支持 `generic` 与 `research` 两类；模板数据结构保留扩展能力                                                                                 |
| 存储           | 专用 Workspace 目录、`raw/` 不可变 Source、`.wiki-db/` 元数据与索引、SHA-256、按文件原子写入                                                  |
| 版本管理       | 每个 Workspace 全新初始化为一个本地 Git 仓库，不复用已有 repo；`wiki/` 和 `raw/` 入库，`.wiki-db/` 忽略；Snapshot、History、Restore、事务恢复 |
| Ingest         | Ingest Guard 要求 Recovery 完成且 tracked `wiki/raw` clean；然后单文件串行导入，两步 CoT，draft 校验后统一提交                                |
| Agent Workflow | Rig 作为 provider adapter；`corpusbot-agent` 内置 typed workflow kernel、bounded retry、self-audit 和 LLM audit ledger                        |
| 搜索           | Tantivy BM25 检索，支持增量重建                                                                                                               |
| 问答           | 检索结果拼装上下文，LLM 回答并输出编号引用                                                                                                    |
| Lint           | 死链、孤儿页、必填 frontmatter、Index 漂移                                                                                                    |
| CLI            | `init`、`status`、`ingest`、`query`、`lint`、`snapshot`、`history`、`restore`                                                                 |
| 桌面端         | 项目选择、三栏骨架、文件树、Markdown 预览、Chat、导入入口、Lint 报告、History 列表与 Restore                                                  |

### 1.2 不做

以下能力留到 Alpha 或 Beta：

- PPR 图检索、Graph View、sigma.js。
- Louvain 聚类和社区着色。
- Claim ledger、Review 面板和矛盾审阅工作流。
- 持久化 Ingest 队列、自动文件监听、并行 worker。
- 通用事务 journal、Git 分支合并、remote clone/fetch/pull/push、自动冲突解决。
- 通用 graph framework、graph DSL、Python LangGraph sidecar、LLM 自由选择 next node。
- MCP Server、Deep Research、Save-to-wiki。
- PDF、EPUB、图片的稳定导入；MVP 的验收输入是 UTF-8 Markdown。
- 多模板迁移、多 Provider 热切换。

## 2. MVP 验收标准

MVP 完成时必须满足：

1. `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过。
2. CLI 可完成以下闭环：
   ```bash
   corpusbot init --root /tmp/demo-wiki --template research
   corpusbot status --root /tmp/demo-wiki
   corpusbot ingest --root /tmp/demo-wiki --file ./fixtures/sample.md
   corpusbot query --root /tmp/demo-wiki --question "这份材料的核心结论是什么？"
   corpusbot lint --root /tmp/demo-wiki
   ```
3. 同一个 SHA-256 的文件重复导入时，不会重复生成同一 Source Version 的 drafts；修改后的文件会创建新的独立 Source Version。
4. Ingest 成功后生成或更新 `wiki/index.md` 与 `wiki/log.md`。
5. Query 返回带 `[1]` 这类编号引用的答案；每条引用必须对应真实 Wiki Page，且包含能在该页面中找到的原文 quote。没有可用证据时返回“当前 Wiki 证据不足”。
6. Lint 能识别构造出来的死链、孤儿页、缺失字段和 Index 漂移。
7. Tauri 桌面端能完成：选择项目、查看 Wiki 文件、打开 Markdown、导入 Markdown、提问、查看 Lint 报告。
8. 引擎不会把 LLM 输出直接写入 Wiki；所有写入必须先进入 drafts，再通过事务接口提交。
9. 用户直接编辑 `wiki/` 后，下一次 mutating/open 操作以磁盘内容为准，并刷新 SQLite 中的 page hash 和派生状态；Query/Lint 使用 Revision Manifest 读取当前内容，不做阻塞写。
10. 在五到十个 Markdown 组成的演示 Wiki 上，十个问题中至少八个能通过 citation validation；失败的问题不得返回无引用的模型先验答案。
11. `init` 只接受不存在的路径或空目录；创建的 root 下必须没有 `.git`。引擎在该 root 全新创建专用 Git repository，并显式使用 `main` branch；`.wiki-db/` 被忽略，`wiki/` 和 `raw/` 可提交。
12. Ingest 中途失败或进程重启后，下一次打开 Workspace 自动执行 Recovery：若 run commit 已存在则标记 committed，否则只还原本次触碰的 Wiki/Raw paths。恢复不使用 `git reset --hard`。
13. 用户能看到本地 Snapshot History，并在显式确认后 Restore 整个 `wiki/` + `raw/`；Restore 前会自动保留当前状态。
14. tracked `wiki/raw` dirty 时，新 Ingest 必须在读取来源文件和调用 LLM 之前失败，返回 dirty paths 和“先 Snapshot”的操作提示；不会把用户修改混入 Ingest commit。
15. Workspace 打开时自动恢复 interrupted apply；恢复完成前，Ingest、Snapshot、Restore 等变更操作被阻塞。
16. Git author identity 可在用户级 Settings 配置，默认读取全局 Git config，缺失时使用 `CorpusBot <corpusbot@local.invalid>`。
17. Workspace Lock 只串行化 Commit、Recovery、Snapshot 和 Restore；Query/Lint 不阻塞写，外部编辑器也不获取该锁。
18. clean Workspace 执行 `snapshot` 时不创建 empty commit，返回 `already_clean` 和当前 HEAD 的完整 `snapshot_id`。
19. 每个 Draft 记录 touched Resource 的 expected revision；Commit 前 CAS 全部通过才写入，任一冲突时整个 Draft 拒绝，不产生 partial commit。
20. Query 和 Lint 开始时捕获 Revision Manifest，并把 answer/report 标注为该 manifest；运行期间外部修改不会造成静默混合版本。
21. Query/Lint 不获取 Workspace Lock；但存在 pending Recovery 时先返回 `RECOVERY_PENDING`，避免读取 partial transaction。
22. Snapshot 基于 captured Revision Manifest 和对应 content 构造；capture 后 Workspace 变化时，已完成 Snapshot 仍是一致 point-in-time，并返回 `workspace_changed_after_capture=true`。
23. Restore 先自动创建 pre-restore Snapshot，再在 engine-private staging 中准备目标内容；切换前 Manifest 变化时返回 `RESTORE_CONFLICT`，不覆盖新修改。
24. Manifest ID 由 sorted `(resource_path, resource_revision)` 的 canonical JSON SHA-256 派生；相同读取集拥有相同 ID。
25. 每次 Workflow node attempt 都有持久 audit event；LLM prompt/response 通过引用保存，API key 和完整凭据不进入 audit。
26. 所有 retry/repair loop 都有 deterministic bound；transition 由 Rust validator 和 workflow policy 决定，不由 LLM 自由文本决定。

## 3. 工作拆解

### S0. 工程基线

**目标**：建立可编译、可测试、可扩展的项目骨架。

**工作内容**

- 创建 Cargo workspace。
- 创建 MVP 需要的 crate：
  - `crates/corpusbot-core`：类型、错误、模板、核心不变量。
  - `crates/corpusbot-store`：文件、SQLite、SHA-256、事务。
  - `crates/corpusbot-vcs`：本地 Git repository、Snapshot、History、Restore。
  - `crates/corpusbot-search`：Tantivy 索引和检索。
  - `crates/corpusbot-ingest`：导入编排和两步 CoT。
  - `crates/corpusbot-lint`：确定性健康检查。
  - `crates/corpusbot-agent`：LLM/Rig 适配层。
  - `crates/corpusbot-cli`：命令行薄客户端。
- 创建 Tauri 2 + Vite + React + TypeScript 前端。
- 固定工具链和依赖版本。
- 建立本地质量脚本和测试 fixtures。

**实现路径**

1. 初始化 Rust workspace，先让空 crate 通过 `cargo test`。
2. 初始化前端，先渲染占位三栏布局。
3. 添加 `justfile` 或 `Makefile`，暴露 fmt、clippy、test、check 命令。
4. 为后续阶段准备 `fixtures/sample.md`、重复导入用样本、死链样本和缺字段样本。

**完成标准**

- `cargo test --workspace` 通过。
- 前端 dev server 可打开三栏占位界面。
- Git 状态干净，README 包含本地启动方式。

### S1. Core 类型与模板

**目标**：让 Wiki 的文件形态、元数据和模板规则先稳定下来。

**工作内容**

定义核心类型：

```rust
pub struct WikiDoc {
    pub path: WikiPath,
    pub frontmatter: Frontmatter,
    pub body: String,
}

pub struct Frontmatter {
    pub page_type: PageType,
    pub title: String,
    pub created: Date,
    pub updated: Date,
    pub tags: Vec<String>,
    pub related: Vec<Wikilink>,
    pub sources: Vec<SourceRef>,
}

pub struct SourceRecord {
    pub source_id: String,
    pub source_version_id: String,
    pub original_name: String,
    pub sha256: String,
    pub size: u64,
    pub imported_at: DateTimeUtc,
}

pub struct ResourceRevision {
    pub resource: ResourceId,
    pub revision: Revision,
}

pub struct RevisionManifest {
    pub manifest_id: ManifestId,
    pub head_snapshot_id: SnapshotId,
    pub resources: Vec<ResourceRevision>,
}

```

固化核心不变量：

- 页面必须位于 `wiki/` 下。
- frontmatter 必填：`type`、`title`、`created`、`updated`、`tags`、`related`、`sources`。
- 链接使用 Obsidian Wikilink：`[[target]]` 或 `[[target|alias]]`。
- `wiki/index.md` 是目录。
- `wiki/log.md` 是操作日志。
- `raw/` 只允许在导入时新增，引擎运行期只读。
- Page Identity 由模板、`page_type` 和规范化后的 canonical name 组成。
- 规范化固定为 Unicode NFKC、trim、连续空白折叠和 casefold。
- alias 只作为可解析键，不参与模糊或语义合并；疑似重复输出 `POSSIBLE_DUPLICATE` warning。
- 每个 mutable Resource 都有 Resource Revision；Wiki Page 的 revision 可由 content hash 派生，SQLite/Tantivy 使用 generation。
- Draft 的 `touched_resources` 必须包含 `expected_revision`；新 Resource 的 expected revision 是 `absent`。
- Manifest ID 对 sorted `(resource_path, resource_revision)` 的 canonical JSON 做 SHA-256；resource path 使用 UTF-8 byte order 排序，JSON 不含无关空白。

定义模板模型：

```rust
pub struct WikiTemplate {
    pub id: TemplateId,
    pub name: String,
    pub description: String,
    pub base_types: Vec<PageTypeDef>,
    pub extended_types: Vec<PageTypeDef>,
    pub schema_prompt: String,
}
```

MVP 内置：

- `generic`：只使用基础页面类型。
- `research`：增加 `thesis`、`methodology`、`finding`。

**实现路径**

1. 在 `corpusbot-core` 实现 `WikiPath`、`Wikilink`、`SourceRef`、`Frontmatter`、`ResourceRevision` 和 `RevisionManifest` 的解析和校验。
2. 用常量定义模板，不引入外部模板文件。
3. 提供 `Template::validate_doc()`，供 store、ingest、lint 复用。
4. 实现 `PageIdentity::normalize()` 和 resolver，覆盖大小写、全半角、空白和 alias。
5. 实现 `RevisionManifest::capture()`、`manifest_id()` 和 `verify_unchanged()`；资源变化时返回冲突详情。
6. 补充正例和反例测试：缺字段、非法类型、非法 Wikilink、越界路径、revision mismatch、manifest ID stability 都必须失败。

**完成标准**

- 核心类型可以完整表达 MVP Wiki 页面。
- 模板校验有单元测试覆盖。
- 同名不同大小写/空白会命中同一 Page Identity；相似但规范化名称不同不会自动合并。
- Resource Revision mismatch 能精确报告 changed resource 和 expected/current revision。
- 相同 Resource set 生成的 Manifest ID 相同；顺序、无关空白和 locale 不影响结果。
- 后续 crate 只依赖 core 的类型和校验接口，不各自解释 frontmatter。

### S2. Store、CLI init 与简化事务

**目标**：建立可靠的本地项目存储，并提供第一个可用 CLI 命令。

**工作内容**

实现目录结构：

```text
demo-wiki/
├── .git/
├── .gitignore
├── wiki/
│   ├── index.md
│   └── log.md
├── raw/
│   └── <sha256>/
│       └── original.md
└── .wiki-db/
    ├── corpusbot.sqlite3
    ├── drafts/
    └── tantivy/
```

实现最小 SQLite 表：

```sql
CREATE TABLE sources (
  source_id TEXT PRIMARY KEY,
  sha256 TEXT NOT NULL UNIQUE,
  original_name TEXT NOT NULL,
  size INTEGER NOT NULL,
  imported_at TEXT NOT NULL
);

CREATE TABLE pages (
  path TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  page_type TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE ingest_runs (
  run_id TEXT PRIMARY KEY,
  source_id TEXT NOT NULL,
  status TEXT NOT NULL,
  baseline_snapshot_id TEXT NOT NULL,
  baseline_manifest_id TEXT NOT NULL,
  touched_resources_json TEXT NOT NULL,
  current_backup_dir TEXT,
  created_at TEXT NOT NULL,
  finished_at TEXT
);
```

实现 Resource CAS、Git-backed 事务与自动 Recovery workflow：

1. Ingest 开始时先执行 preflight：确认没有 pending Recovery、Git 状态安全、tracked `wiki/raw` clean，然后捕获 baseline `RevisionManifest`。
2. orchestrator 把所有 LLM 生成结果写入 `.wiki-db/drafts/<run-id>/`。Draft 记录 touched Resource 的 `expected_revision`；新 Resource 的 expected revision 是 `absent`。
3. 最终 Commit 前，`Commit`、`Snapshot`、`Restore` 和 Recovery 获取 `.wiki-db/workspace.lock`；锁已被活进程持有时返回 `WORKSPACE_BUSY` 和 owner summary。Query/Lint 不获取该锁。
4. 锁内 recheck：没有 pending Recovery、Git 状态安全，并逐个校验 Draft 的 touched Resource revision。
5. 任一 expected revision 不匹配时返回 `RESOURCE_CONFLICT`，列出 expected/current revision；整个 Draft 不写入。
6. CAS 全部通过后，按目标路径排序执行 `tempfile -> rename` 原子替换；`wiki/index.md` 与 `wiki/log.md` 在锁内基于当前状态和 Draft 重新生成。
7. 更新 SQLite 页面元数据、`wiki/index.md` 和 `wiki/log.md`。
8. 成功后创建带 `CorpusBot-Run: <run-id>` trailer 的 commit，并把 run 标记为 `committed`。
9. apply 阶段失败时，只从 baseline Snapshot 读取 touched paths 的旧 blob：存在则原子写回，原本不存在则删除新文件；不使用 `git reset --hard`。
10. 下一次打开 Workspace 时自动处理 `status = applying` 的 run：

- 先在历史中查找相同 `CorpusBot-Run` trailer；找到则标记 `committed` 并刷新派生状态。
- 未找到则先把 touched paths 的当前内容复制到 `.wiki-db/recovery/<run-id>/current/`，再从 baseline 精确还原。
- Recovery 完成、失败或被 unsafe Git state 阻塞都必须显式展示。

11. 外部编辑器不获取锁；run 开始后的外部修改允许存在。如果修改命中 touched Resource，CAS 会让整个 run 失败；如果只改 untouched Resource，Commit 只写入 Draft 声明的 scope，不覆盖外部修改。
12. 任意阶段失败时返回结构化错误，LLM 不直接接触最终 Wiki 文件。

实现 CLI：

```bash
corpusbot init --root <dir> --template generic|research
corpusbot status --root <dir>
corpusbot snapshot --root <dir> --message "before manual cleanup"
corpusbot history --root <dir>
corpusbot restore --root <dir> --snapshot <snapshot-id>
```

`snapshot` 返回：

```json
{
  "result": "created | already_clean",
  "snapshot_id": "<full-commit-hash>",
  "manifest_id": "<manifest-id>",
  "workspace_changed_after_capture": false
}
```

clean Workspace 的 `already_clean` 不创建 empty commit；`snapshot_id` 是当前 HEAD 的完整 commit hash，`status` 也用它作为 `head_snapshot_id`。

Snapshot 和 Restore workflow：

1. Snapshot 开始时捕获 Revision Manifest 和对应 Resource content；Manifest ID 使用 canonical `(resource_path, resource_revision)` 列表计算。
2. 在 Workspace Lock 内用 captured content 构造 scoped Git tree 并创建 Snapshot commit。如果 HEAD tree 已等于 captured tree，返回 `already_clean`，不创建 empty commit。
3. commit 完成后 recompute current manifest；若与 captured manifest 不同，保留已完成的 point-in-time Snapshot，并把 `workspace_changed_after_capture` 设为 `true`。
4. Restore 先捕获当前 state，创建 `pre-restore <restore-run-id>` Snapshot；然后在 `.wiki-db/restore/<run-id>/staging/` 从 selected Snapshot 物化完整 scope。
5. 切换前在 Workspace Lock 内 recheck current manifest。若与 Restore 开始时不同，返回 `RESTORE_CONFLICT`；已创建的 pre-restore Snapshot 保留，但后续用户修改必须由用户显式 Snapshot。
6. recheck 通过后执行短切换：先写入 restore phase/journal，再按 `wiki/`、`raw/`、Workspace `.gitignore` 顺序替换；崩溃后下次打开 Workspace 根据 journal 完成或安全回退。

**实现路径**

1. Workspace root 必须是全新专用 Git repository root。`init` 只接受不存在的路径或空目录；目录已存在且非空返回 `ROOT_NOT_EMPTY`，root 自身已有 `.git` 返回 `REPOSITORY_EXISTS`。两者都不复用、不迁移、不 rebase。Git 解析只在 root 发生，不向上查找外层仓库。即使 Workspace 位于外层仓库内部，也在 root 全新创建嵌套 Workspace repository。
2. fresh repository 显式初始化为 `main`，不依赖 `git init.defaultBranch`。
3. 初始化 `.gitignore`，至少忽略 `.wiki-db/` 和操作系统垃圾文件；`wiki/` 与 `raw/` 是版本管理范围。
4. `init` 创建 scaffold 后，只 add `.gitignore`、`wiki/index.md`、`wiki/log.md` 和 `raw/.gitkeep`，创建 `init <template>` baseline commit；root 里的其他用户文件不 add。
5. Snapshot 和 Ingest commit 使用只包含 CorpusBot scope 的临时 index；用户或其他工具 stage 的 unrelated files 不会被带入 commit，也不会被 unstage。Snapshot 不从 worktree 边读边混合，而是把 captured manifest 对应的内容写入 tree。
6. 使用 `corpusbot-vcs` 隔离 Git API；业务层不直接操作 repository。MVP 不执行 remote clone/fetch/pull/push，也不创建/切换/合并 branch。
7. 使用 SHA-256 做来源去重；重复文件返回已有 `source_id`。
8. 使用 `rusqlite` 管理元数据，SQLite 访问收敛到 `corpusbot-store`。
9. 所有路径先 resolve 到项目根目录内部，拒绝绝对路径逃逸和 `..` 逃逸。
10. Markdown frontmatter 使用 `serde_yaml` 解析，Wikilink 提取在 comrak 解析结果上实现。
11. `open_workspace()` 和 mutating workflow 执行前先扫描 `wiki/**/*.md`，以磁盘内容刷新 SQLite 的 page hash 和 page summary；这一步只更新派生状态，不覆盖用户文件。
12. `query()` 和 `run_lint()` 不刷新 SQLite，不获取 Workspace Lock；它们先捕获 `RevisionManifest`，然后按 manifest 内容读取页面。
13. `open_workspace()` 先获取 Workspace Lock 并自动执行 pending Recovery；恢复完成前，Ingest、Snapshot、Restore 等变更操作返回 `RECOVERY_PENDING`。
14. Workspace Lock 是 `.wiki-db/workspace.lock`，记录 hostname、pid、process start marker、operation 和 started_at。活进程持锁时返回 `WORKSPACE_BUSY`；同机进程崩溃后允许 takeover，但 takeover 后必须先执行 Recovery。锁只约束 CorpusBot commit workflow，不要求 Obsidian、文本编辑器或外部 Git 客户端获取锁。
15. 写入统一走 `WikiTx`，禁止 ingest/lint 直接调用 `std::fs::write`。
16. Ingest preflight 只检查 tracked `wiki/raw` 和 Workspace `.gitignore`；Workspace root 里的其他用户文件不参与 Guard。任何 dirty path 都会让 Ingest 在开始前返回 `WORKSPACE_DIRTY`。
17. run 开始后的外部修改不直接失败；Commit 阶段用 touched Resource CAS 决定是否接受。Git 状态处于 merge、rebase、cherry-pick，或 touched page 有 unresolved conflict marker 时，Ingest/Restore 拒绝执行。
18. `RevisionManifest` 的 canonical 序列化固定为 UTF-8、resource path 字节序排序、fixed field order 和无多余空白；`manifest_id = SHA-256(canonical_json)`。

**完成标准**

- `init` 能创建合法项目。
- root 已有 `.git` 时 `init` 返回 `REPOSITORY_EXISTS`，且不改动该 repository。
- 非空目录 `init` 返回 `ROOT_NOT_EMPTY`，目录内容不变。
- fresh repository 的当前 branch 是 `main`，不受机器全局 `init.defaultBranch` 影响。
- `init` 后 `wiki/` 与 `raw/` 可提交，`.wiki-db/` 不出现在 Git status。
- `init` 创建 baseline commit；root 里的 unrelated files 保持 untracked 或 ignored，不被自动 add。
- clean Workspace 的 `snapshot` 返回 `already_clean` 和当前 HEAD hash，不创建 empty commit。
- Snapshot capture 后外部修改时，返回的 Snapshot 内容仍等于 captured manifest，并带 `workspace_changed_after_capture=true`。
- Restore 前自动创建 pre-restore Snapshot；switch 前 manifest 变化时返回 `RESTORE_CONFLICT` 且不覆盖新修改。
- 相同 Resource set 的 Manifest ID 稳定；resource 顺序变化不改变 ID。
- 活进程持有 Workspace Lock 时，第二个进程收到 `WORKSPACE_BUSY`；崩溃锁 takeover 后自动先 Recovery。
- dirty tracked `wiki/raw` 时，`status` 显示 dirty paths，`ingest` 在调用 LLM 前返回 `WORKSPACE_DIRTY`。
- touched Resource 在 run 期间变化时，Commit 返回 `RESOURCE_CONFLICT`，不写任何 Draft 文件。
- run 期间修改 untouched Resource 不被 Commit 覆盖。
- 重复导入同一文件能命中 SHA-256 去重。
- 用户在磁盘上修改一个 page 后，下一次 mutating/open 操作能检测 hash 变化并刷新索引；query/lint 能通过 manifest 观察当前内容。
- 单元测试覆盖原子写入、非法路径、frontmatter 校验和重复 SHA。
- 集成测试覆盖 dirty Ingest 被阻塞、touched Resource CAS 冲突、untouched Resource 并发修改、Snapshot capture 后外部修改、Restore conflict、restore journal replay、apply 失败恢复、崩溃后 automatic recovery、commit 已落盘但 SQLite 未更新的 reconciliation。
- 位于外层仓库内部时，引擎不读取或不操作外层 repository；merge/rebase 状态和 remote operation 被拒绝。
- 手动破坏一个 draft 后，提交接口返回可读错误且不污染 Wiki。

### S3. Agent 适配层与 Workflow Kernel

**目标**：隔离 LLM 和 Rig API，并用小型 typed workflow kernel 驱动所有 LLM 业务流。

**工作内容**

Provider 接口：

```rust
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, AgentError>;
}
```

- `RigLlmClient`：真实 Provider。
- `FakeLlmClient`：返回固定结构，用于单测、集成测试和 CI。

Workflow kernel：

```rust
pub enum WorkflowNode {
    Analyze,
    ValidateAnalysis,
    RepairAnalysis,
    RetrieveContext,
    GenerateDraft,
    ValidateDraft,
    RepairDraft,
    SelfAudit,
    Commit,
}

pub enum Transition<S> {
    Next { state: S, node: WorkflowNode },
    Retry { state: S, reason: String },
    Reject { reason: String },
    Done { state: S },
}

#[async_trait]
pub trait WorkflowNodeHandler<S> {
    async fn run(&self, ctx: &mut WorkflowContext<S>) -> Result<Transition<S>, WorkflowError>;
}
```

Kernel 只提供五件事：typed state、node execution、attempt counter、deterministic transition、audit persistence。它不提供自然语言 planner，也不允许 LLM 输出任意 next node。

Audit ledger 至少记录：

```rust
pub struct WorkflowAuditEvent {
    pub event_id: EventId,
    pub run_id: RunId,
    pub node: WorkflowNode,
    pub attempt: u32,
    pub status: AttemptStatus,
    pub input_manifest_id: Option<ManifestId>,
    pub output_ref: Option<ArtifactRef>,
    pub prompt_template_id: Option<PromptTemplateId>,
    pub prompt_hash: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub latency_ms: Option<u64>,
    pub tokens_in: Option<u64>,
    pub tokens_out: Option<u64>,
    pub decision: Option<String>,
    pub error_code: Option<String>,
}
```

Large prompt/response artifacts 保存到 `.wiki-db/audit/<run-id>/`，SQLite 只保存引用和元数据。所有日志、错误和 audit 都必须 redact API key。

MVP 的 loop 只有两个 bounded repair loop：

```text
analyze -> validate_analysis
validate_analysis --invalid, attempt < 2--> repair_analysis
validate_analysis --valid--> retrieve_context
retrieve_context -> generate_draft
generate_draft -> validate_draft
validate_draft --invalid, attempt < 2--> repair_draft
validate_draft --valid--> self_audit
self_audit --pass--> commit
self_audit --reject--> failed
```

`validate_analysis`、`validate_draft` 和 `self_audit` 在 MVP 中是确定性 validator；self-audit 不引入额外 LLM reviewer。

MVP 只配置一个 OpenAI-compatible Provider，不要求多 Provider 切换 UI。配置保存在用户级 app config 目录，包含 `base_url`、`model` 和可选 `api_key`；文件权限限定为当前用户。环境变量提供的 API key 优先级最高，Workspace 目录不保存 key。Git identity 也保存在用户级配置：`git_author_name`、`git_author_email` 默认读取全局 Git config，缺失时使用 `CorpusBot <corpusbot@local.invalid>`。

**实现路径**

1. `corpusbot-agent` 对外仍只暴露 `analyze_source()`、`generate_drafts()`、`answer_question()` 三个业务方法。
2. 在 `corpusbot-agent` 内实现 workflow kernel；不新增 `graph-flow`、Rust LangGraph 或 Python LangGraph sidecar 依赖。
3. Rig 只出现在 `RigLlmClient`；业务 workflow 使用 `LlmClient`、typed state 和 deterministic transition。
4. API key 优先读环境变量；用户级配置文件不能进入 Git，也不能被 Workspace 同步带走。
5. 所有期望 JSON 的请求都做 typed serde 校验；失败进入 bounded repair attempt，不执行无限 loop。
6. 为每个 node attempt 写入 audit event；provider 调用前记录 request intent，成功或失败后记录 output、validation decision 和 transition。
7. 使用 fake client 固定 LLM 返回，验证成功、非法 JSON、超时、空输出、schema mismatch、audit persistence 和 attempt exhaustion。

**完成标准**

- Ingest 和 Query 不直接依赖 Rig 类型。
- workflow transition 全部由 Rust 代码决定；没有 LLM 产生的 next node。
- 每个 LLM attempt 可通过 `run_id` 查到输入引用、输出引用、校验结果和最终 transition。
- FakeLlmClient 可以驱动完整业务测试。
- repair loop 超过上限后返回 `ATTEMPTS_EXHAUSTED`，不继续调用 LLM。
- 真实 Provider 只通过一个薄适配器接入。

### S4. 单文件 Ingest 闭环

**目标**：完成设计稿中的两步 CoT，并保证 LLM 输出进入 Wiki 前被校验。

**工作内容**

Ingest 入口：

```bash
corpusbot ingest --root <dir> --file <markdown>
```

流程：

```text
run automatic recovery
  -> run Ingest Guard:
       no pending recovery
       safe Git state
       tracked wiki/raw clean
  -> capture baseline revision manifest
read file
  -> compute SHA-256
  -> exact-content dedupe
  -> copy to raw/
  -> step 1: analyze source
  -> validate analysis
  -> resolve page identities
  -> retrieve related wiki pages
  -> step 2: generate drafts
  -> validate drafts
  -> deterministic self audit
  -> build touched resource set
  -> acquire Workspace Lock
  -> recheck touched resource revisions
  -> commit transaction
  -> update search index
```

Step 1 的结构化分析至少包含：

```json
{
  "title": "string",
  "summary": "string",
  "entities": [
    {
      "name": "string",
      "page_type": "entity",
      "aliases": ["string"]
    }
  ],
  "concepts": [
    {
      "name": "string",
      "definition": "string"
    }
  ],
  "relations": [
    {
      "from": "string",
      "to": "string",
      "relation": "string",
      "evidence_quote": "string"
    }
  ],
  "claims": [
    {
      "text": "string",
      "source_quote": "string",
      "confidence": "high | medium | low"
    }
  ]
}
```

Step 2 输入：

- Step 1 的 JSON。
- 当前模板 schema。
- 相关 Wiki 页面在 baseline manifest 中的标题、frontmatter和节选。
- 原始 Markdown。

Step 2 输出 drafts：

- 一个 source page；Source Page 的路径或展示名必须能区分同 title 的不同 Source Version。
- 零到多个 entity page。
- 零到多个 concept page。
- touched resource set：每个现有目标 page 带 expected revision，新 page 带 `absent`。
- `wiki/index.md` 与 `wiki/log.md` 不交给 LLM 生成；Commit 锁内由引擎根据当前状态和 Draft 确定性重算/追加。

**实现路径**

1. MVP 使用进程内串行执行，不做队列持久化。
2. 每个 run 生成 `run_id`，draft 全部隔离在 `.wiki-db/drafts/<run-id>/`。
3. Ingest Guard 在读取 source file 和调用 LLM 之前执行。dirty tracked `wiki/raw` 返回 `WORKSPACE_DIRTY`、dirty path 列表和 `corpusbot snapshot` 提示；引擎不自动 Snapshot 用户修改。
4. Guard 通过后捕获 baseline `RevisionManifest`；后续 LLM 输入只能来自 manifest 记录的页面内容。
5. 路由规则由模板决定：
   - entity -> `wiki/entities/`
   - concept -> `wiki/concepts/`
   - source -> `wiki/sources/`
   - research extension -> `wiki/thesis/`、`wiki/methodologies/`、`wiki/findings/`
6. 用 Page Identity resolver 决定 create 还是 update。已存在 entity/concept 采用 append-only merge：
   - 引擎 union `tags`、`related`、`sources`。
   - LLM 只生成一个 `## From <Source Title>` 归属 section。
   - 已有正文、标题和用户补充字段保持不变。
   - 相似但规范化名称不同的实体不自动合并，创建新页并输出 `POSSIBLE_DUPLICATE`。
7. Draft 声明 touched Resource set：
   - 现有 source/entity/concept page 带当前 content hash。
   - 新 page expected revision 为 `absent`。
   - index/log 由 store 作为 derived resource generation 管理。
8. 提交前做安全检查：
   - 所有目标路径都在 `wiki/` 内。
   - frontmatter 满足模板要求。
   - 新 source page 的 `sources` 包含当前来源。
   - update draft 不修改已有正文 section，只允许新增归属 section 和引擎 union 后的 frontmatter。
   - LLM 不能改写 `raw/`、`.wiki-db/` 和 `wiki/log.md` 之外的保留文件格式。
9. 在 Workspace Lock 内完成 touched Resource CAS recheck；任一 mismatch 返回 `RESOURCE_CONFLICT`，整个 Draft 不写入。
10. ingest 成功后立即触发增量 Tantivy 索引，并递增 search index generation。

**完成标准**

- 一个 Markdown 样本可以从 CLI 完整导入。
- dirty tracked workspace 时 Ingest 不读取来源文件、不调用 LLM、不创建 drafts，只返回 guard error。
- 生成页面的 frontmatter、Wikilink 和来源引用通过校验。
- touched Resource 变化时返回 `RESOURCE_CONFLICT`；untouched page 同时变化时不被覆盖。
- 第二个来源命中同一 entity/concept 时，旧正文不变，新增 section 和 source 引用。
- 修改后的文件创建新的 Source Version，不影响旧 Source Page。
- FakeLlmClient 集成测试覆盖成功、重复导入、非法 JSON、draft 校验失败。
- 真实 LLM 手工测试至少通过一个中文样本和一个英文样本。

### S5. Search 与 Query

**目标**：提供可验证的 BM25 检索和带引用的 LLM 回答。

**工作内容**

实现 CLI：

```bash
corpusbot query --root <dir> --question "..."
```

MVP 检索流程：

```text
question
  -> reject pending recovery
  -> capture revision manifest
  -> normalize
  -> Tantivy BM25 top-k
  -> load manifest-recorded wiki pages
  -> trim to context budget
  -> LLM answer
  -> validate citations
  -> return Answer
```

MVP 固定参数：

- 默认 `top_k = 8`。
- 默认上下文预算按字符数控制，先使用 24,000 字符，后续再按 token 精确计算。
- 回答必须使用编号引用。
- 不做 LLM keyword expansion、local substring fallback、PPR 图扩展。

Tantivy schema：

| 字段         | 存储 | 索引    |
| ------------ | ---- | ------- |
| `path`       | yes  | keyword |
| `title`      | yes  | text    |
| `page_type`  | yes  | keyword |
| `tags`       | yes  | keyword |
| `body`       | yes  | text    |
| `updated_at` | yes  | date    |

返回结构：

```rust
pub struct QueryAnswer {
    pub answer: String,
    pub citations: Vec<Citation>,
    pub revision_manifest_id: ManifestId,
}

pub struct Citation {
    pub number: u32,
    pub path: WikiPath,
    pub title: String,
    pub quote: String,
    pub resource_revision: Revision,
}
```

**实现路径**

1. Query 和 Lint 不获取 Workspace Lock；但 pending Recovery 时返回 `RECOVERY_PENDING`，不读取 partial transaction。
2. 通过 recovery check 后捕获 `RevisionManifest`，记录每个页面 content hash 和当前 search index generation。
3. Tantivy index 存放在 `.wiki-db/tantivy/`。
4. `rebuild` 用于首次初始化或索引损坏恢复。
5. 每次 ingest 写入新的 immutable Tantivy generation directory，再原子切换 current-generation pointer；Query 在 manifest capture 时打开 generation，并在整个 query 内继续使用该 generation。
6. 检索先用 Tantivy 找候选，再过滤到 `RevisionManifest` 中仍然存在的合法页面；页面正文从 manifest 记录的 revision 读取。
7. 上下文拼装按相关性排序，并保留每页路径、标题和 revision。
8. LLM 回答后校验引用编号、quote 和 revision；编号无效、页面不在 manifest、或归一化空白后 quote 不能在该 manifest revision 中精确找到时，从 answer metadata 中剔除并输出 warning。
9. 所有引用都被剔除时返回 `INSUFFICIENT_WIKI_EVIDENCE`，不返回无引用结论。

**完成标准**

- 中文和英文查询都能命中相关页面。
- 回答中的每个编号引用都有合法路径。
- 每条引用的 quote 能在目标页面中找到。
- Query result 暴露 `revision_manifest_id`；query 运行期间外部修改不会改变已读取的页面内容。
- 引用全被剔除时不输出结论，只输出证据不足状态。
- 空索引时返回友好的“没有检索到相关 Wiki 页面”。
- 单元测试覆盖索引 upsert、重建、top-k、manifest capture、外部修改下的 manifest read 和引用校验。

### S6. 基础 Lint

**目标**：先建立确定性兜底，防止明显坏数据长期留在 Wiki。

**工作内容**

实现 CLI：

```bash
corpusbot lint --root <dir> [--format table|json]
```

MVP 规则：

| Code                        | 级别    | 说明                                                                   |
| --------------------------- | ------- | ---------------------------------------------------------------------- |
| `DEAD_LINK`                 | error   | Wikilink 解析不到 `wiki/` 下的页面                                     |
| `ORPHAN_PAGE`               | warning | 除 `index.md` 和 `log.md` 外，没有任何入边                             |
| `MISSING_FRONTMATTER_FIELD` | error   | 缺少核心必填字段                                                       |
| `INVALID_PAGE_TYPE`         | error   | 页面类型不在当前模板允许范围内                                         |
| `INVALID_DATE`              | error   | `created` 或 `updated` 无法解析                                        |
| `INDEX_DRIFT`               | warning | `index.md` 缺少页面、引用不存在页面或不匹配 canonical order            |
| `POSSIBLE_DUPLICATE`        | warning | 新建 Page 的规范化名称与已有名称高度相似，但没有命中同一 Page Identity |

Lint report 结构：

```json
{
  "generated_at": "2026-09-07T00:00:00Z",
  "template": "research",
  "revision_manifest_id": "<manifest-id>",
  "summary": {
    "pages": 18,
    "errors": 2,
    "warnings": 4
  },
  "issues": [
    {
      "code": "DEAD_LINK",
      "severity": "error",
      "path": "wiki/concepts/vector-search.md",
      "message": "[[Embedding Index]] cannot be resolved",
      "fix_hint": "Create the page or link to an existing entity"
    }
  ]
}
```

**实现路径**

1. Query 和 Lint 不获取 Workspace Lock；但 pending Recovery 时返回 `RECOVERY_PENDING`，不读取 partial transaction。
2. 通过 recovery check 后捕获 `RevisionManifest`，遍历 manifest 记录的 `wiki/**/*.md` 内容，解析 frontmatter 和 Wikilink。
3. 先构建 `path/title/alias -> page` 解析表，再检查每条链接。
4. 孤儿页统计排除 `index.md` 和 `log.md`。
5. Index canonical order 固定为：先按目录/页面类型分组，再按 title 做稳定 Unicode 排序。ingest 每次在 Commit 锁内重新生成整份 index。
6. 先输出结构化 JSON，再渲染 human-readable table；JSON 包含 `revision_manifest_id`。
7. MVP 不自动修复；后续 Smart Fix 必须复用 `WikiTx` 和 touched Resource CAS。
8. Lint 是报告，不是硬门禁：已有 error 不阻塞 ingest/query，但桌面端必须显式展示。

**完成标准**

- 四类 fixtures 分别触发死链、孤儿页、缺字段和 Index 漂移。
- JSON report 可被桌面端直接消费。
- Lint report 暴露 `revision_manifest_id`；lint 运行期间外部修改不会改变已读取的页面内容。
- 存在 error 的 Workspace 仍然可以执行 query；引用全被剔除时返回证据不足。
- Lint 本身不写 Wiki 文件。

### S7. Tauri 桌面最小界面

**目标**：让桌面端复用同一个引擎服务层，完成可视 MVP 闭环。

**页面范围**

- Workspace：选择或创建项目。
- Wiki：文件树、Markdown 预览、导入入口。
- Chat：问题输入、回答、引用列表。
- Lint：健康摘要和问题列表。
- History：Snapshot ID、Manifest ID、时间、来源和 Restore 操作。
- Workspace：打开时自动执行 pending Recovery，并显示 Recovery、Dirty Workspace 或 unsafe Git state。
- Settings：Provider base URL、模型名、API key、Git author name/email 的用户级配置入口；界面和日志都不能回显完整 key。

三栏主界面：

```text
icon sidebar | wiki file tree | chat + markdown preview
```

Tauri commands：

```rust
init_workspace(root: PathBuf, template: TemplateId) -> WorkspaceSummary;
open_workspace(root: PathBuf) -> WorkspaceSummary;
workspace_status(root: PathBuf) -> WorkspaceStatus;
list_wiki_pages(root: PathBuf) -> Vec<WikiPageSummary>;
read_wiki_page(root: PathBuf, path: WikiPath) -> WikiPage;
ingest_file(root: PathBuf, file_path: PathBuf) -> IngestResult;
query(root: PathBuf, question: String) -> QueryAnswer;
run_lint(root: PathBuf) -> LintReport;
create_snapshot(root: PathBuf, message: String) -> SnapshotSummary;
list_snapshots(root: PathBuf) -> Vec<SnapshotSummary>;
restore_snapshot(root: PathBuf, snapshot_id: String, confirmed: bool) -> RestoreResult;
get_settings(root: PathBuf) -> SettingsSummary;
save_settings(root: PathBuf, settings: SettingsInput) -> SettingsSummary;
```

**实现路径**

1. Tauri commands 只做参数转换和错误映射，业务逻辑留在 Rust crate。
2. 前端用 Zustand 管理 workspace、pages、chat、lint 状态。
3. Markdown 预览支持普通 Markdown 和 Wikilink；未实现页面点击时显示提示。
4. Chat 显示回答文本和编号引用列表。
5. Ingest 期间显示 loading 和最终结果，MVP 不做实时 token 级进度。
6. Settings 保存 API key 到用户级私有配置；Workspace、仓库和日志不保存明文 key。
7. 用户直接编辑 Wiki Page 后，返回 Wiki 视图或执行任一引擎操作时刷新 page hash、Index 和 Tantivy 输入；UI 不覆盖用户修改。
8. Workspace 顶栏显示 `Clean`、`Dirty`、`Recovery Pending`、`Recovery Running`、`Locked` 或 `Blocked`，并展示 `head_snapshot_id`。Dirty 状态列出 tracked paths，并提供“Create Snapshot”入口；不提供“Continue Anyway”。
9. History 页面只展示 message、time、run/source 摘要和 Restore；MVP 不做 inline diff viewer。Restore 恢复整个 `wiki/raw` scope，需要二次确认，并提示会先自动保留当前状态。
10. Snapshot 完成后显示 `manifest_id`；如果 `workspace_changed_after_capture=true`，提示 Workspace 在 capture 后又变化，但当前 Snapshot 仍是一致 point-in-time。
11. `RESTORE_CONFLICT` 显示 current/expected manifest 摘要，并引导用户先创建新 Snapshot 后重试；`WORKSPACE_BUSY` 显示 lock owner summary 和 started_at。不提供强行解锁。崩溃后的 stale lock 由下一次 engine 操作检测并 takeover。
12. Git merge/rebase/cherry-pick 状态显示为 blocked，并解释需要先在外部 Git 客户端处理。
13. 前端验证使用 Playwright 或应用内浏览器截图：

- 桌面宽度三栏不重叠。
- 移动或窄窗口下布局不溢出。
- 空 Wiki、加载中、导入失败、Git blocked、Restore 确认和 Lint 报错均有可见状态。

**完成标准**

- 用户可以不使用终端完成状态检查、导入、提问、查看 Lint、查看 History 和 Restore。
- 前后端错误都显示可读文案，而不是白屏或裸 panic。
- 长标题、空页面、无检索结果、LLM 失败都有稳定 UI 状态。
- 本地浏览器或 Playwright 检查通过主要界面截图。

### S8. 集成验证与收尾

**目标**：用固定样本验证 MVP，不依赖单点手工感觉。

**工作内容**

准备测试样本：

1. 五到十个中文/英文研究笔记组成演示 Wiki。
2. 与已有 entity/concept 部分重叠的第二个来源。
3. 规范化名称相同但大小写/空白不同的 Page。
4. 重复导入的相同文件。
5. 修改后重新导入的文件。
6. 用户手工编辑过的 Wiki Page。
7. 含死链、缺字段、孤儿页和 Index 漂移的坏 Wiki。
8. dirty tracked workspace、apply 中途失败、进程崩溃后的 applying run、commit 已落盘但 SQLite 状态落后的 run、已有后续修改的 Snapshot。
9. root 已有 `.git` 的路径。
10. 非空 root、root 已有 `.git`、全局不同 `init.defaultBranch`。
11. 活进程 Workspace Lock 和崩溃后的 stale lock。
12. Ingest run 期间修改 touched page、修改 untouched page、query/lint 运行期间外部修改。
13. Snapshot capture 后修改 resource、Restore staging 后修改 resource、restore journal 中断。

集成测试：

| 场景                          | 断言                                                                            |
| ----------------------------- | ------------------------------------------------------------------------------- |
| init -> lint                  | 新项目没有 error                                                                |
| init existing repository      | `REPOSITORY_EXISTS`，已有 repo 内容不变                                         |
| init non-empty root           | `ROOT_NOT_EMPTY`，目录内容不变                                                  |
| fresh branch                  | repository 当前 branch 是 `main`                                                |
| clean snapshot                | `already_clean`，返回当前 HEAD full hash，不新增 commit                         |
| snapshot after capture change | Snapshot 内容等于 manifest，`workspace_changed_after_capture=true`              |
| concurrent workflow           | 第二个 mutating workflow 收到 `WORKSPACE_BUSY` 和 lock owner summary            |
| stale lock recovery           | takeover 后先执行 pending Recovery，再开放 mutation                             |
| resource CAS conflict         | touched page 修改后 `RESOURCE_CONFLICT`，无 partial write                       |
| untouched concurrent edit     | untouched page 的外部修改保留，Draft scope 正常提交                             |
| read revision manifest        | query/lint 内容与 manifest 一致，report/answer 包含 manifest id                 |
| ingest five-to-ten samples    | source/entity/concept drafts 顺序提交成功                                       |
| append-only merge             | 已有 entity/concept 正文不变，新增归属 section                                  |
| duplicate ingest              | SHA-256 命中，不重复创建 source page                                            |
| changed source ingest         | 新 Source Version 创建，旧 Source Page 保留                                     |
| user edit                     | hash 派生状态刷新，用户内容不被覆盖                                             |
| dirty ingest guard            | `WORKSPACE_DIRTY` 在 LLM 调用前返回；explicit Snapshot 后 ingest 才继续         |
| git apply failure             | touched paths 从 baseline 还原，当前 touched content 先复制到 recovery backup   |
| git crash recovery            | applying run 在下次打开时自动 recovery，Wiki 没有 partial commit                |
| git commit reconciliation     | run commit 已存在时，recovery 标记 committed 而不是还原                         |
| git restore                   | 当前 `wiki/raw` 先 Snapshot，目标 Snapshot 的完整 scope 恢复；root 其他文件不变 |
| restore conflict              | staging 后 manifest 变化时 `RESTORE_CONFLICT`，新修改不被覆盖                   |
| restore journal               | 中断后 recovery 完成或安全回退，不留下混合 scope                                |
| rebuild index -> query        | 能命中样本核心结论                                                              |
| query citation                | 引用编号对应真实页面，quote 精确匹配                                            |
| ten-question evaluation       | 至少八题通过 citation validation                                                |
| lint broken wiki              | 四类 issue 全部出现                                                             |
| desktop e2e                   | 导入、阅读、提问、Lint 均可操作                                                 |

收尾任务：

- 更新 README 的安装、配置和端到端演示步骤。
- 补充 MVP 已知限制。
- 清理 TODO、dead code 和临时 fixture。
- 跑完整质量门禁。

**完成标准**

- 全部自动测试通过。
- 一次真实 LLM 桌面端演示成功。
- README 足以让新用户复现闭环。

## 4. 建议实施顺序

```text
S0 工程基线
  -> S1 core 类型和模板
  -> S2 store / init / transaction
  -> S3 agent 适配层
  -> S4 ingest
  -> S5 search / query
  -> S6 lint
  -> S7 desktop UI
  -> S8 integration
```

可并行点：

- S1 core 类型与 S7 静态界面骨架可以并行。
- S3 agent adapter 与 S5 search schema 可以并行。
- S6 lint 只依赖 S1/S2，可以在 S4 稳定后并行收尾。

## 5. MVP 里程碑

| 里程碑 | 内容                 | 验收演示                                                         |
| ------ | -------------------- | ---------------------------------------------------------------- |
| M0     | 工程可编译           | `cargo test` 和前端页面可用                                      |
| M1     | 项目可创建和读取     | CLI init + store 读写通过                                        |
| M2     | 单文件可入库且可恢复 | CLI ingest 生成页面，失败可 touched-path restore                 |
| M3     | 知识可问答           | CLI query 返回带引用答案                                         |
| M4     | 健康可检查           | CLI lint 输出结构化报告                                          |
| M5     | 桌面端闭环           | UI 状态检查、导入、阅读、提问、Lint、History 和 Restore 全部可用 |

## 6. 关键技术决策

| 决策                 | MVP 选择                                                            | 原因                                                         |
| -------------------- | ------------------------------------------------------------------- | ------------------------------------------------------------ |
| 第一用户             | 个人研究者，桌面为主                                                | 先验证本地知识编译和阅读问答价值                             |
| 输入格式             | 只验收 Markdown                                                     | 先验证知识编译闭环，文档解析不阻塞主线                       |
| Source Version       | 内容 hash 寻址，修改即新版本                                        | 保持来源不可变和可追溯，先不解决 claim 版本链                |
| Page Identity        | NFKC + trim + 空白折叠 + casefold 精确匹配                          | 避免大小写/空白噪声，也避免语义误合并                        |
| 已有页 merge         | frontmatter union + 追加来源归属 section                            | 防止第二次 ingest 重写或删除用户/旧来源内容                  |
| Wiki 权威版本        | 当前磁盘文件                                                        | 保证 Obsidian 和手工编辑是一等工作流                         |
| 模板                 | `generic` + `research`                                              | 覆盖通用和目标研究场景，避免一开始维护六套模板               |
| LLM 接入             | 只接一个用户级 OpenAI-compatible Provider                           | 降低 MVP 配置面；Rig adapter 保留扩展点                      |
| Workflow kernel      | `corpusbot-agent` 内 typed node/bounded attempt/audit ledger        | 覆盖 LLM retry 和审计需求，同时避免引入年轻 graph framework  |
| 队列                 | 进程内串行                                                          | 单文件 MVP 不需要崩溃恢复队列                                |
| 检索                 | Tantivy BM25                                                        | 满足最小问答；PPR 留给 Alpha                                 |
| 版本管理             | Workspace root 全新本地 Git repo，只跟踪 wiki/raw                   | 避免接管用户工程历史；人工编辑、Ingest 和恢复共享一份历史    |
| Snapshot ID          | 当前 HEAD 的 full commit hash；clean snapshot no-op                 | 当前版本始终可追踪，不制造 empty commit                      |
| Snapshot consistency | manifest + captured content，不混合 worktree 读取                   | 外部修改时 Snapshot 仍是可解释 point-in-time                 |
| Restore safety       | pre-restore Snapshot + staging + manifest recheck + journal         | 防止覆盖并发修改，也保证中断后可恢复                         |
| 并发                 | `.wiki-db/workspace.lock` 串行 mutation/recovery                    | 防止桌面端和 CLI 同时改变 Workspace 事务状态                 |
| Ingest workflow      | 自动 Recovery + clean tracked workspace guard                       | 防止中断事务遗留，也防止用户 dirty work 被混入 Ingest commit |
| 事务                 | draft + clean-tree guard + baseline Snapshot + touched-path restore | 防止 partial commit，同时避免自定义 journal 和破坏性 reset   |
| 并发模型             | touched Resource CAS + short Workspace Lock + read manifest         | 长时间 LLM 生成不阻塞读；多页事务不会 partial commit         |
| History              | 整包列表 + 显式 Restore，不做 diff/page restore                     | 提供最小可理解版本管理，控制 MVP 成本                        |
| Git identity         | 用户级配置，默认全局 Git config，最后 fallback CorpusBot            | 保证本地 commit 可创建且身份可解释                           |
| Citation             | 页面 + 可精确定位的 quote                                           | 让回答可以被人类验证，限制模型先验                           |
| Lint                 | 报告但不是硬门禁                                                    | 兜底发现问题，同时不阻断研究工作流                           |
| 前端状态             | Zustand                                                             | 轻量，覆盖 MVP 状态面                                        |

## 7. 主要风险与处理

| 风险                             | MVP 处理                                                                                            |
| -------------------------------- | --------------------------------------------------------------------------------------------------- |
| Rig API 变动                     | 业务层只依赖 `LlmClient`，Rig 细节隔离在 `corpusbot-agent`                                          |
| LLM 输出非法 JSON                | Step 1/Step 2 都使用 typed schema 校验，失败最多重试一次                                            |
| LLM 返回导致状态机失控           | transition 只由 Rust validator/workflow policy 决定；LLM 不能输出 next node                         |
| LLM 调用不可追溯                 | 每个 node attempt 写入 audit event 和 large artifact 引用，prompt hash/model/provider/decision 可查 |
| LLM 幻觉或写错页面               | LLM 只能产 drafts；已有页 append-only merge，提交前校验路径、frontmatter、Wikilink 和来源           |
| 中文检索质量差                   | 选择支持 CJK 的 Tantivy tokenizer，并用中英文样本验收                                               |
| 引用看似存在但不支持答案         | quote 必填，并在目标页面做归一化精确匹配                                                            |
| 用户手工编辑被覆盖               | 磁盘 Wiki 是权威版本；引擎只刷新派生状态，不回写用户正文                                            |
| 相似概念被误合并                 | Page Identity 只做规范化精确匹配；疑似重复只 warning                                                |
| Tauri IPC 错误不清晰             | commands 返回结构化 error code/message，前端统一渲染                                                |
| API key 泄漏                     | key 只存本地私有配置，日志 redact，配置文件不进 Git                                                 |
| Wiki 路径逃逸                    | `WikiPath` 统一 resolve 和校验，store 拒绝 `wiki/` 以外的写入                                       |
| Ingest 中途失败                  | 下次打开自动 Recovery；commit 已存在则 reconcile，否则只还原 touched paths                          |
| 用户 dirty work 混入 Ingest      | Ingest Guard 在 LLM 前失败，要求显式 Snapshot；不提供 continue anyway                               |
| 自动 Recovery 覆盖并发修改       | 只处理 applying run 的 touched paths；恢复前先把当前 touched content 复制到 engine-private backup   |
| 外部 Git 状态冲突                | merge/rebase/cherry-pick 或 conflict marker 时拒绝 Ingest/Restore                                   |
| 已有 repo 被误接管               | `init` 遇到 root `.git` 返回 `REPOSITORY_EXISTS`，不做任何改动                                      |
| 桌面端和 CLI 并发破坏事务        | Workspace Lock 串行化 mutation/recovery，活锁返回 `WORKSPACE_BUSY`                                  |
| LLM run 覆盖并发修改             | Draft 记录 touched Resource expected revision；Commit 锁内 recheck，任一冲突拒绝整个 Draft          |
| Query/Lint 读到混合版本          | 操作开始捕获 Revision Manifest，读取和引用校验都绑定 manifest                                       |
| Snapshot 覆盖 capture 后的新修改 | Snapshot 内容冻结在 manifest；只标记 `workspace_changed_after_capture`，不自动重读                  |
| Restore 覆盖并发修改             | pre-restore Snapshot 保留现状；切换前 manifest recheck，冲突时 `RESTORE_CONFLICT`                   |
| Git API 误用导致破坏性操作       | Git 细节隔离在 corpusbot-vcs，禁止 reset --hard、branch merge 和 remote operation                   |

## 8. Definition of Done

MVP 的最终完成定义是：

1. Rust 全量质量门禁通过。
2. CLI 八命令在 fixture Workspace 上全部成功。
3. 桌面端能完成创建、状态检查、导入、阅读、问答、Lint、History 和 Restore。
4. 五到十个样本的演示 Wiki 达到“10 题至少 8 题可验证引用”的标准。
5. 真实 LLM 和 FakeLlmClient 两条链路都有验证。
6. 用户手工编辑、dirty guard、Resource CAS conflict、untouched concurrent edit、Revision Manifest read、fresh repository policy、clean snapshot no-op、Workspace Lock、Snapshot point-in-time、Restore conflict/journal、workflow audit/bounded repair/attempt exhaustion、相似页、修改后 Source Version、Git 失败/崩溃/commit reconciliation、非法 LLM 输出、非法路径、重复导入均有测试。
7. README、MVP 已知限制和下一步 Alpha 范围已经写清。
