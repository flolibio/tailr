# 日志级别可配置化设计文档

> 版本: v1.2  
> 日期: 2026-06-13  
> 状态: 设计完成 + 工程审核通过，待开发  
> 审核更新: LevelDetector → search-engine, 正则延后, arc-swap  
> 配置集成: 复用 config.toml (figment), 不再使用独立 JSON 文件

## 1. 背景与动机

当前 tailr 的日志级别系统存在以下问题：

1. **硬编码**：`LogLevel` 枚举固定为 ALERT/ERROR/WARN/INFO/DEBUG/TRACE，无法扩展
2. **PHP 倾向**：ALERT 级别来自 PHP/syslog，非通用
3. **颜色硬编码**：CSS 变量、TypeScript 对象、scoped CSS 三处重复定义
4. **不可定制**：用户无法添加自定义级别（如 FATAL、CRITICAL、SUCCESS）

目标：让用户通过 Web UI 自行配置日志级别、检测关键词和颜色，同时提供常用语言的预设组。

## 2. 设计决策记录

| 编号 | 决策项 | 结论 |
|------|--------|------|
| D01 | 配置方式 | 完全可配置 + 预设组 |
| D02 | 预设组 | 通用、Java、Python、PHP、Go、Rust、syslog |
| D03 | 配置 UI | 专用配置页面（全屏模态框），左侧分类导航 |
| D04 | 预设切换 | 加载默认级别和颜色，用户已修改的颜色保留 |
| D05 | 自定义默认值 | 基础模板 ERROR/WARN/INFO/DEBUG |
| D06 | 检测规则 | 简单关键词匹配（正则延后至 Phase 2） |
| D07 | 检测优先级 | 拖拽排序，数组顺序 = 检测优先级 |
| D08 | 后端持久化 | 集成到 config.toml `[log_levels]` 节（figment 分层） |
| D09 | 前后端同步 | 点保存 → POST API → 后端热更新(arc-swap) + 序列化 Config → 覆盖写入 config.toml |
| D10 | 配置生效时机 | 只影响新检测的条目，已推送的不变 |
| D11 | 颜色选择 | 预设色板（深色/浅色各一套），不提供自定义取色 |
| D12 | LOG_PATTERN | 本次不做，未来加入 |
| D13 | 升级迁移 | 无配置时自动使用"通用"预设 |

## 3. 预设级别组定义

### 3.1 通用 (General)

| 级别 | 关键词 | 严重度 | 浅色 | 深色 |
|------|--------|--------|------|------|
| ERROR | ERROR | 0 | #A32D2D | #F09595 |
| WARN | WARN | 1 | #854F0B | #EF9F27 |
| INFO | INFO | 2 | #0C447C | #85B7EB |
| DEBUG | DEBUG | 3 | #3B6D11 | #97C459 |

### 3.2 Java (Log4j/SLF4J)

| 级别 | 关键词 | 严重度 | 浅色 | 深色 |
|------|--------|--------|------|------|
| FATAL | FATAL | 0 | #CC2D26 | #FF6B63 |
| ERROR | ERROR | 1 | #A32D2D | #F09595 |
| WARN | WARN | 2 | #854F0B | #EF9F27 |
| INFO | INFO | 3 | #0C447C | #85B7EB |
| DEBUG | DEBUG | 4 | #3B6D11 | #97C459 |
| TRACE | TRACE | 5 | #5F5E5A | #B4B2A9 |

### 3.3 Python (logging)

| 级别 | 关键词 | 严重度 | 浅色 | 深色 |
|------|--------|--------|------|------|
| CRITICAL | CRITICAL | 0 | #CC2D26 | #FF6B63 |
| ERROR | ERROR | 1 | #A32D2D | #F09595 |
| WARNING | WARNING | 2 | #854F0B | #EF9F27 |
| INFO | INFO | 3 | #0C447C | #85B7EB |
| DEBUG | DEBUG | 4 | #3B6D11 | #97C459 |

### 3.4 PHP (error_log)

| 级别 | 关键词 | 严重度 | 浅色 | 深色 |
|------|--------|--------|------|------|
| ALERT | ALERT | 0 | #CC2D26 | #FF6B63 |
| ERROR | ERROR | 1 | #A32D2D | #F09595 |
| WARNING | WARNING | 2 | #854F0B | #EF9F27 |
| NOTICE | NOTICE | 3 | #0C447C | #85B7EB |
| INFO | INFO | 4 | #3B6D11 | #97C459 |
| DEBUG | DEBUG | 5 | #5F5E5A | #B4B2A9 |

