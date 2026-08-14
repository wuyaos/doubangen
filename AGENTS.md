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
- 配置目录：`dirs = "5"`（跨平台定位用户配置目录）
- 统一数据表示：`serde_json::Value`（对齐 Python 原型的 dict 流）

## 模块职责

| 文件 | 职责 |
|------|------|
| `src/main.rs` | egui GUI：搜索状态机、海报墙、分类/年份/关键词过滤、结果复制、历史记录、设置面板（RadioButton + 自定义头部）、配置位置管理（`ensure_config`/`switch_config_location`/`open_config_dir`）、阻塞 worker 线程 |
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
- 双平台 release 编译 exit 0（Linux ~6.9M / Windows ~5.3M）
- Windows `.exe` `.rsrc` section offset 非 -1（图标嵌入，当前 752）
- 真实 API 冒烟：豆瓣 sid=30433456 rating=7.5；TMDB id=575265 rating=7.175（需有效 `config.toml`）

## 关键约定

### 数据流
搜索 → `Vec<Candidate>`（海报墙） → 选中 → `detail` 取 `Value` → `schema::merge`（merge 模式） → `schema::render_bbcode` / JSON → 结果区 + 历史

### merge 优先级
豆瓣为主体，TMDB 仅补充缺失字段：`enrich_tmdb_rating` / `enrich_tmdb_aka` / `enrich_celebrities`。豆瓣 `full_celebrities` 优先，TMDB 兜底。

### 中文显示
`source`/`format` 的**值保持英文**（供逻辑判断），显示用 `cn_source`/`cn_format` 映射中文。设置面板默认行为改 `ui.radio_value`（○ 自动 ○ 豆瓣 …），值英文 label 中文；顶部 source 仍用 ComboBox + `cn_source`。

### 配置文件管理
- **初次运行**：`ensure_config` 无配置时自动生成 `config.toml`（用 `include_str!("../config.example.toml")` 模板，含注释空 key），程序目录优先 → 不可写回退用户目录
- **查找顺序**：`find_config_path` 先程序目录（`app_config_path` = exe 同目录）→ 再用户目录（`user_config_path` = `dirs::config_dir()/doubangen`）
- **切换位置**：`switch_config_location` **复制原配置文件原始内容（保留注释）到新位置 + 删除旧文件**，`history.json` 一并迁移；不可用草稿重建（会丢注释）
- **打开目录**：`open_config_dir` 调系统命令（Windows `explorer` / Linux `xdg-open` / macOS `open`）
- `config.toml` 含 API Key，**严禁提交**（已 `.gitignore`）；提交 `config.example.toml` 脱敏模板

### 设置面板
- `Window::title_bar(false)` + 自定义头部（标题 strong + × 关闭），与主界面扁平风格一致；不用 `Frame::group`（避免边框色偏差），用 `separator` 分节
- `settings_draft` 为持久草稿：打开设置时克隆一次 `cfg`，编辑跨帧保留；保存成功才关闭，失败保留窗口并提示
- 「配置文件」段：位置 radio（程序目录/用户目录）切换触发 `switch_config_location` + 「📁 打开目录」按钮 + 当前路径显示

### egui 0.29 API
- `ComboBox::from_id_salt`（非已废弃的 `from_id_source`）
- `Window::title_bar(false)` 去默认标题栏自定义头部
- `ui.radio_value(&mut String, val.into(), label)` 单选（值需 `PartialEq`）
- `Stroke::new(1.0_f32, color)`（避免 f64 fallback 警告）
- `Image::new("url")` 直接传 `&str`
- `RichText` **无** `wrap_mode` 方法，换行用 `Label::new(rich).wrap_mode(egui::TextWrapMode::Wrap)`
- 0.29 无 `widgets.highlighted` 字段

### 主题
Catppuccin Mocha：`panel_fill`/`window_fill` = base `#1e1e2e`，控件 surface1，悬停 surface2 + amber 描边，selection/超链接 = amber `#F5A623`。`widgets.open`/`window_stroke` 须显式覆盖（`dark()` 默认残留靛蓝色）。

### Windows 图标
`build.rs` 必须用 `CARGO_CFG_TARGET_OS` 环境变量判断 target，**不可用 `cfg!(target_os)`**（build script 编译为 host 二进制，检查的是 host 而非 target，交叉编译时恒 false）。流程：`windres` .rc→.o → `ar` rcs→libiconres.a → `cargo:rustc-link-lib=static:+whole-archive=iconres`。

### CI / Release
- `.github/workflows/build.yml`：push/PR 编译双平台 artifact；tag `v*` 触发 release job
- **Release 直接发布可执行文件**（不 zip、不含 config）：`doubangen-{ver}-{os}-x86_64[.exe]`，首次运行自动生成 `config.toml`
- Windows 用 ubuntu 交叉编译 `x86_64-pc-windows-gnu`（复用本地验证流程，避免 MSVC 无 windres）

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
- WSL 下编译 Windows 单作业（`CARGO_BUILD_JOBS=1`）避免内存峰值 OOM；Linux 亦可能 OOM（xml-rs），必要时单作业
- 交叉编译需 `x86_64-w64-mingw32-windres` 与 `x86_64-w64-mingw32-ar` 在 PATH
- `gh` CLI 已认证为 `wuyaos`，可用 `gh repo create` / `gh run watch` / `gh release view`
