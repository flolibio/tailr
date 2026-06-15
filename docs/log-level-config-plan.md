# 日志级别可配置化 - 开发计划

> 设计文档: [docs/design/log-level-config.md](design/log-level-config.md)  
> 开始日期: 2026-06-12  
> 预估工期: 5 个工作日  
> 审核更新: LevelDetector → search-engine, 正则延后, arc-swap  
> 配置集成: 复用 config.toml (figment), 不再使用独立 JSON 文件

## 阶段划分

### Phase 1: 后端基础 (Day 1-2)

**Day 1: 数据模型 & 配置存储**

| 任务 | 文件 | 说明 |
|------|------|------|
| 定义 `LevelDef` 和 `LogLevelConfig` 结构体 | `crates/protocol/src/types.rs` | 新增结构体，保留原有 `LogLevel` 枚举 |
| 扩展 `Config` 结构体 | `src/config.rs` (已有) | 新增 `log_levels: Option<LogLevelConfig>` 字段 |
| 实现 `write_config()` | `src/config.rs` | 序列化 Config → TOML → 覆盖写入 |
| 实现预设数据 + 默认配置 | `src/config.rs` | `default_log_levels(preset)` 函数 |
| 新增 `arc-swap` 依赖 | `Cargo.toml` | 热更新用无锁读 |

**Day 2: 动态检测器 & API**

| 任务 | 文件 | 说明 |
|------|------|------|
| 实现 `LevelDetector`（关键词匹配） | `crates/search-engine/src/detector.rs` (新建) | 关键词匹配，零正则依赖 |
| GET `/api/config/log-levels` | `crates/server/src/api.rs` | 返回当前配置 |
| POST `/api/config/log-levels` | `crates/server/src/api.rs` | 保存配置 + 热更新检测器（arc-swap） |
| 修改 `AppState` | `crates/server/src/lib.rs` | 新增 `level_config` 和 `level_detector`（ArcSwap） |

### Phase 2: 后端集成 (Day 3)

| 任务 | 文件 | 说明 |
|------|------|------|
| `FileWatcher` 注入检测器 | `crates/tail-engine/src/watcher.rs` | 持有 `Arc<ArcSwap<LevelDetector>>`，`add()` 时传入 |
| `TailSession` 集成动态检测器 | `crates/tail-engine/src/session.rs` | `read_lines_from_offset()` 使用动态检测器 |
| 搜索 API 集成 | `crates/server/src/api.rs` | `search()` 使用动态检测器 |
| WebSocket 集成 | `crates/server/src/ws.rs` | 推送条目使用动态检测器 |
| 单元测试 | 各 crate | `LevelDetector` 完整覆盖 + 边界场景 |

### Phase 3: 前端配置 UI (Day 3-4)

**Day 3: 前端基础**

| 任务 | 文件 | 说明 |
|------|------|------|
| 定义 TypeScript 类型 | `src/services/api.ts` | `LogLevelConfig`, `LevelDef` 接口 |
| API 客户端 | `src/services/api.ts` | `getLogLevelConfig()`, `saveLogLevelConfig()` |
| 预设数据 + 颜色管理 | `src/composables/useLogLevels.ts` (新建) | 7 组预设 + 16 色色板 + 动态 CSS 变量 |

**Day 4: 配置页面 UI**

| 任务 | 文件 | 说明 |
|------|------|------|
| 全屏模态框骨架 | `src/components/settings/SettingsPage.vue` (新建) | 左侧导航 + 右侧内容区 |
| 外观设置迁移 | `src/components/settings/GeneralSettings.vue` (新建) | 从 SettingsPanel 迁移字号/主题/语言 |
| 日志级别配置面板 | `src/components/settings/LogLevelSettings.vue` (新建) | 预设选择 + 级别列表 + 拖拽排序 |
| 颜色选择器 | `src/components/settings/ColorPicker.vue` (新建) | 色板网格 + 当前选中高亮 |
| 集成到 App.vue | `src/App.vue` | 替换旧 SettingsPanel，设置按钮打开新模态框 |

### Phase 4: 前端集成 (Day 4-5)

| 任务 | 文件 | 说明 |
|------|------|------|
| 动态 CSS 变量 | `src/composables/useLogLevels.ts` | `applyThemeColors()` 动态设置 `--c-*-text/bg/border` |
| LogViewer 颜色适配 | `src/components/LogViewer.vue` | 移除硬编码 badge 颜色，改用动态 CSS 类 |
| App.vue 级别过滤适配 | `src/App.vue` | 使用配置中的级别列表替代硬编码 `allLevels` |
| FilterBar 级别标签适配 | `src/App.vue` | 使用配置中的颜色替代硬编码 `levelDotColors` |
| localStorage 持久化 | `src/composables/useLogLevels.ts` | 前端配置持久化 |