### 3.5 Go (slog/zerolog)

| 级别 | 关键词 | 严重度 | 浅色 | 深色 |
|------|--------|--------|------|------|
| ERROR | ERROR | 0 | #A32D2D | #F09595 |
| WARN | WARN | 1 | #854F0B | #EF9F27 |
| INFO | INFO | 2 | #0C447C | #85B7EB |
| DEBUG | DEBUG | 3 | #3B6D11 | #97C459 |

### 3.6 Rust (tracing)

| 级别 | 关键词 | 严重度 | 浅色 | 深色 |
|------|--------|--------|------|------|
| ERROR | ERROR | 0 | #A32D2D | #F09595 |
| WARN | WARN | 1 | #854F0B | #EF9F27 |
| INFO | INFO | 2 | #0C447C | #85B7EB |
| DEBUG | DEBUG | 3 | #3B6D11 | #97C459 |
| TRACE | TRACE | 4 | #5F5E5A | #B4B2A9 |

### 3.7 syslog

| 级别 | 关键词 | 严重度 | 浅色 | 深色 |
|------|--------|--------|------|------|
| EMERG | EMERG | 0 | #CC2D26 | #FF6B63 |
| ALERT | ALERT | 1 | #D4421E | #FF8A65 |
| CRIT | CRIT | 2 | #A32D2D | #F09595 |
| ERR | ERR | 3 | #854F0B | #EF9F27 |
| WARNING | WARNING | 4 | #0C447C | #85B7EB |
| NOTICE | NOTICE | 5 | #3B6D11 | #97C459 |
| INFO | INFO | 6 | #5F5E5A | #B4B2A9 |
| DEBUG | DEBUG | 7 | #5F5E5A | #B4B2A9 |

## 4. 颜色系统

### 4.1 预设色板

提供 16 个预设颜色，分深色/浅色两套：

| 色系 | 浅色 (bg/text) | 深色 (bg/text) |
|------|----------------|----------------|
| 红-1 | #FFEBE9 / #CC2D26 | #3a1412 / #FF6B63 |
| 红-2 | #FCEBEB / #A32D2D | #2e1515 / #F09595 |
| 橙-1 | #FDE8D0 / #CC5500 | #3a2010 / #FFB340 |
| 橙-2 | #FAEEDA / #854F0B | #2b1f08 / #EF9F27 |
| 黄-1 | #FFF8E1 / #8B6914 | #332b05 / #FFD600 |
| 黄-2 | #FFF3CD / #664D03 | #2b2200 / #FFE066 |
| 绿-1 | #E6F4E0 / #2E7D32 | #0d2818 / #66BB6A |
| 绿-2 | #EAF3DE / #3B6D11 | #152108 / #97C459 |
| 青-1 | #E0F7FA / #00695C | #0a2a2a / #4DD0E1 |
| 青-2 | #E0F2F1 / #004D40 | #0d2929 / #26A69A |
| 蓝-1 | #E3F2FD / #0C447C | #091e35 / #85B7EB |
| 蓝-2 | #E8EAF6 / #283593 | #111638 / #7986CB |
| 紫-1 | #F3E5F5 / #6A1B9A | #1e0a2e / #CE93D8 |
| 紫-2 | #EDE7F6 / #4527A0 | #170d2e / #B39DDB |
| 灰-1 | #F5F5F5 / #616161 | #242420 / #B4B2A9 |
| 灰-2 | #ECEFF1 / #455A64 | #1c2028 / #90A4AE |

### 4.2 颜色存储格式

每个级别颜色存储为浅色/深色各一个 HEX 值：

```json
{
  "color_light": "#A32D2D",
  "color_dark": "#F09595"
}
```

## 5. 数据模型

### 5.1 级别配置 (LogLevelConfig)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLevelConfig {
    /// 预设名称 ("general" | "java" | "python" | "php" | "go" | "rust" | "syslog" | "custom")
    pub preset: String,
    /// 级别列表，顺序 = 检测优先级
    pub levels: Vec<LevelDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelDef {
    /// 级别名称 (如 "ERROR", "CRITICAL")
    pub name: String,
    /// 检测关键词 (如 ["ERROR", "ERR"])
    pub keywords: Vec<String>,
    /// 浅色主题颜色 (HEX)
    pub color_light: String,
    /// 深色主题颜色 (HEX)
    pub color_dark: String,
}
```

### 5.2 配置持久化（集成到 config.toml）

路径: `~/.config/tailr/config.toml` 的 `[log_levels]` 节

```toml
[log_levels]
preset = "python"

