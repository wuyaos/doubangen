# doubangen · Agent 指南

> 本文件供 AI 编码助手在 doubangen 仓库内工作时遵循。人机协作以本文件 + 仓库现状为准。

## 项目定位

豆瓣简介生成器：egui 桌面 GUI，聚合 **豆瓣（DoubanInfo）** 与 **TMDB**，生成 BBCode / JSON 影视简介。豆瓣优先合并，TMDB 补充评分 / 多语言别名 / 演职员代表作。

## 技术栈

- Rust 2021 edition
- GUI：`eframe` / `egui` 0.29（`glow` + `x11` + `wayland`，禁用 `wgpu`/`accesskit` 减体积）
- 图片：`egui_extras`（`image`+`http` loader）+ `image` 0.25（jpeg/png，用于窗口图标）
- HTTP：`reqwest` 0.12 blocking + `rustls-tls`（全局连接池，见 `common::http_client`）
- 序列化：`serde` / `serde_json` / `toml` 0.8
- 统一数据表示：`serde_json::Value`（对齐 Python 原型的 dict 流）

## 模块职责

| 文件 | 职责 |
|------|------|
| `src/main.rs` | egui GUI：搜索状态机、海报墙、分类/年份/关键词过滤、结果复制、历史记录、设置面板、阻塞 worker 线程 |
| `src/common.rs` | 全局 HTTP Client（OnceLock 连接池）、`http_get_json`（429/网络重试）、TMDB `tmdb_get`、国家映射、`Candidate` |
| `src/doubaninfo.rs` | DoubanInfo 客户端：`search` / `detail` / 频控 `throttle`、`media_type` 推断、`douban_link` 补齐 |
| `src/tmdb.rs` | TMDB 客户端：电影/剧集搜索与详情、`find_by_imdb`、评分/海报/演职员/译名、`BaseData` struct 组装 |
| `src/schema.rs` | TV 检测、BBCode 渲染、豆瓣优先 `merge`、演职员合并、递归空字段 `clean_output` |
| `src/config.rs` | `config.toml` 反序列化（keys/defaults/search/merge/limits），`Keys` 需 `Clone` |
| `build.rs` | Windows `.exe` 图标嵌入：`CARGO_CFG_TARGET_OS` 判断 target，`windres`+`ar`→`+whole-archive` 链接 |

## 构建与验证

```bash
# Linux（需 X11/Wayland dev 依赖，见 README）
cargo build --release

# Windows 交叉编译（需 mingw-w64）
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

验证基线（须保持）：
- `cargo check` 0 警告
- `cargo clippy -- -D warnings` 0 问题
- 双平台 release 编译 exit 0
- 真实 API 冒烟：豆瓣 sid=30433456 rating=7.5；TMDB id=575265 rating=7.175（需有效 `config.toml`）

## 关键约定

### 数据流
搜索 → `Vec<Candidate>`（海报墙） → 选中 → `detail` 取 `Value` → `schema::merge`（merge 模式） → `schema::render_bbcode` / JSON → 结果区 + 历史

### merge 优先级
豆瓣为主体，TMDB 仅补充缺失字段：`enrich_tmdb_rating` / `enrich_tmdb_aka` / `enrich_celebrities`。豆瓣 `full_celebrities` 优先，TMDB 兜底。

### 下拉中文
`source`/`format`/`media_type` 的**值保持英文**（供逻辑判断），显示用 `cn_source`/`cn_format`/`cn_media` 映射中文。

### egui 0.29 API
- `ComboBox::from_id_salt`（非已废弃的 `from_id_source`）
- `Stroke::new(1.0_f32, color)`（避免 f64 fallback 警告）
- `Image::new("url")` 直接传 `&str`
- 0.29 无 `widgets.highlighted` 字段

### 主题
Catppuccin Mocha：`panel_fill`/`window_fill` = base `#1e1e2e`，控件 surface1，悬停 surface2 + amber 描边，selection/超链接 = amber `#F5A623`。设置面板用扁平 `separator` 分隔，不用 `Frame::group`（避免边框色偏差）。

### 配置与密钥
- `config.toml` 含 API Key，**严禁提交**（已 `.gitignore`）
- 提交 `config.example.toml` 脱敏模板
- 运行时在可执行文件同目录读 `config.toml`，设置面板保存即时写回

### Windows 图标
`build.rs` 必须用 `CARGO_CFG_TARGET_OS` 环境变量判断 target，**不可用 `cfg!(target_os)`**（build script 编译为 host 二进制，检查的是 host 而非 target，交叉编译时恒 false）。验证嵌入：`.exe` 的 `.rsrc` section offset 非 -1。

## 编码与提交

- 最小变更集：只改目标及其直接依赖；未要求不重构测试/CI/文档
- 命名表达具体业务对象，避免泛名
- 注释解释 WHY（意图/约束/取舍），不复述 WHAT
- 完成态表述须有文件/产物/工具结果支撑，不把预期当结论
- 未授权运行验证时，仅做静态核查（语法/类型/结构/diff）
- 提交信息：`feat: ...` / `fix: ...` / `refactor: ...`，中文描述

## 注意事项

- `target/` 体积大（3G+），跨盘移动项目时排除，按需重编
- 推送前确认 `git status` 不含 `config.toml` / `history.json` / `target/`
- WSL 下编译 Windows 单作业（`CARGO_BUILD_JOBS=1`）避免内存峰值 OOM
- 交叉编译需 `x86_64-w64-mingw32-windres` 与 `x86_64-w64-mingw32-ar` 在 PATH