### Phase 5: 测试 & 收尾 (Day 5)

| 任务 | 说明 |
|------|------|
| 后端单元测试 | `LevelDetector` 完整覆盖 |
| 后端集成测试 | API 读写 + 热更新 |
| 前端组件测试 | 配置页面交互 |
| 端到端测试 | 前后端同步验证 |
| 文档更新 | README.md, CLAUDE.md 更新 API 文档 |
| 清理 | 移除旧 SettingsPanel，移除硬编码颜色 |

## 文件变更清单

### 新增文件

| 文件 | 说明 |
|------|------|
| `crates/search-engine/src/detector.rs` | 动态级别检测器（关键词匹配） |
| `src/components/settings/SettingsPage.vue` | 全屏配置页面 |
| `src/components/settings/LogLevelSettings.vue` | 级别配置 |
| `src/components/settings/ColorPicker.vue` | 颜色选择器 |
| `src/composables/useLogLevels.ts` | 预设数据 + 颜色管理 + 动态 CSS 变量 |
| `docs/design/log-level-config.md` | 设计文档 |

### 修改文件

| 文件 | 说明 |
|------|------|
| `Cargo.toml` | 新增 arc-swap 依赖 |
| `crates/protocol/src/types.rs` | 新增配置结构体 |
| `src/config.rs` | 扩展 Config + write_config + 预设数据 |
| `crates/server/src/lib.rs` | AppState 新增字段（ArcSwap + config_path） |
| `crates/server/src/api.rs` | 新增配置 API + 检测器集成 |
| `crates/server/src/ws.rs` | 检测器集成 |
| `crates/tail-engine/src/watcher.rs` | FileWatcher 注入检测器 |
| `crates/tail-engine/src/session.rs` | 检测器集成 |
| `src/App.vue` | 替换设置面板 + 级别过滤适配 |
| `src/components/LogViewer.vue` | 移除硬编码颜色 |
| `src/services/api.ts` | 新增配置 API |
| `src/style.css` | 新增配置页面样式 |

## 依赖关系

```
Phase 1 (后端基础)
    │
    ▼
Phase 2 (后端集成)  ←── Phase 3 (前端基础) [可并行]
    │                       │
    ▼                       ▼
    └──── Phase 4 (前端集成) ────┘
              │
              ▼
         Phase 5 (测试收尾)
```

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 动态检测器性能 | 大文件扫描变慢 | 关键词匹配用 `contains_case_insensitive`（零分配）；正则用预编译 |
| 前端颜色动态更新 | CSS 变量切换可能闪烁 | 一次性替换所有变量，不逐步更新 |
| 拖拽排序库引入 | 增加前端包体积 | 使用轻量级 `vuedraggable@next`（~10KB gzip） |
| 后端热更新竞态 | 并发读写配置 | 用 `arc-swap` 无锁读 + 原子替换 |

## GSTACK REVIEW REPORT

| Section | Status | Findings |
|---------|--------|----------|
| Architecture | ✅ PASS | config.toml 集成方案合理 |
| Code Quality | ✅ PASS (3 findings fixed) | #12 章节编号冲突 ✅, #13 use_regex 残留 ✅, #14 测试计划正则引用 ✅ |
| Tests | ✅ PASS | 测试计划已更新 |
| Performance | ✅ PASS | arc-swap 方案确认 |

**VERDICT: PASS** — 所有发现已修复，文档 ready for implementation。

**已解决的发现：**
- #1 LevelDetector 从 protocol 移至 search-engine（保持零依赖）
- #2 FileWatcher 注入路径已补充
- #5 正则支持延后至 Phase 2（UI 简化）
- #7 预设数据合并到 config.rs（单一数据源）
- #8 端到端测试场景需补充（预设切换+颜色保留、热更新验证、降级、并发）
- #9 LevelDetector 边界测试需补充（空行、超长行、大小写、多匹配、0级别）
- #11 RwLock → arc-swap（无锁读，适合读密集场景）
- #12 章节编号冲突（两个 6.1.2）→ 修正为 6.1.3
- #13 前端预设示例残留 use_regex → 已移除
- #14 测试计划提到正则匹配 → 已移除（延后至 Phase 2）
- D08 JSON 文件 → config.toml [log_levels] 节（figment 分层）
- D09 API 写入目标改为 config.toml（序列化覆盖写入）

**已知 tradeoff（无需行动）：**
- #3 前端动态 CSS 类：移除硬编码 badge 颜色，改用动态生成
- #6 颜色存储保留 color_light/color_dark（支持用户自定义颜色）

**NO UNRESOLVED DECISIONS**