[[log_levels.level]]
name = "CRITICAL"
keywords = ["CRITICAL"]
color_light = "#CC2D26"
color_dark = "#FF6B63"

[[log_levels.level]]
name = "ERROR"
keywords = ["ERROR"]
color_light = "#A32D2D"
color_dark = "#F09595"
```
```

## 6. 架构设计

### 6.1 后端改动

#### 6.1.1 protocol crate

- `LogLevel` 枚举保留（向后兼容），新增 `LevelDef` 和 `LogLevelConfig` 数据结构
- 不引入新依赖，保持零依赖定位

#### 6.1.2 src/config.rs（已有文件扩展）

扩展 `Config` 结构体，新增 `log_levels` 字段：

```rust
pub struct Config {
    pub log: Vec<PathBuf>,
    pub bind: String,
    pub daemon: DaemonConfig,
    pub log_levels: Option<LogLevelConfig>,  // 新增
}
```

新增函数：
- `write_config(path, config)` — 序列化 Config → TOML → 覆盖写入
- `default_log_levels(preset)` — 返回指定预设的默认 LogLevelConfig

预设数据也放在 `config.rs` 中（单一数据源）。

#### 6.1.3 search-engine crate

新增 `LevelDetector`（`crates/search-engine/src/detector.rs`）：

```rust
/// 动态级别检测器（关键词匹配模式）
pub struct LevelDetector {
    levels: Vec<CompiledLevel>,
}

struct CompiledLevel {
    name: String,
    keywords: Vec<String>,  // 大小写不敏感关键词
}

impl LevelDetector {
    pub fn from_config(config: &LogLevelConfig) -> Self;
    pub fn detect(&self, line: &str) -> String;  // 返回级别名称
}
```

#### 6.1.3 server crate

新增 API 端点：

| 路由 | 方法 | 说明 |
|------|------|------|
| `/api/config/log-levels` | GET | 获取当前级别配置 |
| `/api/config/log-levels` | POST | 保存级别配置（热更新 + 写入 config.toml） |

`AppState` 新增字段（使用 `arc-swap` 实现无锁读）：

```rust
use arc_swap::ArcSwap;

pub struct AppState {
    // ... 现有字段
    level_config: Arc<ArcSwap<LogLevelConfig>>,
    level_detector: Arc<ArcSwap<LevelDetector>>,
    config_path: PathBuf,  // config.toml 路径，用于写回
}
```

POST 处理流程：
1. 反序列化请求体为 `LogLevelConfig`
2. `level_detector.store(Arc::new(LevelDetector::from_config(&config)))` — 热更新
3. `level_config.store(Arc::new(config))` — 更新内存
4. 读取当前完整 Config → 更新 `log_levels` → `write_config()` 覆盖写入 config.toml

所有调用 `detect_level()` 的地方改为从 `AppState` 获取检测器：
- `api.rs` 的 `search()` 
- `session.rs` 的 `read_lines_from_offset()`
- `ws.rs` 的 `handle_subscribe()` 中的条目处理

#### 6.1.4 tail-engine crate

- `FileWatcher` 持有 `Arc<ArcSwap<LevelDetector>>` 引用
- `add()` 时传入检测器引用
- `TailSession` 持有检测器的 `Arc` clone（零成本）
- `read_lines_from_offset()` 使用动态检测器替代硬编码 `detect_level()`

### 6.2 前端改动

#### 6.2.1 新增配置页面组件

```
src/components/
  settings/
    SettingsPage.vue          # 全屏模态框，左侧分类导航
    GeneralSettings.vue       # 外观设置（字号、主题、语言、自动滚动）
    LogLevelSettings.vue      # 日志级别配置
    LogFormatSettings.vue     # 日志格式配置（预留）
```

#### 6.2.2 LogLevelSettings.vue 布局

```
┌──────────────────────────────────────┐
│  ⚙️ 设置                        ✕   │
├──────────┬───────────────────────────┤
│          │                           │
│  🎨 外观  │  预设: [Python ▾]         │
│          │                           │
│  📋 日志级别│  ───────────────────────  │
│  ← 当前  │  ⠿ CRITICAL  [________] 🔴 │
│          │  ⠿ ERROR     [________] 🔴 │
│  📐 格式  │  ⠿ WARNING   [________] 🟡 │
│          │  ⠿ INFO      [________] 🔵 │
│  ⚡ 其他  │  ⠿ DEBUG     [________] 🟢 │
│          │                           │
│          │  [+ 添加级别]              │
│          │                           │
│          │  [保存]  [重置为默认]       │
└──────────┴───────────────────────────┘
```

每个级别行：
- `⠿` 拖拽手柄（调整顺序 = 调整检测优先级）
- 级别名称（可编辑）
- 关键词输入（逗号分隔）
- 颜色点（点击打开色板选择器）
- 删除按钮（`×`）

#### 6.2.3 颜色应用方式

将硬编码的 CSS 变量和颜色替换为动态生成：

```typescript
// store/logLevels.ts
export const useLogLevelStore = defineStore('logLevels', () => {
  const config = ref<LogLevelConfig>(loadFromLocalStorage())

  // 动态生成 CSS 变量
  function applyThemeColors(isDark: boolean) {
    const root = document.documentElement
    config.value.levels.forEach(level => {
      const color = isDark ? level.color_dark : level.color_light
      root.style.setProperty(`--c-${level.name.toLowerCase()}-text`, color)
    })
  }

  return { config, applyThemeColors }
})
```

#### 6.2.4 前端 API 客户端

```typescript
// services/api.ts 新增
export async function getLogLevelConfig(): Promise<LogLevelConfig> { ... }
export async function saveLogLevelConfig(config: LogLevelConfig): Promise<void> { ... }
```

#### 6.2.5 预设数据

预设定义放在前端：

```typescript
// composables/usePresets.ts
export const PRESETS: Record<string, LevelDef[]> = {
  general: [
    { name: 'ERROR', keywords: ['ERROR'], color_light: '#A32D2D', color_dark: '#F09595' },
    { name: 'WARN',  keywords: ['WARN'],  color_light: '#854F0B', color_dark: '#EF9F27' },
    { name: 'INFO',  keywords: ['INFO'],  color_light: '#0C447C', color_dark: '#85B7EB' },
    { name: 'DEBUG', keywords: ['DEBUG'], color_light: '#3B6D11', color_dark: '#97C459' },
  ],
  java: [ /* ... */ ],
  python: [ /* ... */ ],
  // ...
}
```

## 7. 数据流

```
用户在 UI 修改配置
       │
       ▼
  前端更新 localStorage（UI 立即生效）
       │
       ▼
  POST /api/config/log-levels
       │
       ▼
  后端更新 LevelDetector（arc-swap 原子替换）
       │
       ▼
  序列化完整 Config → 覆盖写入 ~/.config/tailr/config.toml
       │
       ▼
  下次启动 figment 自动加载
```

检测流程（每行日志）：

```
日志原始文本
       │
       ▼
  LevelDetector::detect(line)
       │
       ▼
  按数组顺序逐个级别检测：
  对每个级别，遍历关键词列表，
  用 contains_case_insensitive 匹配
       │
       ▼
  匹配到 → 返回该级别名称
  无匹配 → 返回 "UNKNOWN"
```

## 8. 向后兼容

1. **保留原有 `LogLevel` 枚举**：内部使用，不删除
2. **保留原有 `detect_level()` 函数**：作为默认检测器（无配置时使用）
3. **JSON API 格式不变**：`LogEntry.level` 仍返回字符串
4. **WebSocket 协议不变**：现有客户端无需修改
5. **升级迁移**：config.toml 无 `[log_levels]` 节 → `Option<None>` → 自动用"通用"预设
6. **figment 分层**：defaults < config.toml < env vars < CLI args，新字段 `#[serde(default)]` 向后兼容

## 9. 测试计划

### 9.1 后端测试

- `LevelDetector::from_config()` 测试：验证预设加载
- `LevelDetector::detect()` 测试：关键词匹配、优先级、UNKNOWN 兜底
- API 端点测试：GET/POST 配置的读写
- 热更新测试：POST 后检测器立即生效

### 9.2 前端测试

- 预设切换测试：切换预设后级别列表正确更新
- 颜色保留测试：切换预设后已修改的颜色不丢失
- 拖拽排序测试：拖拽后顺序正确保存
- 持久化测试：刷新页面后配置正确恢复
- 色板选择测试：点击颜色点正确应用

## 10. 未来扩展

1. **正则表达式支持（Phase 2）**：每个级别支持关键词或正则二选一，UI 上增加 toggle
2. **LOG_PATTERN 支持**：Pattern 优先，Keyword 兜底
3. **导入/导出配置**：用户可以分享自己的级别配置
4. **按文件配置**：不同日志文件使用不同的级别定义
